//! Pipeline unificado de carga de mods — "6 serem 1" no runtime do BWMS.
//!
//! As 6 capacidades do Windows convergem aqui em 2 fases:
//!
//!   RED4ext    → plugins Rust de 3os (boot_phase)
//!   redscript  → compilado pela toolchain; motor carrega automaticamente
//!   CET        → overlay/console (overlay.rs, sempre ativo)
//!   TweakXL    → YAML auto-apply dos mods ativos + ScriptableTweak dispatch (session_phase)
//!   Codeware   → register::register_all (selfboot on_load — antes desta fase)
//!   ArchiveXL  → resource.link + factory index (boot_phase)
//!
//! Chamadas de lib.rs:
//!   `boot_phase()`       — primeira iteração do tick, antes do gate de player
//!   `session_phase(reg)` — no timing do Session/Start (player presente, RTTI completo)

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static BOOT_DONE: AtomicBool = AtomicBool::new(false);

// `tweakxl-hot-reload`: polling a cada HOT_RELOAD_INTERVAL ticks (~10s).
static HOT_RELOAD_LAST_TICK: AtomicU64 = AtomicU64::new(0);
static HOT_RELOAD_LAST_MTIME: AtomicU64 = AtomicU64::new(0);
const HOT_RELOAD_INTERVAL: u64 = 600;

/// **Fase 1** — roda UMA VEZ na primeira iteração do tick (antes do gate de player).
///
/// - ArchiveXL: resource.link (bwms-reslink.txt gerado pelo `bwms install`)
/// - ArchiveXL: factory index (bwms-factories.txt gerado pelo `bwms install`)
/// - RED4ext: plugins Rust de 3os (red4ext/plugins/*.dylib, opt-in)
/// - Scan inicial da aba Mods do overlay
pub(crate) fn boot_phase() {
    if BOOT_DONE.swap(true, Ordering::Relaxed) {
        return;
    }

    reload_archivexl_tables();

    // --- RED4ext: plugins Rust de 3os ---
    // Carrega .dylib em red4ext/plugins/ — sem plugins = zero impacto.
    if let Some(red4) = parent_of_mods_dir() {
        crate::plugins::load_plugins(&red4.join("plugins"));
    }

    // Scan inicial de mods (popula a aba Mods do overlay). Guarda assíncrono.
    crate::mod_scan::refresh_async();
}

/// `ArchiveXL.Reload()` (`PENDENCIAS-UNIFICADAS.md`, Facade.hpp) — re-lê as 3 tabelas geradas
/// pelo `bwms install` (`bwms-reslink.txt`/`bwms-factories.txt`/`bwms-patches.txt`) SEM precisar
/// rebootar o jogo. Extraído de `boot_phase()` (que chamava isso 1x, atrás do guard `BOOT_DONE`)
/// pra ficar reusável tanto no boot quanto num reload manual — os 3 sub-passos já eram
/// IDEMPOTENTES por design (`install_reslink`/`install_factory_hook`/`install_resolveresource_probe`
/// têm guarda própria de "só instala o hook 1x"; `reslink_file`/`factory_file`/`patch_target_file`
/// só REPOVOAM a tabela de lookup em memória a partir do arquivo, sem efeito colateral de
/// instalação repetida) — mesmo padrão de reuso seguro que fechou `TweakXL.Reload`.
pub(crate) fn reload_archivexl_tables() {
    // --- ArchiveXL: resource.link ---
    // bwms-reslink.txt é gerado por bwms-core::apply_xl::emit_reslink a partir das
    // seções `resource.link` dos .xl ativos. O hook é idempotente (só instala 1x).
    if let Some(red4) = parent_of_mods_dir() {
        let f = red4.join("bwms-reslink.txt");
        if f.is_file() {
            unsafe { crate::selftest::install_reslink() };
            crate::selftest::reslink_file(&f.to_string_lossy());
            crate::log("[pipeline] ArchiveXL resource.link: tabela carregada");
        }
    }

    // --- ArchiveXL: factory index ---
    // bwms-factories.txt é gerado por bwms-core::apply::write_factory_table a partir
    // das seções `factories:` dos .xl ativos.
    if let Some(red4) = parent_of_mods_dir() {
        let f = red4.join("bwms-factories.txt");
        if f.is_file() {
            unsafe { crate::selftest::install_factory_hook() };
            unsafe { crate::selftest::install_resolveresource_probe() };
            crate::selftest::factory_file(&f.to_string_lossy());
            crate::log("[pipeline] ArchiveXL factory index: hook instalado");
        }
    }

    // --- ArchiveXL: resource.patch targets ---
    // bwms-patches.txt gerado por `bwms install` a partir das seções `resource.patch` dos .xl ativos.
    if let Some(red4) = parent_of_mods_dir() {
        let f = red4.join("bwms-patches.txt");
        if f.is_file() {
            crate::selftest::patch_target_file(&f.to_string_lossy());
        }
    }
}

/// **Fase 2** — roda no timing do Session/Start (player presente, RTTI completo).
///
/// - TweakXL: ScriptableTweak dispatch (OnApply() em cada subclasse concreta)
/// - TweakXL: auto-apply dos .yaml em BWMS/mods/<tema>/<mod>/r6/tweaks/
pub(crate) unsafe fn session_phase(reg: &crate::rtti::Registry) {
    // --- TweakXL: ScriptableTweak dispatch ---
    dispatch_scriptable_tweaks(reg);

    // --- TweakXL: auto-apply YAML dos mods ativos ---
    apply_mod_tweaks();
}

// ── Funções internas ──────────────────────────────────────────────────────────

/// Acha a pasta red4ext/ (pai de blackwall-mods/) de forma portável.
fn parent_of_mods_dir() -> Option<std::path::PathBuf> {
    std::path::Path::new(&crate::mods_dir())
        .parent()
        .map(|p| p.to_path_buf())
}

/// Raiz do jogo (2 níveis acima da nossa dylib: red4ext/ → <jogo>/).
fn game_root() -> Option<std::path::PathBuf> {
    crate::dylib_dir()
        .map(std::path::PathBuf::from)
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

/// Anda `dir` recursivamente empilhando subpastas (mesmo padrão de `bwms-core/src/apply.rs::
/// collect_under`) — mods reais quase sempre namespaceiam `r6/tweaks/<autor>/<mod>/*.yaml`
/// (ex.: o próprio teste deste projeto usa `r6/tweaks/omaha/*.yaml`), então um `read_dir` raso
/// nunca via esses arquivos.
fn walk_tweaks_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            match e.file_type() {
                Ok(ft) if ft.is_dir() => stack.push(p),
                Ok(ft) if ft.is_file() => out.push(p),
                _ => {}
            }
        }
    }
}

/// Auto-aplica todos os .yaml/.yml em BWMS/mods/<tema>/<mod>/r6/tweaks/ (recursivo — ver
/// `walk_tweaks_files`) para cada mod ativo no staging.
///
/// 2026-08-05 (achado de auditoria — bug real corrigido): (a) o scan era raso (`read_dir` direto,
/// sem recursão), perdendo qualquer `.yaml` dentro de subpasta — o caso comum na prática; (b)
/// `.tweak` (a DSL declarativa do TweakXL, formato diferente de YAML — parser PEGTL-like,
/// NUNCA implementado neste projeto) era silenciosamente aceito por `classify.rs`/`apply.rs`
/// (que o staged pro `r6/tweaks/`) mas nunca lido aqui — um mod `.tweak` "instalava" sem erro
/// nenhum e nunca fazia efeito algum. Agora ele é DETECTADO e logado alto (não aplicado — o
/// parser ainda não existe — mas pelo menos diagnosticável em vez de falha muda).
pub(crate) fn apply_mod_tweaks() {
    let Some(root) = game_root() else { return };
    let staging = root.join("BWMS/mods");

    let active = crate::mod_scan::get()
        .into_iter()
        .filter(|m| m.active)
        .collect::<Vec<_>>();
    if active.is_empty() {
        return;
    }

    let mut total = 0usize;
    let mut unsupported_tweak = 0usize;
    for m in &active {
        let tweaks_dir = staging.join(&m.theme).join(&m.name).join("r6/tweaks");
        if !tweaks_dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        walk_tweaks_files(&tweaks_dir, &mut files);
        for p in files {
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            if ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml") {
                crate::log(&format!("[pipeline] TweakXL: {}", p.display()));
                crate::tweakdb_rt::apply_xl_file_auto(&p.to_string_lossy());
                total += 1;
            } else if ext.eq_ignore_ascii_case("tweak") {
                crate::log(&format!(
                    "[pipeline] TweakXL AVISO: {} é formato .tweak (DSL declarativa) — parser ainda NÃO existe neste projeto, arquivo NÃO aplicado (mod {}/{})",
                    p.display(), m.theme, m.name
                ));
                unsupported_tweak += 1;
            }
        }
    }
    if total > 0 {
        crate::log(&format!(
            "[pipeline] TweakXL: {total} arquivo(s) aplicado(s) de {} mod(s)",
            active.len()
        ));
    }
    if unsupported_tweak > 0 {
        crate::log(&format!(
            "[pipeline] TweakXL: {unsupported_tweak} arquivo(s) .tweak IGNORADO(S) (formato não suportado)"
        ));
    }
}

/// Enumera todas as subclasses concretas de ScriptableTweak no RTTI e chama
/// `OnApply()` em cada uma. Roda no timing de Session/Start.
unsafe fn dispatch_scriptable_tweaks(reg: &crate::rtti::Registry) {
    use std::ffi::c_void;
    let st_cname = crate::cname::cname("ScriptableTweak");
    if reg.class_by_name("ScriptableTweak").is_null() {
        crate::log("[tweakxl] ScriptableTweak não no RTTI — dispatch pulado");
        return;
    }

    // GetAllTypes(this, DynArray<IRTTIType*>&) — slot +0x40 (provável; GetType@+0x38).
    // Se errado: log imprime count inválido e nada é chamado (sem crash).
    const GET_ALL_TYPES_SLOT: usize = 0x40;
    let fn_ptr = reg.vtbl_slot(GET_ALL_TYPES_SLOT);
    if fn_ptr.is_null() {
        crate::log("[tweakxl] GetAllTypes@vtbl+0x40 ilegível");
        return;
    }

    // DynArray<IRTTIType*>: ptr(u64)@0x00 + capacity(u32)@0x08 + size(u32)@0x0C (layout real,
    // confirmado por disassembly ARM64 em 2026-08-11 — comentário antigo tinha size/cap
    // trocados). Ler `capacity` aqui (buf[8..12], como sempre foi) continua CORRETO porque este
    // slot é GetNativeTypes/GetAllTypes especificamente, a ÚNICA família sem filtro — cap==size
    // sempre pra ela (confirmado: 27798==27798). Ver `rtti.rs::call_dynarray_out0` pro fix real
    // (que passou a ler `size`), necessário pras famílias COM filtro (`GetClasses` etc.), onde
    // cap≠size.
    let mut arr = [0u8; 16];
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) = std::mem::transmute(fn_ptr);
    f(reg.reg, arr.as_mut_ptr() as *mut c_void);

    let entries_raw = u64::from_le_bytes(arr[0..8].try_into().unwrap_or([0u8; 8]));
    let count = u32::from_le_bytes(arr[8..12].try_into().unwrap_or([0u8; 4])) as usize;
    let entries = entries_raw as *const *mut c_void;

    if count < 100 || count > 300_000 || entries.is_null()
        || !crate::gum::is_readable(entries as *const c_void, 8)
    {
        crate::log(&format!(
            "[tweakxl] GetAllTypes slot+0x40 inválido (count={count} ptr={entries_raw:#x}) — slot errado?"
        ));
        if crate::dev_mode() {
            for probe_off in [0x38usize, 0x48, 0x50, 0x58] {
                let pfp = reg.vtbl_slot(probe_off);
                if pfp.is_null() {
                    continue;
                }
                let mut pa = [0u8; 16];
                let pf: unsafe extern "C" fn(*mut c_void, *mut c_void) = std::mem::transmute(pfp);
                pf(reg.reg, pa.as_mut_ptr() as *mut c_void);
                let pc = u32::from_le_bytes(pa[8..12].try_into().unwrap_or([0u8; 4])) as usize;
                let pe = u64::from_le_bytes(pa[0..8].try_into().unwrap_or([0u8; 8]));
                crate::log(&format!(
                    "[tweakxl] probe vtbl+{probe_off:#x}: count={pc} ptr={pe:#x}"
                ));
            }
        }
        return;
    }
    crate::log(&format!(
        "[tweakxl] GetAllTypes: {count} tipos, varrendo ScriptableTweak"
    ));

    let mut dispatched = 0u32;
    for i in 0..count {
        let ty = *entries.add(i);
        if ty.is_null() || !crate::gum::is_readable(ty as *const c_void, 0x80) {
            continue;
        }
        let clsb = ty as *const u8;

        // CClass+0x70 = flags; bit0 = isAbstract → skip
        let flags = (clsb.add(0x70) as *const u32).read_unaligned();
        if flags & 1 != 0 {
            continue;
        }

        // Verifica ancestral: CClass+0x10=parent, +0x18=name(CName hash)
        let self_name = (clsb.add(0x18) as *const u64).read_unaligned();
        if self_name == st_cname {
            continue; // skip a própria base
        }
        let mut parent = (clsb.add(0x10) as *const *mut u8).read_unaligned();
        let mut found = false;
        let mut depth = 0usize;
        while !parent.is_null() && depth < 16 {
            depth += 1;
            if !crate::gum::is_readable(parent as *const c_void, 0x20) {
                break;
            }
            let pname = (parent.add(0x18) as *const u64).read_unaligned();
            if pname == st_cname {
                found = true;
                break;
            }
            parent = (parent.add(0x10) as *const *mut u8).read_unaligned();
        }
        if !found {
            continue;
        }

        // IType::GetName() em vtbl+0x10 → CName hash → string
        let name_str = {
            let vt = (ty as *const *const u8).read_unaligned();
            if vt.is_null() || !crate::gum::is_readable(vt as *const c_void, 0x18) {
                continue;
            }
            let get_name_fn = *(vt.add(0x10) as *const *const c_void);
            if get_name_fn.is_null() {
                continue;
            }
            let get_name: unsafe extern "C" fn(*mut c_void) -> u64 =
                std::mem::transmute(get_name_fn);
            crate::cname::resolve_cname(get_name(ty))
        };

        let obj = crate::rtti::new_object(reg, &name_str);
        if obj.is_null() {
            crate::log(&format!("[tweakxl] new_object falhou: {name_str}"));
            continue;
        }
        if let Some(rf) = crate::rtti::resolve_func(reg, &name_str, "OnApply") {
            crate::rtti::call_func(&rf, obj, &[]);
            dispatched += 1;
            crate::log(&format!("[tweakxl] OnApply → {name_str}"));
        }
        // obj leaked — ScriptableTweak instances são efêmeras
    }
    crate::log(&format!(
        "[tweakxl] dispatch_scriptable_tweaks: {dispatched} OnApply() chamados"
    ));
}

/// **TweakXL hot-reload** — roda a cada `HOT_RELOAD_INTERVAL` ticks em gameplay.
/// Varre o mtime máximo dos yamls ativos; se mudou desde a última checagem, re-aplica.
pub(crate) unsafe fn hot_reload_tick(reg: &crate::rtti::Registry) {
    let now = crate::ticks();
    let last = HOT_RELOAD_LAST_TICK.load(Ordering::Relaxed);
    if now.saturating_sub(last) < HOT_RELOAD_INTERVAL {
        return;
    }
    HOT_RELOAD_LAST_TICK.store(now, Ordering::Relaxed);

    let Some(root) = game_root() else { return };
    let staging = root.join("BWMS/mods");
    let new_mt = scan_tweaks_max_mtime(&staging);
    if new_mt == 0 {
        return;
    }
    let old_mt = HOT_RELOAD_LAST_MTIME.swap(new_mt, Ordering::Relaxed);
    if old_mt != 0 && new_mt != old_mt {
        crate::log("[tweakxl-hotreload] yaml modificado → re-apply tweaks + dispatch");
        apply_mod_tweaks();
        dispatch_scriptable_tweaks(reg);
    }
}

fn scan_tweaks_max_mtime(staging: &std::path::Path) -> u64 {
    let mut max: u64 = 0;
    let Ok(rd) = std::fs::read_dir(staging) else { return 0 };
    for theme in rd.flatten() {
        let Ok(mrd) = std::fs::read_dir(theme.path()) else { continue };
        for moddir in mrd.flatten() {
            let tweaks = moddir.path().join("r6/tweaks");
            let Ok(td) = std::fs::read_dir(&tweaks) else { continue };
            for e in td.flatten() {
                let p = e.path();
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
                if !ext.eq_ignore_ascii_case("yaml") && !ext.eq_ignore_ascii_case("yml") {
                    continue;
                }
                if let Ok(m) = std::fs::metadata(&p) {
                    if let Ok(mt) = m.modified() {
                        let secs = mt
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        if secs > max {
                            max = secs;
                        }
                    }
                }
            }
        }
    }
    max
}
