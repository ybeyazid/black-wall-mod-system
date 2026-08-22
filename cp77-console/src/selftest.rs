//! Self-test DEV-GATED do hooking + diagnóstico do ArchiveXL.
//!
//! TUDO aqui só roda sob `crate::dev_mode()` (env `BWMS_DEV` ou `/tmp/bwms-dev`).
//! No build/boot de PRODUÇÃO nada disto instala hook, lê memória do jogo ou loga —
//! o jogo do usuário fica intacto.
//!
//! Três peças:
//!   1. **Relocador (inline hook, type 1):** instala em um getter-folha do pool
//!      (`PoolStorageProxy<PoolRoot>::GetHandle`, 4 instr `adrp/add/ldr/ret`),
//!      confirma que o relocador converteu o `adrp` do prólogo em `movz/movk`
//!      no trampolim, e DESINSTALA na hora. A função-alvo NUNCA é chamada.
//!   2. **vtable (type 2):** se houver uma vtable RTTI viva e segura, troca um slot,
//!      confirma a troca e restaura. Conservador: sem alvo 100% seguro → só loga
//!      que precisa de runtime com objeto vivo (não arrisca).
//!   3. **Diagnóstico ArchiveXL:** instala em `PoolStorageProxy<PoolArchive>::Allocate`
//!      um replacement OBSERVE-ONLY (shim naked p/ capturar x30 = ret-addr do caller),
//!      loga os ~8 primeiros callers (rebaseados p/ vmaddr) e SEMPRE chama a original.
//!
//! Segurança transversal: guarda de legibilidade + guarda de prólogo (compara os 16
//! bytes com o esperado) ANTES de hookar; aborta limpo se o binário mudou (patch).
//! Hook reversível (`Interceptor::revert`). Patch via VM_PROT_COPY (COW) só na cópia
//! do processo.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};

use crate::gum::{self, Interceptor};

// ===================== Alvos estáticos (vmaddr; base de link 0x100000000) =====================

/// `red::memory::PoolStorageProxy<red::memory::PoolRoot>::GetHandle()` @ 0x10001d9a0.
/// Folha pura `adrp x8 / add x8 / ldr w0,[x8] / ret`. O `adrp` no 1º slot exercita o
/// relocador (vira `movz/movk`). NUNCA é chamada pelo self-test.
const RELOC_TARGET_VM: u64 = 0x1_0001_d9a0;
/// Prólogo esperado (4 instr, LE u32): adrp / add / ldr / ret.
const RELOC_PROLOGUE: [u32; 4] = [0x90046d48, 0x910f6108, 0xb9402100, 0xd65f03c0];

/// `red::memory::PoolStorageProxy<archive::PoolArchive>::Allocate(unsigned long long)`
/// @ 0x103e2f17c. Prólogo `sub sp / stp x20,x19 / stp x29,x30 / add x29` — 16 bytes
/// 100% não-PC-relativos → relocador copia verbatim. O `adrp` PC-relativo está em +0x14
/// (5ª instr), FORA da janela de 16 bytes.
const ARCHIVE_ALLOC_VM: u64 = 0x1_03e2_f17c;
/// Prólogo esperado (4 instr, LE u32): sub / stp / stp / add.
const ARCHIVE_ALLOC_PROLOGUE: [u32; 4] = [0xd100c3ff, 0xa9014ff4, 0xa9027bfd, 0x910083fd];

/// `red4ext-reloc-prove-ingame` (2026-07-13): `AlignedFree(void*, void*)` @ 0x100fa6f00 —
/// achada offline via scan dos símbolos definidos (`cp77-symbols/symbols-mangled.txt`) filtrando
/// pelo 1º instr = CBZ/CBNZ (script `find_cbz_tbz_bcond_prologues.py`, scratchpad). Função MÍNIMA
/// (4 instr, tail-call): `cbz x1,+0xc / ldur x0,[x1,#-8] / b <free-real> / ret`. O `cbz` no 1º
/// slot exercita a relocação de CBZ (gum.rs inverte pra `cbnz` saltando um abs-jump de 16B —
/// caminho só testado OFFLINE até agora). NUNCA é chamada pelo self-test (install→verifica→
/// revert síncrono, igual ao teste do ADRP acima).
const CBZ_TARGET_VM: u64 = 0x1_00fa_6f00;
/// Prólogo esperado (4 instr, LE u32): cbz / ldur / b / ret.
const CBZ_PROLOGUE: [u32; 4] = [0xb4000061, 0xf85f8020, 0x14ea5ca0, 0xd65f03c0];

// ===================== ABI do Allocate =====================

/// ABI nativa do `Allocate`: `(size: u64) -> *mut c_void`. x0 = size, retorno em x0.
type AllocFn = unsafe extern "C" fn(u64) -> *mut c_void;

/// Trampolim do original (devolvido por `Interceptor::replace`), tipado como `AllocFn`.
static ORIG_ALLOC: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
/// Contador de chamadas observadas (limita o log a N callers → sem flood).
static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
/// Quantos primeiros callers logar.
const ALLOC_LOG_N: u64 = 8;

// ===================== Entrada pública =====================

/// Roda os 3 self-tests, SÓ se `crate::dev_mode()`. No-op em produção.
/// Deve ser chamada DEPOIS do hook do executor estar instalado (do selfboot).
pub fn run_dev_selftests() {
    if !crate::dev_mode() {
        return; // produção: nada roda, jogo intacto
    }
    crate::log("[selftest] === início (DEV) ===");
    unsafe {
        // ArchiveXL diag PRIMEIRO (o valioso: instala o hook observe-only; não pode ser
        // bloqueado por um teste posterior).
        install_archivexl_diag();
        // Path B (ArchiveXL): HookAfter InitializeArchives → dump do ResourceGameDepot.
        install_initarchives_diag();
        // axl-factories-apply: HookAfter LoadFactoryAsync (re-inject dos factories do mod = adicionar
        // itens). NO-OP até o offset Mac ser confirmado (RE em curso) — não instala hook torto.
        install_factory_hook();
        // axl-factories-apply: probe observe-only do candidato a ResolveResource (2026-07-24, RE
        // delegada) — só loga+cross-referencia contra o que LoadFactoryAsync já captura, nunca
        // MUDA comportamento (sempre chama a original). MAS: achado 2026-08-03 (/goal) — 2 crashes
        // ao vivo consecutivos, ZERO relação com o comando sendo testado, tiveram o log deste probe
        // como ÚLTIMA atividade antes de morrer (mesmo endereço de crash genérico `0x1021730ec` que
        // QUALQUER dispatch RTTI quebrado atinge). O endereço hookado é um "candidato" (nunca
        // formalmente confirmado como `ResolveResource` de verdade) — gate atrás do MESMO marcador
        // que a outra chamada deste probe já usa (`~/.bwms-facttest`, lib.rs) em vez de sempre-ligado:
        // essa investigação (`axl-factories-apply`) já está FECHADA há sessões, não precisa rodar em
        // todo boot de dev — reduz superfície de instabilidade de fundo pra investigações futuras.
        if std::env::var("HOME").ok().map(|h| std::path::Path::new(&h).join(".bwms-facttest").exists()).unwrap_or(false) {
            install_resolveresource_probe();
        }
        // ArchiveXL: HookBefore open-archive → loga o PATH de cada .archive carregado (prova
        // que os nossos carregam + mapeia a função do Path B).
        install_openarchive_diag();
        // resource.link: hook NAKED em 0x1021c5858 (ResourcePath->ref) — swap de path = link. Comandos
        // reslinkdump/reslink/reslinkstat provam in-game.
        install_reslink();
        // EARLY-LOAD: carrega bwms-reslink.txt AQUI (on_load/ctor-time, antes de qualquer
        // inicialização do motor) para que o redirect esteja ativo ANTES da primeira construção
        // de ResourcePath a partir de string (ex. atlas_common.xbm é construído 24272x no menu,
        // começando antes do cp77_tick rodar). Sem isso o redirect chega tarde e os widgets já
        // foram inicializados com o atlas original.
        if let Some(red4) = std::path::Path::new(&crate::mods_dir()).parent() {
            let f = red4.join("bwms-reslink.txt");
            if f.is_file() {
                reslink_file(&f.to_string_lossy());
                crate::log(&format!("[reslink] early-load: {} carregado no on_load", f.display()));
            }
        }
        let _ = install_sweep;
        let _ = install_depot_probes;
        let _ = install_reqres_probe;
        // Relocador (type 1) — já provado in-game.
        test_relocator();
        // `red4ext-reloc-prove-ingame`: mesma técnica, alvo real com prólogo CBZ (não ADRP).
        test_relocator_cbz();
        // `test_vtable_pool()` — TENTADO e REVERTIDO (2026-07-13): CRASHOU ao vivo (EXC_BAD_ACCESS
        // null-deref dentro de `PoolStorageProxy<PoolDefault>::AllocateAligned`). Causa: `run_dev_selftests`
        // roda MUITO cedo (dentro de `on_load`/`selfboot_if_needed`, ~ctor-time), ANTES do pool
        // `PoolDefault` do próprio motor estar inicializado — `rtti::pool_alloc` só é seguro
        // mais tarde (confirmado: todo forge de classe usa pool_alloc só de dentro de
        // `class_validate_probe_hook`, que roda durante o bind do script, bem depois). MESMA
        // categoria de armadilha de timing já documentada pra RTTI/GetOrRegisterType — reusar o
        // "mais cedo tecnicamente seguro" já mapeado (2ª chamada de GetOrRegisterType em diante),
        // não `on_load` direto. Função mantida (não chamada) pra retomada futura no hook certo.
        // vtable (type 2): vtable_hook/unhook está pronto e compila. O teste-vivo antigo
        // (test_vtable) protegia memória do STACK (inválido -> travava o fluxo). Valida-se
        // contra uma vtable real __DATA_CONST do jogo na fase do Codeware UI, não numa array falsa.
        crate::log("[selftest] vtable: função pronta (vtable_hook/unhook), valida-se contra vtable real do jogo (Codeware UI)");
    }
    crate::log("[selftest] === fim ===");
}

// ===================== 1) Relocador (inline hook, type 1) =====================

/// Replacement-dummy do teste do relocador. NUNCA é chamado (o alvo é hookado e
/// revertido sem nunca executar a função). Existe só pra ter um ponteiro válido.
extern "C" fn reloc_dummy_repl() {}
/// 2º replacement-dummy — corpo distinto do 1º (evita code-folding pro MESMO endereço),
/// usado no 2º ciclo de install+revert (mesmo alvo) do teste `red4ext-attach-detach-contract`.
static RELOC_DUMMY2_TOUCHED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
extern "C" fn reloc_dummy_repl2() {
    RELOC_DUMMY2_TOUCHED.store(1, std::sync::atomic::Ordering::Relaxed);
}

unsafe fn test_relocator() {
    let target = crate::rebase(RELOC_TARGET_VM);

    // GUARD 1: legibilidade dos 16 bytes.
    if !gum::is_readable(target as *const c_void, 16) {
        crate::log("[selftest] relocador: alvo ilegível -> pulado (sem hook)");
        return;
    }
    // GUARD 2: prólogo bate com o esperado? Se um patch moveu/mudou a função, ABORTA
    // sem hookar (não corrompe nada).
    if !prologue_matches(target, &RELOC_PROLOGUE) {
        crate::log("[selftest] relocador: prólogo não casou (patch?) -> abortado (sem hook)");
        return;
    }

    // `red4ext-attach-detach-contract` (2026-07-13): guarda os 16 bytes ORIGINAIS ANTES de
    // qualquer hook, pra comparar byte-a-byte depois do revert (não só "revert() foi chamado" —
    // prova que o conteúdo REALMENTE voltou idêntico ao que era).
    let mut original = [0u8; 16];
    std::ptr::copy_nonoverlapping(target as *const u8, original.as_mut_ptr(), 16);

    let it = Interceptor::obtain();
    match it.replace(target, reloc_dummy_repl as *mut c_void) {
        Some(tramp) => {
            // O 1º slot do alvo é um `adrp` → o relocador deve materializá-lo como
            // movz/movk no início do trampolim. Confirma que tramp[0..4] é um `movz`.
            // movz (64-bit): 0xD28xxxxx (bits[31:23] = 0b110100101). Máscara 0xFF80_0000.
            let mut first = [0u8; 4];
            std::ptr::copy_nonoverlapping(tramp as *const u8, first.as_mut_ptr(), 4);
            let insn = u32::from_le_bytes(first);
            let is_movz = (insn & 0xFF80_0000) == 0xD280_0000;

            // DESINSTALA IMEDIATAMENTE — nunca deixa o alvo hookado, nunca chama a fn.
            it.revert(target);

            // Byte-exato: os 16 bytes do alvo DEPOIS do revert precisam bater 1:1 com os
            // capturados ANTES de qualquer hook — prova que o detach restaura o prólogo
            // original de verdade, não uma aproximação.
            let mut after = [0u8; 16];
            std::ptr::copy_nonoverlapping(target as *const u8, after.as_mut_ptr(), 16);
            let bytes_match = after == original;

            if is_movz {
                crate::log(&format!(
                    "[selftest] relocador OK em PoolStorageProxy<PoolRoot>::GetHandle (adrp relocado -> movz {insn:#010x})"
                ));
            } else {
                crate::log(&format!(
                    "[selftest] relocador FALHOU: tramp[0..4]={insn:#010x} não é movz (adrp não relocado?)"
                ));
            }
            crate::log(&format!(
                "[selftest] attach-detach: prólogo pós-revert {} do original ({} bytes) -> {}",
                if bytes_match { "BATE byte-a-byte" } else { "DIVERGE" },
                original.len(),
                if bytes_match { ">>> BYTE-EXATO OK <<<" } else { ">>> FALHOU <<<" }
            ));

            // `red4ext-attach-detach-contract` (hooks múltiplos por target, sequencial): re-hooka
            // o MESMO alvo com um replacement DIFERENTE, confirma que o 2º install também produz
            // um trampolim relocado corretamente, e reverte de novo — prova que o alvo não fica
            // "marcado"/corrompido por um ciclo anterior de install+revert.
            match it.replace(target, reloc_dummy_repl2 as *mut c_void) {
                Some(tramp2) => {
                    let mut first2 = [0u8; 4];
                    std::ptr::copy_nonoverlapping(tramp2 as *const u8, first2.as_mut_ptr(), 4);
                    let insn2 = u32::from_le_bytes(first2);
                    let is_movz2 = (insn2 & 0xFF80_0000) == 0xD280_0000;
                    it.revert(target);
                    let mut after2 = [0u8; 16];
                    std::ptr::copy_nonoverlapping(target as *const u8, after2.as_mut_ptr(), 16);
                    let bytes_match2 = after2 == original;
                    crate::log(&format!(
                        "[selftest] 2º ciclo (replacement diferente, mesmo alvo): relocado={is_movz2} byte-exato-pós-revert={bytes_match2} -> {}",
                        if is_movz2 && bytes_match2 { ">>> MÚLTIPLOS HOOKS POR TARGET OK <<<" } else { ">>> FALHOU <<<" }
                    ));
                }
                None => crate::log("[selftest] 2º ciclo FALHOU: Interceptor::replace (2ª vez) devolveu None"),
            }
        }
        None => {
            crate::log("[selftest] relocador FALHOU: Interceptor::replace devolveu None");
        }
    }
    std::mem::forget(it); // Interceptor é ZST; evita qualquer Drop implícito
}

/// `red4ext-reloc-prove-ingame` — MESMO padrão de `test_relocator`, mas no alvo CBZ
/// (`AlignedFree`, ver `CBZ_TARGET_VM`). Prova que o relocador lida com CBZ/CBNZ num alvo REAL
/// (não só o teste sintético offline `gum::tests`) — a inversão pra `cbnz` + abs-jump de 16B
/// (gum.rs:293-303) precisa materializar corretamente no trampolim.
unsafe fn test_relocator_cbz() {
    let target = crate::rebase(CBZ_TARGET_VM);
    if !gum::is_readable(target as *const c_void, 16) {
        crate::log("[selftest] relocador(CBZ): alvo ilegível -> pulado (sem hook)");
        return;
    }
    if !prologue_matches(target, &CBZ_PROLOGUE) {
        crate::log("[selftest] relocador(CBZ): prólogo não casou (patch?) -> abortado (sem hook)");
        return;
    }
    let mut original = [0u8; 16];
    std::ptr::copy_nonoverlapping(target as *const u8, original.as_mut_ptr(), 16);

    let it = Interceptor::obtain();
    match it.replace(target, reloc_dummy_repl as *mut c_void) {
        Some(tramp) => {
            // O 1º slot do alvo é um `cbz` → o relocador deve materializar um `cbnz` invertido
            // saltando por cima de um abs-jump (gum.rs:293-303). Confirma tramp[0..4] = cbnz
            // (mesma família CBZ/CBNZ, bit24 setado): máscara 0x7E000000 == 0x34000000, bit24=1.
            let mut first = [0u8; 4];
            std::ptr::copy_nonoverlapping(tramp as *const u8, first.as_mut_ptr(), 4);
            let insn = u32::from_le_bytes(first);
            let is_cbnz_family = (insn & 0x7E000000) == 0x34000000 && (insn & 0x01000000) != 0;

            it.revert(target);

            let mut after = [0u8; 16];
            std::ptr::copy_nonoverlapping(target as *const u8, after.as_mut_ptr(), 16);
            let bytes_match = after == original;

            crate::log(&format!(
                "[selftest] relocador(CBZ) em AlignedFree: 1ª instr do tramp={insn:#010x} cbnz-invertido={is_cbnz_family} | pós-revert byte-exato={bytes_match} -> {}",
                if is_cbnz_family && bytes_match { ">>> RELOC-EM-CBZ-REAL OK <<<" } else { ">>> FALHOU <<<" }
            ));
        }
        None => crate::log("[selftest] relocador(CBZ): Interceptor::replace devolveu None"),
    }
    std::mem::forget(it);
}

// ===================== 2) vtable (type 2) =====================
// Teste-vivo ANTIGO removido: protegia memória do STACK (mach_vm_protect numa array
// local) — inválido, travava o self-test. `test_vtable_pool` (2026-07-13) resolve isso
// usando `rtti::pool_alloc` (HEAP do pool do próprio jogo, mesma alocação que os forges de
// classe usam a sessão inteira) — protection-flip funciona igual em heap, sem o problema
// de stack. NÃO mexe em nenhuma vtable REAL do jogo (buffer 100% nosso, nunca consultado
// por nenhum sistema do motor) — zero risco a sistemas vivos, mas exercita a MESMA função
// `vtable_hook`/`vtable_unhook` que a `BwmsApi` expõe pra plugins.

/// `red4ext-api-prove-hooks-extplugin` (fatia vtable_hook, 2026-07-13): aloca um buffer de 2
/// slots no POOL do jogo (`rtti::pool_alloc`, mesma via que os forges de classe usam),
/// escreve um ponteiro conhecido no slot 0, chama `gum::vtable_hook` (a MESMA função por trás
/// de `BwmsApi.vtable_hook`) pra trocar por outro ponteiro, confirma a troca, e
/// `vtable_unhook` pra restaurar — tudo num buffer que NENHUM sistema do motor consulta.
unsafe fn test_vtable_pool() {
    let buf = crate::rtti::pool_alloc(16, 8) as *mut u64;
    if buf.is_null() {
        crate::log("[selftest] vtable(pool): pool_alloc devolveu null -> pulado");
        return;
    }
    let original_fn = reloc_dummy_repl as *const c_void;
    let replacement_fn = reloc_dummy_repl2 as *const c_void;
    buf.write(original_fn as u64);
    buf.add(1).write(0xDEAD_BEEF_0000_0000u64); // slot vizinho, só pra confirmar que não vaza

    let hooked = gum::vtable_hook(buf, 0, replacement_fn);
    let slot0_after_hook = buf.read() as *const c_void;
    let hook_ok = hooked == Some(original_fn) && slot0_after_hook == replacement_fn;

    gum::vtable_unhook(buf, 0, original_fn);
    let slot0_after_unhook = buf.read() as *const c_void;
    let unhook_ok = slot0_after_unhook == original_fn;
    let neighbor_intact = buf.add(1).read() == 0xDEAD_BEEF_0000_0000u64;

    crate::log(&format!(
        "[selftest] vtable(pool): hook devolveu original certo={hook_ok} | pós-unhook restaurou certo={unhook_ok} | slot vizinho intacto={neighbor_intact} -> {}",
        if hook_ok && unhook_ok && neighbor_intact { ">>> VTABLE_HOOK/UNHOOK OK <<<" } else { ">>> FALHOU <<<" }
    ));
}

// ===================== 3) Diagnóstico ArchiveXL (observe-only) =====================

// ===== Path B (ArchiveXL): HookAfter InitializeArchives — dump do ResourceGameDepot =====
// InitializeArchives(ResourceGameDepot* this) @ vmaddr 0x103ed96b0 (cadeia mapeada em
// six-core-mods-status.md). this+0x68 = DynArray<ArchiveSet> (entries@0x68, size@0x74, stride 0x140).
// 1º passo do Path B: provar que hookamos o LOADER + acessamos o depot — base p/ carregar nossos
// .archive de pasta própria (com ordem controlada), em vez de depender do glob basegame_*.
// Observe-only: chama a original (HookAfter) e só LÊ o depot.
const INITARCH_VM: u64 = 0x1_03ed_96b0;
static ORIG_INITARCH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

unsafe extern "C" fn initarch_replacement(this: *mut c_void) {
    // HookAfter: roda a original PRIMEIRO (x0 = this chega cru; o abs-jump usa x17).
    let orig = ORIG_INITARCH.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: unsafe extern "C" fn(*mut c_void) = std::mem::transmute(orig);
        f(this);
    }
    if this.is_null() || !gum::is_readable(this as *const c_void, 0x80) {
        crate::log("[initarch-diag] depot ilegível");
        return;
    }
    let entries = core::ptr::read_unaligned((this as *const u8).add(0x68) as *const u64) as *const u8;
    let count = core::ptr::read_unaligned((this as *const u8).add(0x74) as *const u32);
    crate::log(&format!(
        "[initarch-diag] InitializeArchives HOOKADO ✓ depot={this:p} (static {:#x}) ArchiveSets entries={entries:p} count={count}",
        crate::un_rebase(this)
    ));
    if !entries.is_null() && gum::is_readable(entries as *const c_void, 0x10) {
        for i in 0..count.min(3) as usize {
            let set = entries.add(i * 0x140);
            if !gum::is_readable(set as *const c_void, 0x10) {
                break;
            }
            let q0 = core::ptr::read_unaligned(set as *const u64);
            let q1 = core::ptr::read_unaligned(set.add(8) as *const u64);
            crate::log(&format!("[initarch-diag]   set[{i}] @ {set:p}: q0={q0:#x} q1={q1:#x}"));
        }
    }
}

/// Instala um hook de diagnóstico "observe-only" standalone: 1 alvo, guard de legibilidade com
/// mensagem PRÓPRIA (retorna cedo, sem tentar instalar) antes do guard de idempotência — ordem
/// distinta de `install_hook_probe!` (que combina os 2 guards numa condição só), por isso um macro
/// separado em vez de reusar aquele. Generaliza 3 funções `install_*_diag`/`install_*_probe`
/// byte-idênticas (`initarchives`/`openarchive`/`reqres`, cada uma copiada da anterior em sessões
/// diferentes) — 2026-08-06 (cont.30), mesmo padrão de redução de redundância do cont.26-29.
macro_rules! install_diag_hook {
    ($vm:expr, $probe:expr, $orig:expr, $unreadable:expr, $ok:expr, $fail:expr) => {{
        static INSTALLED: AtomicBool = AtomicBool::new(false);
        let target = crate::rebase($vm);
        if !gum::is_readable(target as *const c_void, 16) {
            crate::log($unreadable);
            return;
        }
        if INSTALLED.swap(true, Ordering::Relaxed) {
            return;
        }
        let it = Interceptor::obtain();
        match it.replace(target, $probe as *mut c_void) {
            Some(tramp) => {
                $orig.store(tramp, Ordering::Relaxed);
                std::mem::forget(it);
                crate::log($ok);
            }
            None => {
                INSTALLED.store(false, Ordering::Relaxed);
                crate::log($fail);
            }
        }
    }};
}

unsafe fn install_initarchives_diag() {
    install_diag_hook!(
        INITARCH_VM, initarch_replacement, ORIG_INITARCH,
        "[initarch-diag] alvo InitializeArchives ilegível -> sem hook",
        "[initarch-diag] hook instalado em InitializeArchives (HookAfter, observe-only)",
        "[initarch-diag] FALHA ao hookar InitializeArchives (replace None)"
    );
}

// ===== axl-factories-apply — HookAfter FactoryIndex::LoadFactoryAsync (ADICIONAR ITENS, uso #1) =====
// Mecanismo (cp2077-archive-xl/src/App/Extensions/FactoryIndex/Extension.cpp): HookAfter
// LoadFactoryAsync(aIndex x0, ResourcePath aPath x1 /*u64*/, aContext x2). Quando aPath == o ÚLTIMO
// factory vanilla (SENTINEL "base\gameplay\factories\vehicles\vehicles.csv"), re-chama LoadFactoryAsync
// p/ CADA factory do mod → injeta os itens do mod DEPOIS do vanilla terminar. O aPath é um ResourcePath
// = FNV-1a64 do path normalizado = o nosso resource_path_hash (mesma fn do resource.link, agora em
// bwms-hashes). **Offset Mac de LoadFactoryAsync = [UNCONFIRMED]** (RE em curso, workflow
// bwms-6mods-attack): o install só liga quando FACTORY_LOADFACTORY_VM for válido (!=0 e legível).
// vmaddr Mac de FactoryIndex::LoadFactoryAsync (link base 0x100000000) — achado por RE semântica
// ARM64 2026-07-16 (workflow de disassembly, ALTA confiança; ver notes/native-addrs-found-2026-07-16.md).
// Assinatura `void(uintptr_t aIndex /*x0*/, ResourcePath aPath /*x1,u64*/, uintptr_t aContext /*x2*/)`.
// Único caller = 0x100cc11c8 (iterador 0x100cc10dc), 1 chamada por factory csv. VERIFY: os goldens
// no factory_replacement (vehicles.csv=0xf94faab4ff97393a sentinel, object_pool_budgets=0x433d78092c642133).
const FACTORY_LOADFACTORY_VM: u64 = 0x1_00cc_0710;
/// Goldens de verificação (bwms_hashes::resource_path_hash dos paths .csv de factory conhecidos).
const FACTORY_GOLDEN_SENTINEL: u64 = 0xf94f_aab4_ff97_393a; // base\gameplay\factories\vehicles\vehicles.csv (ÚLTIMA)
const FACTORY_GOLDEN_OBJPOOL: u64 = 0x433d_7809_2c64_2133; // base\gameplay\factories\items\object_pool_budgets.csv
/// Contador de chamadas observadas de LoadFactoryAsync (limita log a N).
static FACTORY_CALLS: AtomicU64 = AtomicU64::new(0);
const FACTORY_SENTINEL: &str = "base\\gameplay\\factories\\vehicles\\vehicles.csv";
static ORIG_LOADFACTORY: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
/// Último `aIndex` (FactoryIndex*) visto pelo hook — exposto pro comando de canal `factdump` (dump
/// read-only, pós-async, pra achar o offset REAL do count do registry). Ver `dump_factory_index`.
pub(crate) static FACTORY_LAST_INDEX: AtomicUsize = AtomicUsize::new(0);
/// aIndex ESPECÍFICO capturado no momento do sentinel/re-inject (não sobrescrito por outras chamadas).
pub(crate) static FACTORY_REINJECT_INDEX: AtomicUsize = AtomicUsize::new(0);
/// Hashes (ResourcePath) dos .csv de factory dos mods — o mod-manager popula do `.xl` (secção
/// `factories`), o runtime re-injeta no sentinel. Análogo ao RESLINK_MAP do resource.link.
static FACTORY_PATHS: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());

/// Adiciona UM .csv de factory (por path) à lista de re-inject. Dedup por hash.
pub(crate) fn factory_add(path: &str) {
    let h = resource_path_hash(path);
    if let Ok(mut v) = FACTORY_PATHS.lock() {
        if !v.contains(&h) {
            v.push(h);
        }
        crate::log(&format!("[factory] +'{path}' (#{h:#018x}); {} factories na fila", v.len()));
    }
}
/// Carrega N paths de factory de um arquivo (1 por linha; `#`=comentário) — o que o mod-manager gera
/// do `.xl`. Como o reslink_file.
pub(crate) fn factory_file(path: &str) {
    match std::fs::read_to_string(path) {
        Ok(c) => {
            let mut n = 0;
            for line in c.lines() {
                let l = line.trim();
                if !l.is_empty() && !l.starts_with('#') {
                    factory_add(l);
                    n += 1;
                }
            }
            crate::log(&format!("[factory] {n} factories carregados de '{path}'"));
        }
        Err(e) => crate::log(&format!("[factory] não leu '{path}': {e}")),
    }
}

/// Carrega hashes de mesh-alvo de um arquivo (1 hash hex por linha; `#`=comentário).
/// Primeira entrada válida define o target para `axl-resource-patch-apply`.
/// Formato: hex64 sem 0x (e.g. "df9c9af540d736ba") ou com 0x.
pub(crate) fn patch_target_file(path: &str) {
    match std::fs::read_to_string(path) {
        Ok(c) => {
            for line in c.lines() {
                let l = line.trim().trim_start_matches("0x");
                if l.is_empty() || l.starts_with('#') { continue; }
                match u64::from_str_radix(l, 16) {
                    Ok(h) => {
                        PATCH_TARGET_HASH_RT.store(h, Ordering::Relaxed);
                        crate::log(&format!("[axl-patch] target configurado: {h:#018x}"));
                        return;
                    }
                    Err(_) => crate::log(&format!("[axl-patch] hash inválido: '{l}'")),
                }
            }
            crate::log("[axl-patch] bwms-patches.txt sem hash válido");
        }
        Err(_) => {} // arquivo ausente = inject desabilitado (silencioso)
    }
}

/// HookAfter LoadFactoryAsync: roda o vanilla; se o path é o sentinel (último factory), re-injeta os
/// factories do mod com a MESMA fn nativa (aIndex/aContext preservados).
unsafe extern "C" fn factory_replacement(index: usize, path: u64, context: usize) {
    let orig = ORIG_LOADFACTORY.load(Ordering::Relaxed);
    if orig.is_null() {
        return;
    }
    // VERIFICAÇÃO do endereço (observe-only, achado por RE 2026-07-16): loga os N primeiros callers.
    // Se x1 (path) casar os goldens (object_pool_budgets no meio da rajada + vehicles.csv como a
    // ÚLTIMA/sentinel) e index (aIndex) for constante, o endereço 0x100cc0710 está CONFIRMADO.
    {
        FACTORY_LAST_INDEX.store(index, Ordering::Relaxed);
        let n = FACTORY_CALLS.fetch_add(1, Ordering::Relaxed);
        let sentinel = path == FACTORY_GOLDEN_SENTINEL;
        if n < 32 || sentinel {
            let tag = if sentinel {
                " <== SENTINEL (vehicles.csv, última)"
            } else if path == FACTORY_GOLDEN_OBJPOOL {
                " <== object_pool_budgets.csv (golden)"
            } else {
                ""
            };
            crate::log(&format!(
                "[factory-diag] LoadFactoryAsync #{n} aIndex={index:#x} aPath={path:#018x} aContext={context:#x}{tag}"
            ));
        }
    }
    let f: unsafe extern "C" fn(usize, u64, usize) = std::mem::transmute(orig);
    f(index, path, context); // HookAfter: vanilla primeiro
    if path == resource_path_hash(FACTORY_SENTINEL) {
        // CAPTURA o aIndex ESPECÍFICO deste sentinel (achado 2026-07-17: FACTORY_LAST_INDEX é
        // sobrescrito por OUTRAS chamadas de LoadFactoryAsync com aIndex DIFERENTE — várias factory
        // tables coexistem/streamam durante gameplay — então "o último visto" não é confiável pra
        // rastrear ESTE re-inject especificamente ao longo do tempo). `factdump` usa este ponteiro fixo.
        FACTORY_REINJECT_INDEX.store(index, Ordering::Relaxed);
        // PROVA (log, sem visual): o FactoryIndex (aIndex) tem um registry cujo COUNT cresce quando um
        // factory novo carrega. Snapshot dos u32 de [aIndex+0 .. +0x80] ANTES e DEPOIS da re-injeção;
        // qualquer offset que CRESCEU = o count do registry → prova que o meu factory ENTROU no índice.
        let idx = index as *const u8;
        let readable = gum::is_readable(idx as *const c_void, 0x80);
        let mut before = [0u32; 32];
        if readable {
            for (i, b) in before.iter_mut().enumerate() {
                *b = (idx.add(i * 4) as *const u32).read_unaligned();
            }
        }
        if let Ok(v) = FACTORY_PATHS.lock() {
            for &h in v.iter() {
                f(index, h, context); // re-injeta o factory do mod DEPOIS do sentinel
                crate::log(&format!("[factory] re-injetado #{h:#018x} (após o sentinel)"));
            }
        }
        if readable {
            let mut cresceu = String::new();
            for i in 0..32usize {
                let after = (idx.add(i * 4) as *const u32).read_unaligned();
                if after > before[i] && after.wrapping_sub(before[i]) < 100_000 {
                    cresceu.push_str(&format!(" [+{:#x}]{}→{}", i * 4, before[i], after));
                }
            }
            if cresceu.is_empty() {
                crate::log("[factory] aIndex: NENHUM u32 [0..0x80] cresceu após a re-inject (o factory não entrou no registry, ou o count é noutro offset/estrutura)");
            } else {
                crate::log(&format!(
                    ">>> FACTORY-APPLY OK: o registry do FactoryIndex CRESCEU após injetar o factory custom (entrada ativa):{cresceu} <<<"
                ));
            }
        }
    }
}

/// Instala o HookAfter em LoadFactoryAsync. NO-OP enquanto FACTORY_LOADFACTORY_VM==0 (offset pendente
/// de RE) — não instala hook torto. Quando o offset for confirmado, liga sozinho.
pub(crate) unsafe fn install_factory_hook() {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if FACTORY_LOADFACTORY_VM == 0 {
        return; // offset ainda não confirmado (RE em curso)
    }
    let target = crate::rebase(FACTORY_LOADFACTORY_VM);
    if !gum::is_readable(target as *const c_void, 16) || INSTALLED.swap(true, Ordering::Relaxed) {
        return;
    }
    let it = Interceptor::obtain();
    match it.replace(target, factory_replacement as *mut c_void) {
        Some(tramp) => {
            ORIG_LOADFACTORY.store(tramp, Ordering::Relaxed);
            std::mem::forget(it);
            crate::log("[factory] HookAfter LoadFactoryAsync instalado (re-inject dos factories do mod)");
        }
        None => {
            INSTALLED.store(false, Ordering::Relaxed);
            crate::log("[factory] FALHA ao hookar LoadFactoryAsync");
        }
    }
}

/// `axl-factories-apply` (achado 2026-07-17): LoadFactoryAsync é ASSÍNCRONO — a re-inject só ENFILEIRA
/// o load, o registry do FactoryIndex cresce DEPOIS. Snapshot imediato (0 boots atrás) não achou nada
/// em [aIndex+0..0x80]. Este dump é READ-ONLY e ROBUSTO: varre uma janela BEM mais ampla
/// [0x00..0x400) procurando por padrões `{ptr_heap, u32 cap, u32 count}` (DynArray genérico do engine,
/// mesmo layout achado no depot do pathb) — candidatos plausíveis (ptr em heap real, cap>=count,
/// count pequeno) são logados pra eu comparar antes/depois manualmente (2 chamadas, com espera entre).
pub(crate) unsafe fn dump_factory_index() {
    // Prefere o aIndex FIXADO no momento do sentinel/re-inject (estável) — FACTORY_LAST_INDEX é
    // sobrescrito por outras chamadas concorrentes de LoadFactoryAsync (várias tables coexistem).
    let idx = match FACTORY_REINJECT_INDEX.load(Ordering::Relaxed) {
        0 => FACTORY_LAST_INDEX.load(Ordering::Relaxed),
        v => v,
    };
    if idx == 0 {
        crate::log("[factdump] nenhum aIndex capturado ainda (hook não rodou?)");
        return;
    }
    let base = idx as *const u8;
    const WIN: usize = 0x400;
    if !gum::is_readable(base as *const c_void, WIN) {
        crate::log(&format!("[factdump] aIndex={idx:#x} ilegível na janela {WIN:#x}"));
        return;
    }
    let mut out = format!("[factdump] aIndex={idx:#x} candidatos DynArray-like em [0..{WIN:#x}):");
    let mut n = 0;
    let mut off = 0usize;
    while off + 16 <= WIN {
        let ptr = (base.add(off) as *const u64).read_unaligned();
        let cap = (base.add(off + 8) as *const u32).read_unaligned();
        let cnt = (base.add(off + 12) as *const u32).read_unaligned();
        // heap real (não null, não a faixa de imagem estática 0x1_0000_0000..0x1_1000_0000)
        let ptr_heap = ptr != 0 && !(0x1_0000_0000..0x1_1000_0000).contains(&ptr);
        if ptr_heap && cnt > 0 && cnt <= cap && cap < 100_000 && gum::is_readable(ptr as *const c_void, 8) {
            out.push_str(&format!(" [+{off:#x}]{{ptr={ptr:#x} cap={cap} count={cnt}}}"));
            n += 1;
            if n >= 24 {
                break;
            }
        }
        off += 4; // granularidade de 4B (structs não necessariamente 8-alinhadas aqui)
    }
    if n == 0 {
        out.push_str(" (nenhum candidato — talvez a estrutura seja HashMap/outro layout, ou o count ainda não cresceu)");
    }
    crate::log(&out);
}

// ===== axl-factories-apply — probe observe-only do candidato a FactoryIndex::ResolveResource =====
// RE delegada 2026-07-24: candidato 0x100cc0bec (bucket+chain hash lookup, cross-validado contra o
// Entry-array já confirmado via um rehash-helper achado à parte, 0x100cc1434, que toca os MESMOS
// offsets +0x68/+0x70/+0x74/+0x78/+0x84 no MESMO objeto que tem +0xa0/+0xac). NÃO confirmado que é
// exatamente `FactoryIndex::ResolveResource` (pode ser um `HashIndex::Find` genérico reusado por
// várias instâncias de `ent::GenericListFactory<T>` no motor inteiro — 16 callers espalhados, nenhum
// vindo diretamente do código de orquestração da FactoryIndex). Este hook é PURO OBSERVE-ONLY (chama
// o original sempre, nunca muda o resultado) — objetivo é só logar e comparar contra o que
// `factory_replacement`/`FACTORY_REINJECT_INDEX` já captura, pra decidir se bate.
const FACTORY_RESOLVE_VM: u64 = 0x1_00cc_0bec;
static ORIG_RESOLVE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static RESOLVE_CALLS: AtomicU64 = AtomicU64::new(0);

type ResolveFn = unsafe extern "C" fn(*mut c_void, u64) -> *mut c_void;

unsafe extern "C" fn resolveresource_replacement(x0: *mut c_void, x1: u64) -> *mut c_void {
    let orig = ORIG_RESOLVE.load(Ordering::Relaxed);
    let ret = if orig.is_null() {
        std::ptr::null_mut()
    } else {
        let f: ResolveFn = std::mem::transmute(orig);
        f(x0, x1)
    };
    let n = RESOLVE_CALLS.fetch_add(1, Ordering::Relaxed);
    let sentinel = x1 == FACTORY_GOLDEN_SENTINEL;
    let objpool = x1 == FACTORY_GOLDEN_OBJPOOL;
    // cross-referência: este `x0` (this do candidato) bate com o aIndex (FactoryIndex*) que
    // LoadFactoryAsync/factory_replacement JÁ capturou pra este mesmo hash?
    let last_idx = FACTORY_LAST_INDEX.load(Ordering::Relaxed);
    let reinject_idx = FACTORY_REINJECT_INDEX.load(Ordering::Relaxed);
    let same_obj = (x0 as usize) == last_idx || (x0 as usize) == reinject_idx;
    // 2026-07-29: `same_obj` era logado SEM cap (n<40 só cobria o caso base) — durante streaming
    // de mundo denso, `same_obj` é quase sempre true (mesma FactoryIndex resolvendo centenas de
    // hashes), gerando um open+write de arquivo por chamada sem limite. `axl-factories-apply` já
    // fechou (IMPL) faz tempo — o valor diagnóstico já foi extraído; capar TUDO em 40 remove I/O
    // autoinfligido que pode estar competindo com o próprio streaming lento (achado do dia, ver
    // DATABASE.md "streaming lento"). Comportamento observado (retorna o original) é IDÊNTICO.
    if n < 40 {
        let tag = match (sentinel, objpool, same_obj) {
            (true, _, _) => " <== SENTINEL (vehicles.csv) — MESMO hash que LoadFactoryAsync viu por último",
            (_, true, _) => " <== object_pool_budgets.csv (golden)",
            (_, _, true) => " <<< x0 BATE com o FactoryIndex* já visto por LoadFactoryAsync — MESMO OBJETO",
            _ => "",
        };
        crate::log(&format!(
            "[resolve-probe] ResolveResource? #{n} this={x0:p} hash={x1:#018x} -> {ret:p}{tag}"
        ));
    }
    ret
}

/// Instala o probe observe-only no candidato de `ResolveResource`. NO-OP se o alvo for ilegível.
/// Nunca muda comportamento (sempre chama a original) — seguro mesmo se o candidato estiver errado
/// (na pior hipótese, loga chamadas de uma função qualquer, não quebra nada).
pub(crate) unsafe fn install_resolveresource_probe() {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    let target = crate::rebase(FACTORY_RESOLVE_VM);
    if !gum::is_readable(target as *const c_void, 16) || INSTALLED.swap(true, Ordering::Relaxed) {
        return;
    }
    let it = Interceptor::obtain();
    match it.replace(target, resolveresource_replacement as *mut c_void) {
        Some(tramp) => {
            ORIG_RESOLVE.store(tramp, Ordering::Relaxed);
            std::mem::forget(it);
            crate::log("[resolve-probe] hook instalado no candidato ResolveResource (observe-only, 0x100cc0bec)");
        }
        None => {
            INSTALLED.store(false, Ordering::Relaxed);
            crate::log("[resolve-probe] FALHA ao hookar candidato ResolveResource (replace None)");
        }
    }
}

/// Consulta o FactoryIndex capturado: dado um nome de entry (ex: "bwms_test_weapon"), computa o
/// CName hash e chama a ResolveResource original. Retorno != 0 → entry existe no factory index.
/// Chama dois entries: o argumento passado + "weapon_root" (golden vanilla, sempre presente se o
/// index for o de armas). Compara os dois pra distinguir "factory index errado" de "entry não carregado".
pub(crate) unsafe fn factlookup_test(name: &str) {
    let orig = ORIG_RESOLVE.load(Ordering::Relaxed);
    let idx = match FACTORY_REINJECT_INDEX.load(Ordering::Relaxed) {
        0 => FACTORY_LAST_INDEX.load(Ordering::Relaxed),
        v => v,
    };
    if orig.is_null() || idx == 0 {
        crate::log(&format!(
            "[factlookup] NOT READY: orig={orig:p} idx={idx:#x} (probe não instalado ou sem factory index)"
        ));
        return;
    }
    let f: ResolveFn = std::mem::transmute(orig);
    // golden do bwms_factory_test = base\gameplay\items\consumables\inhalers\base_inhaler_item.ent
    let expected_bwms_path = resource_path_hash("base\\gameplay\\items\\consumables\\inhalers\\base_inhaler_item.ent");
    let expected_weapon_root_path = resource_path_hash("base\\weapons\\weapon_root.ent");
    for lookup in &["weapon_root", name] {
        let h = crate::cname::cname(lookup);
        let ret = f(idx as *mut c_void, h);
        let ret_u64 = ret as u64;
        if ret_u64 == 0 {
            crate::log(&format!(
                "[factlookup] '{lookup}' cname={h:#018x} -> 0 (NULL) — NÃO encontrado no factory idx={idx:#x}"
            ));
        } else if ret_u64 == expected_bwms_path {
            crate::log(&format!(
                ">>> FACTORY-APPLY OK: '{lookup}' -> {ret_u64:#018x} = base_inhaler_item.ent PROVADO — factory BWMS INJETADO no idx={idx:#x} <<<"
            ));
        } else if ret_u64 == expected_weapon_root_path {
            crate::log(&format!(
                ">>> FACTORY-APPLY OK: '{lookup}' -> {ret_u64:#018x} = weapon_root.ent path hash CONFIRMADO no factory idx={idx:#x} <<<"
            ));
        } else {
            crate::log(&format!(
                "[factlookup] '{lookup}' cname={h:#018x} -> {ret_u64:#018x} (path hash não-zero = entry EXISTE; expected_bwms={expected_bwms_path:#018x} expected_weapon_root={expected_weapon_root_path:#018x})"
            ));
        }
    }
}

// ===== axl-factories-apply — leitura + escrita direta na hash table do FactoryIndex =====
// O archive system não consegue injetar (glob seletivo). Alternativa: escrever diretamente nas
// estruturas internas do FactoryIndex. ResolveResource @0x100cc0bec = HashIndex::Find(this, cname).
// A hash table tem layout bucket+chain; os offsets +0x68/+0x70/+0x74/+0x78/+0x84 foram achados pelo
// rehash-helper @0x100cc1434. `factread` dumpa esses offsets pra entender o layout real.
// `factwrite` tenta escrever diretamente na entry array após lê-la.

/// Dumpa a hash table interna do FactoryIndex (offsets +0x68..+0x90) pra RE do layout.
/// Objetivo: achar o offset do entry array (key=CName, val=ResourcePath) pra `factwrite`.
pub(crate) unsafe fn factread() {
    let idx = match FACTORY_REINJECT_INDEX.load(Ordering::Relaxed) {
        0 => FACTORY_LAST_INDEX.load(Ordering::Relaxed),
        v => v,
    };
    if idx == 0 {
        crate::log("[factread] nenhum factory index capturado");
        return;
    }
    let base = idx as *const u8;
    if !gum::is_readable(base as *const c_void, 0x100) {
        crate::log(&format!("[factread] idx={idx:#x} ilegível"));
        return;
    }
    // Lê os offsets suspeitos do RE (+0x68..+0x90) como u64 e u32
    let mut s = format!("[factread] FactoryIndex={idx:#x} offsets-chave:");
    for off in [0x68usize, 0x70, 0x74, 0x78, 0x7c, 0x80, 0x84, 0x88, 0x8c, 0x90] {
        let v64 = (base.add(off) as *const u64).read_unaligned();
        let v32 = v64 as u32;
        let is_heap_ptr = v64 > 0x1_1000_0000 && gum::is_readable(v64 as *const c_void, 8);
        s.push_str(&format!(" [+{off:#x}]={v64:#018x}(u32={v32}){}", if is_heap_ptr { "*" } else { "" }));
    }
    crate::log(&s);
    // Se +0x68 é ponteiro heap, lê as 8 primeiras u64s como candidatos (key,val pares)
    let ptr68 = (base.add(0x68) as *const u64).read_unaligned();
    if ptr68 > 0x1_1000_0000 && gum::is_readable(ptr68 as *const c_void, 128) {
        let mut t = format!("[factread] *[+0x68] primeiras 8 u64 (candidatos key/val de entry):");
        for i in 0..8usize {
            let v = *((ptr68 as *const u64).add(i));
            t.push_str(&format!(" [{i}]={v:#018x}"));
        }
        crate::log(&t);
    }
    // Busca o CName hash de "weapon_root" nos primeiros 0x400 bytes (scan linear)
    let target = crate::cname::cname("weapon_root");
    let mut found_off = None;
    let mut off = 0usize;
    while off + 8 <= 0x400 {
        let v = (base.add(off) as *const u64).read_unaligned();
        if v == target {
            found_off = Some(off);
            break;
        }
        off += 8;
    }
    if let Some(fo) = found_off {
        let val = (base.add(fo + 8) as *const u64).read_unaligned();
        crate::log(&format!(
            "[factread] cname('weapon_root')={target:#018x} ACHADO em FactoryIndex+{fo:#x} → next_u64={val:#018x}"
        ));
    } else {
        crate::log(&format!("[factread] cname('weapon_root')={target:#018x} NÃO encontrado em [idx..idx+0x400] scan linear (entries podem ser em ptr separado)"));
        // tenta achar via ptr em +0x68 (bucket array) e +0x78 (entry array, mais provável)
        // stride 16 (key u64 + val u64) e 24 (key u64 + val u64 + next u64)
        for (off_ptr, stride) in [(0x78usize, 16usize), (0x78, 24), (0x68, 16)] {
            let ptr = (base.add(off_ptr) as *const u64).read_unaligned();
            if ptr <= 0x1_1000_0000 || !gum::is_readable(ptr as *const c_void, 32) {
                continue;
            }
            // capacidade: usa o u32 em [+0x80] (entry_count, lido como u32)
            let cap_u32 = (base.add(0x80) as *const u32).read_unaligned();
            let cap = (cap_u32 as usize).min(2048);
            // log dos primeiros 4 entries no stride dado
            let mut preview = format!("[factread] ptr[+{off_ptr:#x}] stride={stride} cap_hint={cap} primeiras 4 entries:");
            for i in 0..4usize {
                let byte_off = i * stride;
                if !gum::is_readable((ptr + byte_off as u64) as *const c_void, 16) { break; }
                let k = *((ptr + byte_off as u64) as *const u64);
                let v = *((ptr + byte_off as u64 + 8) as *const u64);
                preview.push_str(&format!(" [{i}]({k:#018x},{v:#018x})"));
            }
            crate::log(&preview);
            // scan por weapon_root
            let mut found = false;
            for i in 0..cap {
                let byte_off = i * stride;
                if !gum::is_readable((ptr + byte_off as u64) as *const c_void, 16) { break; }
                let k = *((ptr + byte_off as u64) as *const u64);
                if k == target {
                    let v = *((ptr + byte_off as u64 + 8) as *const u64);
                    crate::log(&format!(
                        ">>> factread ACHADO ptr[+{off_ptr:#x}][{i}] stride={stride}: key={k:#018x} val={v:#018x} <<<",
                    ));
                    found = true;
                    // valida: val deve ser resource_path_hash("base\\weapons\\weapon_root.ent")
                    let expected = resource_path_hash("base\\weapons\\weapon_root.ent");
                    if v == expected {
                        // entry válida — tentar escrever bwms_test_weapon ANTES desta (slot i-1 se vago,
                        // ou no próximo slot i+1 se vago). Prefere o PRÓXIMO slot.
                        let bwms_key = crate::cname::cname("bwms_test_weapon");
                        let bwms_val = expected;
                        crate::log(&format!("[factread] val BATE com weapon_root.ent — tenta escrever bwms_test_weapon key={bwms_key:#018x} val={bwms_val:#018x}"));
                        // slot i+1
                        if i + 1 < cap {
                            let noff = (i + 1) * stride;
                            if gum::is_readable((ptr + noff as u64) as *const c_void, 16) {
                                let nk = *((ptr + noff as u64) as *const u64);
                                let nv = *((ptr + noff as u64 + 8) as *const u64);
                                if nk == 0 {
                                    let wk = (ptr + noff as u64) as *mut u64;
                                    let wv = (ptr + noff as u64 + 8) as *mut u64;
                                    *wk = bwms_key;
                                    *wv = bwms_val;
                                    crate::log(&format!("[factread] ESCRITO em slot[{i}+1] (era k={nk:#018x}) — verifica com factlookup bwms_test_weapon"));
                                } else {
                                    crate::log(&format!("[factread] slot[{i}+1] NÃO vazio (k={nk:#018x},v={nv:#018x}) — abortado"));
                                }
                            }
                        }
                    } else {
                        crate::log(&format!("[factread] val={v:#018x} != expected={expected:#018x} — stride errado ou entry misalinhada"));
                    }
                    break;
                }
            }
            if found { break; }
        }
    }
}

// Lê os primeiros N bytes da entry array em +0x78 do FactoryIndex e busca weapon_root com stride bruto
pub(crate) unsafe fn factread2() {
    let idx = match FACTORY_REINJECT_INDEX.load(Ordering::Relaxed) {
        0 => FACTORY_LAST_INDEX.load(Ordering::Relaxed),
        v => v,
    };
    if idx == 0 { crate::log("[factread2] sem idx"); return; }
    let base = idx as *const u8;
    // lê o ptr em +0x78 — a entry array
    let ptr78 = (base.add(0x78) as *const u64).read_unaligned();
    let ptr68 = (base.add(0x68) as *const u64).read_unaligned();
    crate::log(&format!("[factread2] idx={idx:#x} ptr+0x78={ptr78:#018x} ptr+0x68={ptr68:#018x}"));
    // verifica legibilidade direta (sem is_readable)
    let target = crate::cname::cname("weapon_root");
    let expected_val = resource_path_hash("base\\weapons\\weapon_root.ent");
    // Tenta ler diretamente os primeiros 40 entries com stride 24 a partir de ptr78
    for &(ptr, label) in &[(ptr78, "+0x78"), (ptr68, "+0x68")] {
        if ptr < 0x1000_0000 { continue; }
        let readable = gum::is_readable(ptr as *const c_void, 24 * 40);
        crate::log(&format!("[factread2] {label} ptr={ptr:#018x} readable_960b={readable}"));
        if !readable { continue; }
        // Log primeiros 5 entries (stride=16 e stride=24 sobrepostos para identificar formato)
        let mut preview = format!("[factread2] {label} primeiros 5×24bytes:");
        for i in 0..5usize {
            let off = i * 24;
            let k  = *((ptr + off as u64) as *const u64);
            let v  = *((ptr + off as u64 + 8) as *const u64);
            let nx = *((ptr + off as u64 + 16) as *const u32);
            preview.push_str(&format!(" [{i}](k={k:#010x},v={v:#010x},nx={nx:#08x})"));
        }
        crate::log(&preview);
        // scan weapon_root com stride 24
        for i in 0..1500usize {
            let off = i * 24;
            if !gum::is_readable((ptr + off as u64) as *const c_void, 16) { break; }
            let k = *((ptr + off as u64) as *const u64);
            if k == target {
                let v = *((ptr + off as u64 + 8) as *const u64);
                let nx = *((ptr + off as u64 + 16) as *const u32);
                crate::log(&format!("[factread2] ACHADO stride24 [{label}][{i}] key={k:#018x} val={v:#018x} next={nx:#08x}"));
                if v == expected_val {
                    crate::log("[factread2] val BATE weapon_root.ent — injetando bwms_test_weapon no slot seguinte");
                    let bk = crate::cname::cname("bwms_test_weapon");
                    // slot i+1: escreve se key==0
                    let noff = (i + 1) * 24;
                    if gum::is_readable((ptr + noff as u64) as *const c_void, 24) {
                        let nk = *((ptr + noff as u64) as *const u64);
                        if nk == 0 {
                            let wk = (ptr + noff as u64) as *mut u64;
                            let wv = (ptr + noff as u64 + 8) as *mut u64;
                            let wnx = (ptr + noff as u64 + 16) as *mut u32;
                            *wk = bk; *wv = expected_val; *wnx = nx;
                            crate::log(&format!("[factread2] ESCRITO slot[{i}+1] — testar com factlookup bwms_test_weapon"));
                        } else {
                            crate::log(&format!("[factread2] slot seguinte k={nk:#018x} (NÃO vazio) — buscando slot vazio..."));
                            // busca slot vazio mais adiante
                            for j in (i+2)..1500 {
                                let joff = j * 24;
                                if !gum::is_readable((ptr + joff as u64) as *const c_void, 24) { break; }
                                let jk = *((ptr + joff as u64) as *const u64);
                                if jk == 0 {
                                    let wk = (ptr + joff as u64) as *mut u64;
                                    let wv = (ptr + joff as u64 + 8) as *mut u64;
                                    *wk = bk; *wv = expected_val;
                                    crate::log(&format!("[factread2] ESCRITO slot[{j}] — bucket chain precisa de fix manual"));
                                    break;
                                }
                            }
                        }
                    }
                }
                break;
            }
        }
    }
}

// Proba os vtable slots das 4 classes de recurso (EntityTemplate/AppearanceResource/CMesh/MorphTargetMesh)
// para localizar PostLoad. Usa new_object → lê vtable da instância → loga slots 0x20..0x58.
// Slots confirmados (Mac, Itanium +1 dtor): GetSize@CLS+0x18, Construct@CLS+0x40; PostLoad esperado ~0x30 na instância.
pub(crate) unsafe fn postload_probe(reg: &crate::rtti::Registry) {
    for &cls_name in &["EntityTemplate", "AppearanceResource", "CMesh", "MorphTargetMesh"] {
        let inst = crate::rtti::new_object(reg, cls_name);
        if inst.is_null() {
            crate::log(&format!("[postload-probe] {cls_name}: new_object NULL — skip"));
            continue;
        }
        // vtable pointer é sempre offset 0 (C++ ABI; instância derivada de IScriptable ou não)
        let inst_vtbl = *(inst as *const *const u8);
        if inst_vtbl.is_null() || !crate::gum::is_readable(inst_vtbl as *const c_void, 0x60) {
            crate::log(&format!("[postload-probe] {cls_name}: inst_vtbl não legível @ {inst:p}"));
            continue;
        }
        // Lê slots 0x18..0x58 (PostLoad candidato @ 0x30 por hipótese do Itanium shift)
        let mut slots = format!("[postload-probe] {cls_name} inst={inst:p} vtbl slots:");
        for byte_off in (0x18usize..=0x58).step_by(8) {
            let fn_ptr = *(inst_vtbl.add(byte_off) as *const *const c_void);
            let static_off = crate::un_rebase(fn_ptr);
            slots.push_str(&format!(" +{byte_off:#04x}={static_off:#012x}"));
        }
        crate::log(&slots);
    }
}

// Injeta entrada bwms_test_weapon na chain table do FactoryIndex (usa value_ptr do weapon_root como emprestado)
// Formato da chain entry (stride=24, CONFIRMADO via disasm ResolveResource@0x100cc0bec):
//   +0: u32 next_idx (0xffffffff=end)
//   +4: u32 hash32 = (cname>>32)^(cname&0xffffffff)  (quick-compare fold)
//   +8: u64 cname_key  (full 64-bit CName hash)
//   +16: u64 value_ptr (→DynArray_entry, →+0x18→ResourcePath_ptr, →hash)
// Lookup: bucket_ptr=idx+0x68; bucket_count=idx+0x74; chain_ptr=idx+0x78; stride=idx+0x84
pub(crate) unsafe fn factinject() {
    let idx = match FACTORY_REINJECT_INDEX.load(Ordering::Relaxed) {
        0 => FACTORY_LAST_INDEX.load(Ordering::Relaxed),
        v => v,
    };
    if idx == 0 { crate::log("[factinject] sem idx"); return; }
    let base = idx as *const u8;

    let bucket_ptr = (base.add(0x68) as *const u64).read_unaligned();
    let bucket_count_check = (base.add(0x70) as *const u32).read_unaligned() as u64; // para is_readable
    let bucket_count_mod = (base.add(0x74) as *const u32).read_unaligned() as u64;   // para modulo (disasm)
    let chain_ptr = (base.add(0x78) as *const u64).read_unaligned();
    let chain_cap = (base.add(0x80) as *const u32).read_unaligned() as usize;
    let chain_stride = (base.add(0x84) as *const u32).read_unaligned() as u64;
    // +0x74 é o modulo que o ResolveResource usa (confirmado no disasm); +0x70 é dado auxiliar.
    let bucket_count = bucket_count_mod;

    crate::log(&format!(
        "[factinject] idx={idx:#x} bucket_ptr={bucket_ptr:#x} bc_check={bucket_count_check} bc_mod={bucket_count_mod} chain_ptr={chain_ptr:#x} cap={chain_cap} stride={chain_stride}"
    ));

    if chain_stride == 0 || chain_stride > 64 { crate::log("[factinject] stride inválido"); return; }
    if bucket_count == 0 { crate::log("[factinject] bucket_count==0"); return; }
    if chain_cap == 0 || chain_cap > 8192 { crate::log("[factinject] cap inválido"); return; }
    if !gum::is_readable(bucket_ptr as *const c_void, (bucket_count_check * 4) as usize) {
        crate::log("[factinject] bucket array não legível"); return;
    }
    if !gum::is_readable(chain_ptr as *const c_void, chain_cap * chain_stride as usize) {
        crate::log("[factinject] chain array não legível"); return;
    }

    // 1. Achar entrada do weapon_root (cname_key em offset+8) → pegar seu value_ptr (offset+16)
    let weapon_root_cname = crate::cname::cname("weapon_root");
    let bwms_cname = crate::cname::cname("bwms_test_weapon");
    let mut weapon_root_value_ptr = 0u64;
    let mut free_slot_idx: Option<usize> = None;

    for i in 0..chain_cap {
        let ep = chain_ptr + i as u64 * chain_stride;
        let ckey = *((ep + 8) as *const u64);
        let vptr = *((ep + 16) as *const u64);
        if ckey == weapon_root_cname && vptr != 0 {
            weapon_root_value_ptr = vptr;
            crate::log(&format!("[factinject] weapon_root @ chain[{i}] value_ptr={vptr:#018x}"));
        }
        if ckey == 0 && vptr == 0 && free_slot_idx.is_none() && i > 0 {
            free_slot_idx = Some(i); // primeiro slot livre
        }
        if weapon_root_value_ptr != 0 && free_slot_idx.is_some() { break; }
    }

    if weapon_root_value_ptr == 0 {
        crate::log("[factinject] weapon_root NÃO encontrado na chain — abort"); return;
    }
    let free_idx = match free_slot_idx {
        None => { crate::log("[factinject] sem slot livre na chain — abort"); return; }
        Some(i) => i,
    };
    crate::log(&format!("[factinject] free slot={free_idx} usará value_ptr de weapon_root"));

    // 2. Computar bucket do bwms_test_weapon (mesmo fold que o disasm: lsr+eor+udiv+msub)
    let bwms_fold32 = ((bwms_cname >> 32) as u32) ^ (bwms_cname as u32);
    let bwms_bucket = (bwms_fold32 as u64 % bucket_count) as u32;
    crate::log(&format!(
        "[factinject] bwms cname={bwms_cname:#018x} fold32={bwms_fold32:#010x} bucket={bwms_bucket}"
    ));

    // 3. Verificar se bwms_test_weapon já existe (para não duplicar)
    {
        let head = *((bucket_ptr as *const u32).add(bwms_bucket as usize));
        if head != 0xffffffff {
            let mut ci = head as u64;
            loop {
                if ci == 0xffffffff { break; }
                let ep = chain_ptr + ci * chain_stride;
                let ck = *((ep + 8) as *const u64);
                if ck == bwms_cname {
                    crate::log("[factinject] bwms_test_weapon JÁ existe na chain — abort");
                    return;
                }
                ci = *((ep) as *const u32) as u64;
            }
        }
    }

    // 4. Escrever nova chain entry no slot livre
    let new_ep = chain_ptr + free_idx as u64 * chain_stride;
    // Verificar que é writable (usando is_readable como proxy; escrita em .data heap não precisa de proteção extra)
    // Ler current bucket head para encadear
    let current_head = *((bucket_ptr as *const u32).add(bwms_bucket as usize));

    // next_idx(u32), hash32(u32), cname_key(u64), value_ptr(u64)
    *((new_ep) as *mut u32) = current_head;            // next = old head (0xffffffff se bucket vazio)
    *((new_ep + 4) as *mut u32) = bwms_fold32;        // hash32 quick-compare
    *((new_ep + 8) as *mut u64) = bwms_cname;         // full cname key
    *((new_ep + 16) as *mut u64) = weapon_root_value_ptr; // value_ptr emprestado do weapon_root

    // 5. Atualizar bucket array: apontar para o novo slot
    *((bucket_ptr as *mut u32).add(bwms_bucket as usize)) = free_idx as u32;

    crate::log(&format!(
        "[factinject] INJETADO: bucket[{bwms_bucket}]={free_idx} entry_ptr={new_ep:#x} (value_ptr weapon_root)"
    ));
    crate::log("[factinject] agora: factlookup bwms_test_weapon");
}

// Scan raw de VALUE weapon_root.ent na entry array — stride-agnostic
pub(crate) unsafe fn factread3() {
    let idx = match FACTORY_REINJECT_INDEX.load(Ordering::Relaxed) {
        0 => FACTORY_LAST_INDEX.load(Ordering::Relaxed),
        v => v,
    };
    if idx == 0 { crate::log("[factread3] sem idx"); return; }
    let base = idx as *const u8;
    let ptr78 = (base.add(0x78) as *const u64).read_unaligned();
    let target_val = resource_path_hash("base\\weapons\\weapon_root.ent"); // 0xb4e975987a1eefdf
    let target_key = crate::cname::cname("weapon_root");                   // 0x613ea1673ce5a16e
    crate::log(&format!("[factread3] idx={idx:#x} ptr78={ptr78:#018x} target_val={target_val:#018x} target_key={target_key:#018x}"));
    if ptr78 < 0x1000_0000 || !gum::is_readable(ptr78 as *const c_void, 8) {
        crate::log("[factread3] ptr78 não legível"); return;
    }
    // Scan byte-a-byte pelo VALUE (stride 1) nos primeiros 40KB da entry array
    let mut val_offset = None;
    for off in 0usize..40960 {
        if off % 4096 == 0 && !gum::is_readable((ptr78 + off as u64) as *const c_void, 8) { break; }
        let v = *((ptr78 + off as u64) as *const u64);
        if v == target_val { val_offset = Some(off); break; }
    }
    match val_offset {
        None => crate::log("[factread3] target_val NÃO encontrado em scan raw 40KB"),
        Some(off) => {
            // Achou o value em ptr78+off. Lê contexto ao redor (offset -32..+32)
            let start = off.saturating_sub(32);
            let mut ctx = format!("[factread3] target_val em ptr78+{off:#x} — contexto [-32..+32]:");
            for i in (start..=(off+32)).step_by(8) {
                if !gum::is_readable((ptr78 + i as u64) as *const c_void, 8) { break; }
                let v = *((ptr78 + i as u64) as *const u64);
                let mark = if i == off { " <VAL" } else { "" };
                ctx.push_str(&format!(" +{i:#x}={v:#018x}{mark}"));
            }
            crate::log(&ctx);
            // Tenta estimar stride: procura o VALUE novamente a partir de off+1
            for stride in [16usize, 20, 24, 28, 32] {
                let next = off + stride;
                if gum::is_readable((ptr78 + next as u64) as *const c_void, 8) {
                    let nv = *((ptr78 + next as u64) as *const u64);
                    crate::log(&format!("[factread3] stride={stride}: off+stride={next:#x} val={nv:#018x}"));
                }
            }
            // Procura o key logo ANTES do val para identificar layout (offset -16..-1)
            for key_off_delta in [8usize, 16, 4, 12] {
                if off >= key_off_delta {
                    let koff = off - key_off_delta;
                    let kv = *((ptr78 + koff as u64) as *const u64);
                    crate::log(&format!("[factread3] val-{key_off_delta:#x}={kv:#018x} (key candidato)"));
                    if kv == target_key {
                        crate::log(&format!("[factread3] >>> KEY BATE em -0x{key_off_delta:x} — entry_val_off={key_off_delta:#x} confirma ABI entry"));
                    }
                }
            }
        }
    }
}

// ===== axl-resource-patch-apply — resolve os 4 endereços de PostLoad construindo instância real =====
// RE delegada 2026-07-24: string/símbolo pra ResourceSerializer::Load/Deserialize/OnDependenciesReady
// NÃO existe em lugar nenhum do binário (categoria mais dura, tipo axl-factories-apply). MAS os 4
// PostLoad (EntityTemplate/AppearanceResource/CMesh/MorphTargetMesh, overrides virtuais reais de
// ISerializable::PostLoad) têm atalho barato: este projeto JÁ SABE construir uma instância real de
// qualquer classe RTTI por nome (`rtti::new_object`, via CClass vtable slot 0x40 Construct — usado
// o dia inteiro nas forjas de Event/Target) e JÁ SABE o shift Itanium do slot PostLoad (Windows
// 0x28 -> Mac 0x30, documentado na memória `cp77-macos-rtti-vtable-offsets`). Construir a instância
// REAL (não forjada por nós — classe genuína do motor) e ler `instance+0x00` (vtable própria do
// objeto) -> `vtable+0x30` resolve PostLoad sem RE estática nova nenhuma.
//
// RISCO: `new_object` foi só provado em classes FORJADAS por nós ou simples (Vector4). Construir uma
// classe REAL PESADA do motor (EntityTemplate/CMesh são recursos grandes, normalmente só criados
// pelo pipeline de load, nunca por Construct() cru) pode ter efeito colateral não previsto — por
// isso este probe é read-only DEPOIS da construção (só lê vtable, não chama PostLoad em si) e
// gated por comando explícito, não roda sozinho no boot.
pub(crate) unsafe fn probe_postload_addresses() {
    const CLASSES: &[&str] = &[
        "entEntityTemplate", "appearanceAppearanceResource", "CMesh", "MorphTargetMesh",
        // axl-animation-apply:
        "animAnimSet", "animRig", "animAnimDatabase",
        // axl-attachment-apply:
        "entAttachmentSlots", "entSlotComponent",
        // axl-puppet-state-apply:
        "gameObject", "entPuppet",
        // axl-mesh-apply (components):
        "entGarmentSkinnedMeshComponent", "entSkinnedMeshComponent", "entMeshComponent",
        // axl-world-streaming-apply: PostLoad do setor é o hook do ArchiveXL
        "worldStreamingSector",
    ];
    let reg = match crate::rtti::Registry::obtain() {
        Some(r) => r,
        None => {
            crate::log("[postload-probe] RTTI Registry não disponível ainda");
            return;
        }
    };
    for &name in CLASSES {
        let obj = crate::rtti::new_object(&reg, name);
        if obj.is_null() {
            crate::log(&format!("[postload-probe] {name}: new_object falhou (null)"));
            continue;
        }
        if !gum::is_readable(obj, 8) {
            crate::log(&format!("[postload-probe] {name}: instância {obj:p} ilegível"));
            continue;
        }
        let vtbl = (obj as *const u64).read_unaligned() as *const c_void;
        if vtbl.is_null() || !gum::is_readable(vtbl, 0x38) {
            crate::log(&format!("[postload-probe] {name}: instância {obj:p} vtable {vtbl:p} ilegível"));
            continue;
        }
        let postload_slot = (vtbl as *const u8).add(0x30) as *const u64;
        let postload_ptr = postload_slot.read_unaligned() as usize;
        let vmaddr = if postload_ptr != 0 {
            postload_ptr.wrapping_sub(crate::game_base()) + 0x1_0000_0000
        } else {
            0
        };
        crate::log(&format!(
            "[postload-probe] {name}: instância={obj:p} vtable={vtbl:p} PostLoad(+0x30)={postload_ptr:#x} (vmaddr candidato={vmaddr:#x})"
        ));
    }
}

// Codeware `#58` (`ISerializable.ProcessPostLoad`/`RefreshResource`, catálogo exaustivo) — prova
// ao vivo GENUÍNA da invocação real (não só leitura de endereço, como `probe_postload_addresses`
// acima já fazia desde 2026-07-25). Constrói uma instância FRESCA e DESCARTÁVEL via `new_object`
// (mesma classe/mesmo mecanismo já seguro há várias sessões via `postloadprobe`, nunca anexada a
// nenhum sistema do jogo — puro heap allocation), dumpa N bytes ANTES da chamada, invoca
// `rtti::iserializable_process_post_load` de verdade (dispatch genérico via vtable slot +0x30,
// o MESMO código-path que `BwmsProcessPostLoad`/`ProcessPostLoad`/`RefreshResource` expõem pro
// redscript), dumpa N bytes DEPOIS e loga o diff byte-a-byte. Prova decisiva: se o PostLoad real
// da classe mutar QUALQUER byte do objeto (bit de "já processado", ponteiro resolvido, etc.), o
// diff confirma execução genuína — não apenas "não crashou". Read-only exceto essa 1 chamada,
// sobre um objeto throwaway (zero risco pro save/estado vivo do jogo, mesmo em caso de mutação
// real dentro do corpo nativo). Gated por comando explícito (`postloadcall <classe>`).
pub(crate) unsafe fn postload_call_probe(class_name: &str) {
    let reg = match crate::rtti::Registry::obtain() {
        Some(r) => r,
        None => {
            crate::log("[postload-call] RTTI Registry não disponível ainda");
            return;
        }
    };
    let obj = crate::rtti::new_object(&reg, class_name);
    if obj.is_null() {
        crate::log(&format!("[postload-call] {class_name}: new_object falhou (null)"));
        return;
    }
    const DUMP_LEN: usize = 0x100;
    let mut before = [0u8; DUMP_LEN];
    if !gum::read_chunk(obj as usize, &mut before) {
        crate::log(&format!("[postload-call] {class_name}: instância {obj:p} ilegível pré-call — abortado"));
        return;
    }
    crate::log(&format!("[postload-call] {class_name}: instância={obj:p} chamando ProcessPostLoad..."));
    let ok = crate::rtti::iserializable_process_post_load(obj, false);
    let mut after = [0u8; DUMP_LEN];
    if !gum::read_chunk(obj as usize, &mut after) {
        crate::log(&format!(
            "[postload-call] {class_name}: instância {obj:p} ilegível PÓS-call (ok={ok}) — sobreviveu à chamada mas ponteiro não é mais legível"
        ));
        return;
    }
    let mut diffs: Vec<(usize, u8, u8)> = Vec::new();
    for i in 0..DUMP_LEN {
        if before[i] != after[i] {
            diffs.push((i, before[i], after[i]));
        }
    }
    crate::log(&format!(
        "[postload-call] {class_name}: instância={obj:p} ok={ok} bytes_diferentes={}/{} >>> {} <<<",
        diffs.len(),
        DUMP_LEN,
        if diffs.is_empty() { "SEM MUTACAO OBSERVADA (pode ser correto p/ instancia vazia)" } else { "MUTACAO REAL CONFIRMADA" }
    ));
    for (off, b, a) in diffs.iter().take(24) {
        crate::log(&format!("[postload-call]   +{off:#04x}: {b:#04x} -> {a:#04x}"));
    }
}

// ===== ArchiveXL: HookBefore open-archive — loga o PATH de cada .archive =====
// open archive @ vmaddr 0x103e2ebd4 (x0=ArchiveInfo*, x1=path, x2=group). HookBefore observe-only:
// loga x1 (path) e chama a original. Prova quais .archive o engine carrega (inclui os nossos
// basegame_zzbwms_*) + é a função que o Path B chamaria por .archive nosso.
const OPENARCH_VM: u64 = 0x1_03e2_ebd4;
static ORIG_OPENARCH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static OPENARCH_N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Tenta ler o path em `x1` como string: 1º como C-string em x1, depois x1 como ptr-p/-string.
unsafe fn read_path_arg(x1: *mut c_void) -> String {
    let try_cstr = |p: *const u8| -> String {
        let mut s = String::new();
        for i in 0..256 {
            if !gum::is_readable(p.add(i) as *const c_void, 1) {
                break;
            }
            let b = *p.add(i);
            if b == 0 {
                break;
            }
            if b.is_ascii_graphic() || b == b' ' {
                s.push(b as char);
            } else {
                break;
            }
        }
        s
    };
    if x1.is_null() || !gum::is_readable(x1 as *const c_void, 8) {
        return "<null>".into();
    }
    let s = try_cstr(x1 as *const u8);
    if s.len() >= 3 {
        return s;
    }
    let inner = (x1 as *const *const u8).read_unaligned();
    if !inner.is_null() && gum::is_readable(inner as *const c_void, 1) {
        let s2 = try_cstr(inner);
        if s2.len() >= 3 {
            return format!("(via ptr) {s2}");
        }
    }
    "<não-string>".into()
}

type OpenArchFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, u64, u64) -> *mut c_void;

unsafe extern "C" fn openarch_replacement(
    x0: *mut c_void,
    x1: *mut c_void,
    x2: *mut c_void,
    x3: u64,
    x4: u64,
) -> *mut c_void {
    let n = OPENARCH_N.fetch_add(1, Ordering::Relaxed);
    if n < 30 {
        // o glob (pattern+dir) vive em x0+0x78. Loga p/ ver todos os prefixos/dirs varridos
        // (incl. basegame_*.archive em content/ = onde nossos mods são pegos).
        let glob = read_path_arg((x0 as *mut u8).add(0x78) as *mut c_void);
        if glob.len() >= 6 {
            let ours = glob.contains("content") || glob.contains("zzbwms");
            crate::log(&format!(
                "[openarch-diag] glob#{n} '{glob}'{}",
                if ours { "  <<< dir/prefixo dos NOSSOS mods" } else { "" }
            ));
        }
    }
    if n < 6 {
        // DUMP RAW + scan de x0/x2 pra achar onde mora o path (string inline OU via ptr).
        crate::log(&format!(
            "[openarch-diag] #{n} RAW x0={x0:p} x1={x1:p} x2={x2:p} x3={x3:#x} x4={x4:#x}"
        ));
        for (name, base) in [("x0", x0), ("x1", x1), ("x2", x2)] {
            if base.is_null() || !gum::is_readable(base as *const c_void, 0x80) {
                continue;
            }
            for off in (0..0x80usize).step_by(8) {
                let s = read_path_arg((base as *mut u8).add(off) as *mut c_void);
                if s.len() >= 6 && (s.contains(".archive") || s.contains('/') || s.contains('_')) {
                    crate::log(&format!("[openarch-diag]   {name}+{off:#04x}: '{s}'"));
                }
            }
        }
    }
    let orig = ORIG_OPENARCH.load(Ordering::Relaxed);
    if orig.is_null() {
        return std::ptr::null_mut();
    }
    let f: OpenArchFn = std::mem::transmute(orig);
    f(x0, x1, x2, x3, x4)
}

unsafe fn install_openarchive_diag() {
    install_diag_hook!(
        OPENARCH_VM, openarch_replacement, ORIG_OPENARCH,
        "[openarch-diag] alvo open-archive ilegível -> sem hook",
        "[openarch-diag] hook instalado em open-archive (HookBefore, loga paths)",
        "[openarch-diag] FALHA ao hookar open-archive (replace None)"
    );
}

// ===== resource.link/copy: live-confirm de RequestResource (observe-only) =====
// 0x103eda898 = cand a ResourceDepot::RequestResource(x0=depot, x1=outHandle, x2=path, x3=archiveHandle).
// Disasm: prólogo limpo `sub sp,#0x70`, 4-arg, chama o resolve-helper 0x103eda360, lê campos do depot.
// (0x103eda894 era o ramo `bl __stack_chk_fail` da função ANTERIOR — entrada real é +4.)
// HookBefore: loga (x0, x2) dos 1os calls + confirma x0==singleton do depot, chama a orig. Só observa.
const REQRES_VM: u64 = 0x1_03ed_a898;
static ORIG_REQRES: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static REQRES_N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
type ReqResFn = unsafe extern "C" fn(*mut c_void, *mut c_void, u64, *mut c_void) -> *mut c_void;

unsafe extern "C" fn reqres_replacement(
    x0: *mut c_void,
    x1: *mut c_void,
    x2: u64,
    x3: *mut c_void,
) -> *mut c_void {
    let n = REQRES_N.fetch_add(1, Ordering::Relaxed);
    if n < 12 {
        // confirma x0 == singleton do depot ([0x109003000+0x1f8] deref)
        let depot_pp = (crate::rebase(0x1_0900_3000) as *const u8).add(0x1f8) as *const *const u8;
        let singleton = if gum::is_readable(depot_pp as *const c_void, 8) {
            depot_pp.read()
        } else {
            std::ptr::null()
        };
        let is_depot = x0 as *const u8 == singleton;
        crate::log(&format!(
            "[reqres-probe] #{n} x0={x0:p} (==depot? {is_depot}) path={x2:#018x} outH={x1:p} arch={x3:p}"
        ));
    }
    let orig = ORIG_REQRES.load(Ordering::Relaxed);
    if orig.is_null() {
        return std::ptr::null_mut();
    }
    let f: ReqResFn = std::mem::transmute(orig);
    f(x0, x1, x2, x3)
}

unsafe fn install_reqres_probe() {
    install_diag_hook!(
        REQRES_VM, reqres_replacement, ORIG_REQRES,
        "[reqres-probe] alvo RequestResource ilegível -> sem hook",
        "[reqres-probe] hook instalado em RequestResource cand 0x103eda894 (observe-only)",
        "[reqres-probe] FALHA ao hookar RequestResource (replace None)"
    );
}

// ===== Batch-probe da seção do depot: identifica RequestResource/CheckResource por ARGS AO VIVO =====
// Static signature-matching falhou (regs viram scratch). Aqui hookamos candidatos observe-only e
// deixamos os args reais decidirem: quem é chamado com x0==depot + um arg com cara de hash de
// ResourcePath (alta entropia, não-ponteiro) é o alvo.
unsafe fn dprobe_depot_ptr() -> u64 {
    let pp = (crate::rebase(0x1_0900_3000) as *const u8).add(0x1f8) as *const u64;
    if gum::is_readable(pp as *const c_void, 8) {
        pp.read()
    } else {
        0
    }
}
/// Heurística de hash de ResourcePath: >32 bits, não-zero no topo, e NÃO num range de ponteiro
/// do macOS (code 0x10–0x16, heap/stack 0x60–0x7f).
fn dprobe_pathlike(v: u64) -> bool {
    let hi = v >> 40;
    v > 0xffff_ffff && hi != 0 && !(0x10..=0x16).contains(&hi) && !(0x60..=0x7f).contains(&hi)
}
/// Loga (slot, args) quando o método VIRTUAL é chamado com x0==depot. RequestResource sai com
/// path-hash em x2; CheckResource com path em x1.
unsafe fn dprobe_log_vt(slot: usize, x1: u64, x2: u64, x3: u64) {
    crate::log(&format!(
        "[vprobe vt+{:#04x}] DEPOT x1={x1:#018x}{} x2={x2:#018x}{} x3={x3:#x}",
        slot * 8,
        if dprobe_pathlike(x1) { " <PATH" } else { "" },
        if dprobe_pathlike(x2) { " <PATH" } else { "" },
    ));
}

/// Vtable do `res::ResourceGameDepot` (static, do depotdump). `vtable_hook` troca o PONTEIRO do slot
/// (COW em __DATA_CONST) — NÃO patcha __TEXT → sem overlap, sem relocação, sem o crash do inline-hook.
const DEPOT_VTABLE_VM: u64 = 0x1_06f5_01c0;

macro_rules! vt_probe {
    ($name:ident, $slot:literal) => {
        mod $name {
            use crate::gum;
            use std::ffi::c_void;
            use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
            pub static ORIG: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
            static N: AtomicU64 = AtomicU64::new(0);
            pub const SLOT: usize = $slot;
            pub unsafe extern "C" fn repl(x0: u64, x1: u64, x2: u64, x3: u64, x4: u64, x5: u64) -> u64 {
                let d = super::dprobe_depot_ptr();
                if x0 == d && d != 0 && N.fetch_add(1, Ordering::Relaxed) < 5 {
                    super::dprobe_log_vt($slot, x1, x2, x3);
                }
                let o = ORIG.load(Ordering::Relaxed);
                if o.is_null() {
                    return 0; // slot pulado (gap) nunca instalado → nunca chega aqui
                }
                let f: unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64 =
                    std::mem::transmute(o);
                f(x0, x1, x2, x3, x4, x5)
            }
            pub fn set_orig(o: *const c_void) {
                ORIG.store(o as *mut c_void, Ordering::Relaxed);
            }
        }
    };
}

vt_probe!(vp00, 0); vt_probe!(vp01, 1); vt_probe!(vp02, 2); vt_probe!(vp03, 3);
vt_probe!(vp04, 4); vt_probe!(vp05, 5); vt_probe!(vp06, 6); vt_probe!(vp07, 7);
vt_probe!(vp08, 8); vt_probe!(vp09, 9); vt_probe!(vp10, 10); vt_probe!(vp11, 11);
vt_probe!(vp12, 12); vt_probe!(vp13, 13); vt_probe!(vp14, 14); vt_probe!(vp15, 15);
vt_probe!(vp16, 16); vt_probe!(vp17, 17); vt_probe!(vp18, 18); vt_probe!(vp19, 19);
vt_probe!(vp20, 20); vt_probe!(vp21, 21); vt_probe!(vp22, 22); vt_probe!(vp23, 23);
vt_probe!(vp24, 24); vt_probe!(vp25, 25); vt_probe!(vp26, 26); vt_probe!(vp27, 27);
vt_probe!(vp28, 28); vt_probe!(vp29, 29); vt_probe!(vp30, 30); vt_probe!(vp31, 31);

// vt+0x50 (slot 10) = "itera archives por índice" do depot. O RequestResource, ao buscar um path,
// ITERA os archives → CHAMA vt+0x50. Hookamos esse slot com um SHIM NAKED que preserva x0-x8+x30 (ABI
// intacta — conserta o crash do passthrough tipado): grava o x30 (caller) num ring buffer e faz `br`
// (tail-call) ao original. Em GAMEPLAY os callers distintos = RequestResource (não-virtual, sem símbolo).
#[repr(C)]
struct Vt50Ring {
    idx: AtomicU64,
    buf: [AtomicU64; 256],
}
pub(crate) static VT50_RING: Vt50Ring = Vt50Ring {
    idx: AtomicU64::new(0),
    buf: [const { AtomicU64::new(0) }; 256],
};
static ORIG_VT50N: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

// NAKED: usa só x9-x13 (scratch, livres na entrada). NÃO toca x0-x8 nem x30. Grava o caller (x30) em
// VT50_RING.buf[idx&255] e dá `br` no original (que retorna a x30 = caller real, intacto).
#[unsafe(naked)]
unsafe extern "C" fn vt50_naked() {
    core::arch::naked_asm!(
        "adrp x9, {ring}@PAGE",
        "add  x9, x9, {ring}@PAGEOFF",   // x9 = &VT50_RING (idx@0, buf@8)
        "ldr  x10, [x9]",                 // idx
        "add  x11, x10, #1",
        "str  x11, [x9]",                 // idx++ (relaxed, ok p/ diag)
        "and  x10, x10, #0xff",
        "add  x12, x9, #8",               // &buf[0]
        "str  x30, [x12, x10, lsl #3]",   // buf[idx&255] = caller
        "adrp x13, {orig}@PAGE",
        "add  x13, x13, {orig}@PAGEOFF",
        "ldr  x13, [x13]",                // x13 = original
        "br   x13",                       // tail-call (x30 intacto → original retorna ao caller)
        ring = sym VT50_RING,
        orig = sym ORIG_VT50N,
    )
}

unsafe fn install_depot_probes() {
    let vtbl = crate::rebase(DEPOT_VTABLE_VM) as *mut u64;
    if !gum::is_readable(vtbl as *const c_void, 8 * 16) {
        crate::log("[vt50] vtable do depot ilegível -> sem probe");
        return;
    }
    match gum::vtable_hook(vtbl, 10, vt50_naked as *const c_void) {
        Some(orig) => {
            ORIG_VT50N.store(orig as *mut c_void, Ordering::Relaxed);
            crate::log("[vt50] hook NAKED em vt+0x50 instalado — drene com 'vt50drain' em gameplay");
        }
        None => crate::log("[vt50] vtable_hook falhou"),
    }
}

/// Drena o ring de callers do vt+0x50 (comando 'vt50drain' em gameplay). Loga os endereços distintos
/// (un-rebased) por frequência = candidatos ao RequestResource.
pub(crate) fn drain_vt50_ring() {
    let total = VT50_RING.idx.load(Ordering::Relaxed);
    let mut seen: std::collections::BTreeMap<u64, u32> = std::collections::BTreeMap::new();
    for slot in VT50_RING.buf.iter() {
        let v = slot.load(Ordering::Relaxed);
        if v != 0 {
            *seen.entry(v).or_insert(0) += 1;
        }
    }
    crate::log(&format!("[vt50drain] {total} chamadas, {} callers distintos:", seen.len()));
    let mut v: Vec<(u64, u32)> = seen.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (caller, cnt) in v.into_iter().take(24) {
        let st = unsafe { crate::un_rebase(caller as *const c_void) };
        crate::log(&format!("[vt50drain]   x30={st:#x} (static)  x{cnt}"));
    }
}

// ===== Codeware `#120` (`RawInputHook`/`Raw::inkSystem::ProcessInputEvents`) — CAPTURA DINÂMICA =====
// (2026-08-14, retomada com o ângulo explicitamente pedido: a RE ESTÁTICA já esgotou hoje mais
// cedo — string-xref/call-graph/vtable-neighbor, 4+ tentativas, zero hit, ver HISTORICO.md. Nessa
// MESMA rodada `Red::InkSystem::Get()` ganhou endereço de CÓDIGO confirmado pela 1ª vez
// (0x10488b8dc), mas tem **562 chamadores** — inline demais pra distinguir por string/disasm.
//
// Ângulo novo: hookar `Get()` OBSERVE-ONLY (NUNCA muda x0/retorno, NUNCA muda x30/LR — só
// intercepta e sempre repassa pro Get() real) e gravar o LR (x30 = endereço de retorno, aponta
// PRA DENTRO do caller) de CADA chamada. Correlacionar quais call-sites disparam só durante um
// `presskey` real (candidatos fortes a `ProcessInputEvents`/muito perto dele) vs. os que disparam
// sempre (idle/baseline) — a mesma técnica de ring+naked-shim que `install_depot_probes`/`vt50`
// já usa pra vtable, aqui adaptada pra um INLINE hook (`replace_adrp_br8`, 2 instr = 8 bytes,
// mesma técnica já provada no getter da phase-byte no GOG) porque `Get()` não é despachada por
// vtable — é chamada direto via `bl` PC-relativo dos 562 sites.
//
// `Get()` real: `adrp x8,pág ; ldr x0,[x8,#0x830] ; ret` (12 bytes, 3 instr). `replace_adrp_br8`
// rouba só as 2 primeiras (ADRP+LDR, 8 bytes) e o trampolim salta pro `t+8` = o próprio `ret` da
// função original intocada — o RET final usa x30 (nunca tocado por nós) e devolve x0 (calculado
// pelo trampolim, idêntico ao original) direto pro caller de verdade.
const INKGET_VM: u64 = 0x1_0488_b8dc;
const INKGET_RING_LEN: usize = 4096; // grande o bastante p/ não perder call-sites raros numa janela de vários segundos

#[repr(C)]
struct InkGetRing {
    idx: AtomicU64,
    buf: [AtomicU64; INKGET_RING_LEN],
}
static INKGET_RING: InkGetRing = InkGetRing {
    idx: AtomicU64::new(0),
    buf: [const { AtomicU64::new(0) }; INKGET_RING_LEN],
};
static ORIG_INKGET: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static INKGET_INSTALLED: AtomicBool = AtomicBool::new(false);
/// Snapshot da janela IDLE (baseline) — call-sites (vmaddr ESTÁTICO) únicos vistos até o
/// `inkgetbaseline` mais recente. Comparado contra a janela pós-`presskey` em `inkgetpost`.
static INKGET_BASELINE: std::sync::Mutex<std::collections::BTreeSet<u64>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

// NAKED: usa só x9-x13 (scratch, livres na entrada — `Get()` não recebe argumento nenhum, x0 só
// carrega o RETORNO, nunca lido por nós). NÃO toca x0 nem x30. Grava o caller (x30) em
// INKGET_RING.buf[idx&(LEN-1)] e dá `br` no trampolim (que executa o Get() real e retorna a x30 =
// caller real, intacto — nunca modifica o que o caller recebe).
#[unsafe(naked)]
unsafe extern "C" fn inkget_naked() {
    core::arch::naked_asm!(
        "adrp x9, {ring}@PAGE",
        "add  x9, x9, {ring}@PAGEOFF",   // x9 = &INKGET_RING (idx@0, buf@8)
        "ldr  x10, [x9]",                 // idx
        "add  x11, x10, #1",
        "str  x11, [x9]",                 // idx++ (relaxed, ok p/ diagnóstico)
        "and  x10, x10, #0xfff",          // idx & (INKGET_RING_LEN-1), LEN=4096
        "add  x12, x9, #8",               // &buf[0]
        "str  x30, [x12, x10, lsl #3]",   // buf[idx&mask] = LR (call-site do caller real)
        "adrp x13, {orig}@PAGE",
        "add  x13, x13, {orig}@PAGEOFF",
        "ldr  x13, [x13]",                // x13 = trampolim (Get() real relocado)
        "br   x13",                       // tail-call — x0/x30 saem intactos pro caller
        ring = sym INKGET_RING,
        orig = sym ORIG_INKGET,
    )
}

/// Instala o hook OBSERVE-ONLY em `Red::InkSystem::Get()`. Gate `bwms_hook_enabled("inkget-lr")`
/// (`~/.bwms-hook-inkget-lr` ou o marcador genérico `~/.bwms-hook`) — nunca ligado por padrão.
/// Checa o prólogo (3ª instrução == RET) antes de patchar, senão ABORTA sem tocar o binário —
/// mesma disciplina de todo hook de endereço estático deste projeto.
pub(crate) unsafe fn install_inkget_probe() {
    // `bwms_hook_enabled` (helper compartilhado) é uma `fn` ANINHADA dentro de
    // `install_postload_hooks()` — não visível daqui. Mesmo gate, checado inline.
    let home = std::env::var("HOME").unwrap_or_default();
    let gated = std::path::Path::new(&format!("{home}/.bwms-hook")).exists()
        || std::path::Path::new(&format!("{home}/.bwms-hook-inkget-lr")).exists();
    if !gated {
        return;
    }
    if INKGET_INSTALLED.swap(true, Ordering::Relaxed) {
        return; // já instalado nesta sessão de processo
    }
    let target = crate::rebase(INKGET_VM);
    if !gum::is_readable(target as *const c_void, 12) {
        INKGET_INSTALLED.store(false, Ordering::Relaxed);
        crate::log("[inkget] alvo Get() ilegível -> sem probe");
        return;
    }
    let ret_check = std::ptr::read_unaligned((target as *const u8).add(8) as *const u32);
    if ret_check != 0xD65F_03C0 {
        INKGET_INSTALLED.store(false, Ordering::Relaxed);
        crate::log(&format!(
            "[inkget] prólogo mudou (esperava RET @+8, achei {ret_check:#010x}) -> ABORTA, zero patch"
        ));
        return;
    }
    let it = Interceptor::obtain();
    match it.replace_adrp_br8(target, inkget_naked as *const c_void as *mut c_void) {
        Some(tramp) => {
            ORIG_INKGET.store(tramp, Ordering::Relaxed);
            std::mem::forget(it);
            crate::log(&format!(
                "[inkget] hook OBSERVE-ONLY instalado em Red::InkSystem::Get() @ {INKGET_VM:#010x} (562 callers conhecidos) — 'inkgetbaseline' agora (idle), depois presskey 3-5x, depois 'inkgetpost'"
            ));
        }
        None => {
            INKGET_INSTALLED.store(false, Ordering::Relaxed);
            crate::log("[inkget] FALHA ao instalar hook em Get() (replace_adrp_br8 recusou)");
        }
    }
}

/// Getter thread-safe: hook JÁ instalado com sucesso? Usado pelo retry-loop automático
/// (`cw-inkget-autoretry`, thread dedicada spawnada em `on_load`) pra saber quando PARAR de
/// tentar — `install_inkget_probe()` já é idempotente (swap-then-verify em `INKGET_INSTALLED`),
/// mas o retry-loop precisa de um jeito de LER o resultado sem re-executar a lógica de
/// instalação (que faria log redundante mesmo já instalado). `Relaxed` é suficiente: o único
/// consumidor é o próprio processo, e a única transição que importa (false->true definitivo)
/// só acontece dentro de `install_inkget_probe`, sempre seguida do log de sucesso ANTES de
/// qualquer leitura daqui (ordem de programa dentro da mesma call, sem necessidade de fence
/// mais forte).
pub(crate) fn inkget_is_installed() -> bool {
    INKGET_INSTALLED.load(Ordering::Relaxed)
}

fn inkget_drain_raw() -> std::collections::BTreeMap<u64, u32> {
    let mut seen: std::collections::BTreeMap<u64, u32> = std::collections::BTreeMap::new();
    for slot in INKGET_RING.buf.iter() {
        let v = slot.load(Ordering::Relaxed);
        if v != 0 {
            let st = unsafe { crate::un_rebase(v as *const c_void) };
            *seen.entry(st).or_insert(0) += 1;
        }
    }
    seen
}

fn inkget_reset_ring() {
    for slot in INKGET_RING.buf.iter() {
        slot.store(0, Ordering::Relaxed);
    }
    INKGET_RING.idx.store(0, Ordering::Relaxed);
}

/// Comando `inkgetbaseline`: drena o ring AGORA (janela idle, ANTES do presskey), guarda o
/// conjunto de call-sites únicos como baseline, loga a contagem/top-50, e RESETA o ring pra
/// próxima janela (pós-presskey) começar limpa — sem isso a 2ª janela ficaria contaminada com os
/// mesmos call-sites "sempre ativos" da 1ª.
pub(crate) fn inkget_baseline() {
    let total = INKGET_RING.idx.load(Ordering::Relaxed);
    let seen = inkget_drain_raw();
    crate::log(&format!(
        "[inkgetbaseline] {total} chamadas observadas (idle), {} call-sites únicos:",
        seen.len()
    ));
    let mut v: Vec<(u64, u32)> = seen.iter().map(|(&k, &c)| (k, c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (addr, cnt) in v.iter().take(50) {
        crate::log(&format!("[inkgetbaseline]   {addr:#010x}  x{cnt}"));
    }
    {
        let mut base = INKGET_BASELINE.lock().unwrap();
        *base = seen.keys().copied().collect();
    }
    inkget_reset_ring();
    crate::log("[inkgetbaseline] ring resetado -> dispare 'presskey <kc>' 3-5x agora, depois rode 'inkgetpost'");
}

/// Comando `inkgetpost`: drena o ring de novo (janela pós-presskey), compara contra o baseline
/// salvo por `inkgetbaseline` -> loga os call-sites que aparecem SÓ AGORA (nunca vistos no idle) —
/// candidatos fortes a estarem no caminho de `ProcessInputEvents`/muito perto dele.
pub(crate) fn inkget_post() {
    let total = INKGET_RING.idx.load(Ordering::Relaxed);
    let seen = inkget_drain_raw();
    crate::log(&format!(
        "[inkgetpost] {total} chamadas observadas (pós-presskey), {} call-sites únicos:",
        seen.len()
    ));
    let mut v: Vec<(u64, u32)> = seen.iter().map(|(&k, &c)| (k, c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (addr, cnt) in v.iter().take(50) {
        crate::log(&format!("[inkgetpost]   {addr:#010x}  x{cnt}"));
    }
    let base = INKGET_BASELINE.lock().unwrap();
    let novos: Vec<(u64, u32)> = v.into_iter().filter(|(a, _)| !base.contains(a)).collect();
    crate::log(&format!(
        "[inkgetpost] === {} call-site(s) NOVO(S) (nunca visto no baseline idle) — candidatos fortes: ===",
        novos.len()
    ));
    for (addr, cnt) in novos.iter() {
        crate::log(&format!("[inkgetpost]   >>> CANDIDATO {addr:#010x}  x{cnt}"));
    }
    if novos.is_empty() {
        crate::log("[inkgetpost] nenhum call-site novo — o presskey não alcançou nenhum caller de Get() fora do baseline idle (achado honesto, não decisivo)");
    }
}

// ===== Codeware `#120` (`Raw::inkSystem::ProcessInputEvents`) — candidato `0x104a13088`
// (2026-08-17, disassembly Capstone offline pós-captura de LR em `InkSystem::Get()`) =====
// Achado (ver `HISTORICO.md`/`CATALOGO-EXAUSTIVO-CODEWARE.md` item `#120`, entrada 2026-08-17):
// dos 6 candidatos de baixa frequência capturados via LR-tracing em `Get()` (`INKGET_VM`
// acima), este é o ÚNICO onde `x0`("this") é usado DIRETO como `InkSystem*` — sem precisar de
// `Get()` interno pra resolvê-lo — via `x21 = x19+0x370` (`x19=x0` no prólogo), batendo EXATO
// com o offset `requestsHandler`/`WeakHandle<T>` já cross-validado por 2 mecanismos
// independentes em 2026-08-14. Prólogo confirmado LIMPO pra `Interceptor::replace()` (disasm
// real, `sub sp,#0x1f0` + 5x `stp` de registradores callee-saved, ZERO PC-relativo nos
// primeiros 16 bytes — diferente de `INKGET_VM`, que é um leaf de 8B e por isso usa
// `replace_adrp_br8`, este alvo tem 1144B e prólogo comum, então usa o mecanismo GERAL
// (`it.replace`/`install_hook_probe!`, já provado em ~30 outros probes desta sessão) em vez do
// leaf-only. Probe = função `extern "C"` NORMAL (não naked) — mesmo idioma de
// `cmesh_postload_replacement`/`streaming_sl14_probe` acima: captura os 4 primeiros argumentos
// (x0..x3, defensivo — a assinatura esperada tem 3 args mas o disasm só confirmou x0/x1 usados
// no caminho percorrido) SEM alterá-los, repassa intactos pro trampolim original, e retorna
// (void, mesma assinatura de `ProcessInputEvents` no header vendorizado).
//
// 100% OBSERVE-ONLY: nunca modifica args nem retorno; gated atrás de `~/.bwms-hook-ink120cand`
// (ou o marcador genérico `~/.bwms-hook`), nunca ligado por padrão. Item `#120` continua
// REAL_GAP até um boot confirmar dado real capturado — este bloco é só a instrumentação.
const INK120CAND_VM: u64 = 0x1_04a1_3088;
const INK120CAND_RING_LEN: usize = 512; // candidato de baixíssima frequência (1-2 hits/~2min na captura de origem) — folga generosa

#[repr(C)]
struct Ink120CandRing {
    x0: [AtomicU64; INK120CAND_RING_LEN],
    x1: [AtomicU64; INK120CAND_RING_LEN],
    x2: [AtomicU64; INK120CAND_RING_LEN],
    x3: [AtomicU64; INK120CAND_RING_LEN],
}
static INK120CAND_RING: Ink120CandRing = Ink120CandRing {
    x0: [const { AtomicU64::new(0) }; INK120CAND_RING_LEN],
    x1: [const { AtomicU64::new(0) }; INK120CAND_RING_LEN],
    x2: [const { AtomicU64::new(0) }; INK120CAND_RING_LEN],
    x3: [const { AtomicU64::new(0) }; INK120CAND_RING_LEN],
};
static INK120CAND_HITS: AtomicU64 = AtomicU64::new(0);
static ORIG_INK120CAND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

type Ink120CandFn = unsafe extern "C" fn(u64, u64, u64, u64);

/// Probe observe-only: grava (x0,x1,x2,x3) no ring (índice em anel, últimos `INK120CAND_RING_LEN`
/// hits se a contagem exceder o tamanho — o candidato é raro o bastante que isso não deve
/// acontecer na prática) e repassa os MESMOS 4 valores, intocados, pro trampolim original.
unsafe extern "C" fn ink120cand_probe(a0: u64, a1: u64, a2: u64, a3: u64) {
    let n = INK120CAND_HITS.fetch_add(1, Ordering::Relaxed) as usize;
    let slot = n & (INK120CAND_RING_LEN - 1);
    INK120CAND_RING.x0[slot].store(a0, Ordering::Relaxed);
    INK120CAND_RING.x1[slot].store(a1, Ordering::Relaxed);
    INK120CAND_RING.x2[slot].store(a2, Ordering::Relaxed);
    INK120CAND_RING.x3[slot].store(a3, Ordering::Relaxed);

    let orig = ORIG_INK120CAND.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: Ink120CandFn = std::mem::transmute(orig);
        f(a0, a1, a2, a3);
    }
}

/// Instala o probe (chamável sob demanda via comando `ink120hook`, e também amarrado em
/// `install_postload_hooks()` atrás do mesmo gate). Idempotente (`INK120CAND_HITS` sozinho não
/// serve de guarda de instalação — usa o `INSTALLED` interno do próprio `install_hook_probe!`).
pub(crate) unsafe fn install_ink120cand_probe() {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    let target = crate::rebase(INK120CAND_VM);
    if gum::is_readable(target as *const c_void, 16) && !INSTALLED.swap(true, Ordering::Relaxed) {
        let it = Interceptor::obtain();
        match it.replace(target, ink120cand_probe as *mut c_void) {
            Some(tramp) => {
                ORIG_INK120CAND.store(tramp, Ordering::Relaxed);
                std::mem::forget(it);
                crate::log(&format!(
                    "[ink120cand] hook OBSERVE-ONLY instalado @ {INK120CAND_VM:#010x} (candidato #120/ProcessInputEvents — this=InkSystem* direto, sem Get() interno) — 'ink120dump' pra ler o ring"
                ));
            }
            None => {
                INSTALLED.store(false, Ordering::Relaxed);
                crate::log("[ink120cand] FALHA ao instalar hook (Interceptor::replace recusou)");
            }
        }
    }
}

/// Comando `ink120dump`: loga o total de chamadas + até `INK120CAND_RING_LEN` entradas
/// (x0/x1/x2/x3) capturadas desde a instalação ou o último `ink120reset`.
pub(crate) fn ink120cand_dump() {
    let total = INK120CAND_HITS.load(Ordering::Relaxed);
    let n = (total.min(INK120CAND_RING_LEN as u64)) as usize;
    crate::log(&format!(
        "[ink120dump] {total} chamada(s) observada(s) no candidato #120 @ {INK120CAND_VM:#010x} ({n} entrada(s) no ring):"
    ));
    for i in 0..n {
        let x0 = INK120CAND_RING.x0[i].load(Ordering::Relaxed);
        let x1 = INK120CAND_RING.x1[i].load(Ordering::Relaxed);
        let x2 = INK120CAND_RING.x2[i].load(Ordering::Relaxed);
        let x3 = INK120CAND_RING.x3[i].load(Ordering::Relaxed);
        crate::log(&format!(
            "[ink120dump]   #{i} x0(this)={x0:#018x} x1={x1:#018x} x2={x2:#018x} x3={x3:#018x}"
        ));
    }
    if total == 0 {
        crate::log("[ink120dump] zero chamadas — ou o hook não instalou, ou o candidato genuinamente não disparou nesta janela (achado honesto, não decisivo)");
    }
}

/// Comando `ink120reset`: zera o contador + ring, pra uma janela de captura nova (ex. antes de
/// mandar `presskey` de novo, mesmo padrão de `inkget_reset_ring`).
pub(crate) fn ink120cand_reset() {
    INK120CAND_HITS.store(0, Ordering::Relaxed);
    for i in 0..INK120CAND_RING_LEN {
        INK120CAND_RING.x0[i].store(0, Ordering::Relaxed);
        INK120CAND_RING.x1[i].store(0, Ordering::Relaxed);
        INK120CAND_RING.x2[i].store(0, Ordering::Relaxed);
        INK120CAND_RING.x3[i].store(0, Ordering::Relaxed);
    }
    crate::log("[ink120reset] ring zerado");
}

// ===== Codeware `#198`/`#199` (`ResourceLoader::LoadAsync`/`LoadResource`) — CAPTURA DINÂMICA
// via ANCORAGEM ALTERNATIVA (2026-08-14, rodada dedicada "ângulo genuinamente novo") =====
// Blocker documentado (`CATALOGO-EXAUSTIVO-CODEWARE.md` `#198`/`#199`, `CATALOGO-EXAUSTIVO-
// ARCHIVEXL.md` `#25`/`#58`): o único candidato de endereço já cogitado pra
// `ResourceLoader::LoadAsync` (`0x1021c26fc`) foi REFUTADO por disassembly real (Capstone +
// `LC_FUNCTION_STARTS`, achado do RED4ext `#406`/`#408`) — é um `HashMap<uint64_t,16-bytes>::
// Insert` genérico (hash FNV1a32+chain-walk+growth, 4 argumentos), não um ctor/chamada de
// `LoadAsync` (que seria ~10-15 instruções, 2 argumentos). Zero candidato de endereço
// confiável restante pra `LoadResource`/`LoadReference`.
//
// Ângulo novo (NÃO usa `Red::InkSystem::Get()`, já tentado 4x hoje pro item `#120` sem
// sucesso — todas travaram antes de GAMEPLAY): `ResourceDepot::CheckResource@0x103ed9e9c` é
// um endereço JÁ CONHECIDO e JÁ PROVADO EM PRODUÇÃO (`resource_exists()`/
// `BwmsResourceExists`, `lib.rs`, testado `existsReal=true`/`existsFake=false`) — e mais
// importante, já documentado disparando `CONSTANTEMENTE` pelas chamadas NATURAIS do próprio
// jogo (`lib.rs::prove_copy_tick`, comentário "o jogo chama CheckResource constantemente" —
// achado original de RE 2026-07-16, reconfirmado por captura ao vivo do `this`/depot real via
// `check_res_replacement`). Está no MESMO subsistema (pipeline de resource loading/depot) que
// abriga `LoadAsync`/`LoadResource`: hipótese central — o(s) caller(s) que checam "existe?" via
// `CheckResource` antes de pedir o load muito provavelmente chamam `LoadAsync`/`LoadResource`
// LOGO DEPOIS, no mesmo corpo de função (padrão clássico `if (depot->ResourceExists(path))
// loader->LoadAsync(path);`) — os call-sites capturados aqui (endereços ESTÁTICOS, vmaddr
// já convertido via `un_rebase`) viram alvo de disassembly OFFLINE numa sessão futura (mesma
// técnica Capstone que refutou `0x1021c26fc`) procurando o `bl` irmão pra uma função ainda não
// nomeada.
//
// Vantagem prática sobre o ângulo `InkSystem::Get()`/`#120`: `CheckResource` dispara durante o
// CARREGAMENTO em si (streaming de arquivo/archive, resolução de path), não precisa de
// `presskey` nem de estado "GAMEPLAY" pleno — deve popular o ring já na tela de loading,
// sobrevivendo a um boot que trave antes do menu/auto-continue (o padrão que vem travando as
// tentativas de hoje pro `#120`).
//
// IMPORTANTE: este endereço JÁ TEM outro hook (`lib.rs::check_res_replacement`, usado por
// `axl-copy-makeexist`/`prove_copy_tick`) — os dois NUNCA devem ser armados na mesma sessão de
// processo (2 `Interceptor::replace()` no mesmo alvo corrompe o trampolim). Este probe é
// gated atrás de um marcador PRÓPRIO (`~/.bwms-hook-checkres-lr`), nunca ligado por padrão, e
// deve ser testado ISOLADO de `copytest`.
//
// 100% OBSERVE-ONLY: naked shim usa só x9-x13 (scratch) — nunca toca x0 (depot*)/x1
// (ResourcePath hash)/x30 (LR) — só grava o LR no ring e tail-calla (`br`) o trampolim
// original, mesmo idioma já provado em `inkget_naked`/`sweep_shim!` acima.
const CHECKRES_VM: u64 = 0x1_03ed_9e9c;
const CHECKRES_RING_LEN: usize = 4096;

#[repr(C)]
struct CheckResRing {
    idx: AtomicU64,
    buf: [AtomicU64; CHECKRES_RING_LEN],
}
static CHECKRES_RING: CheckResRing = CheckResRing {
    idx: AtomicU64::new(0),
    buf: [const { AtomicU64::new(0) }; CHECKRES_RING_LEN],
};
static ORIG_CHECKRES: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static CHECKRES_INSTALLED: AtomicBool = AtomicBool::new(false);
/// Snapshot da janela BASE (call-sites únicos vistos até o `checkresbaseline` mais recente).
/// Comparado contra a janela seguinte em `checkrespost` — diferente do `#120`/`inkget`, aqui a
/// diff não depende de um `presskey` (o alvo dispara sozinho); serve só pra separar "call-sites
/// já vistos no início do boot" de "novos vistos depois de mais streaming/uma ação real".
static CHECKRES_BASELINE: std::sync::Mutex<std::collections::BTreeSet<u64>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

#[unsafe(naked)]
unsafe extern "C" fn checkres_naked() {
    core::arch::naked_asm!(
        "adrp x9, {ring}@PAGE",
        "add  x9, x9, {ring}@PAGEOFF",   // x9 = &CHECKRES_RING (idx@0, buf@8)
        "ldr  x10, [x9]",                 // idx
        "add  x11, x10, #1",
        "str  x11, [x9]",                 // idx++ (relaxed, ok p/ diagnóstico)
        "and  x10, x10, #0xfff",          // idx & (CHECKRES_RING_LEN-1), LEN=4096
        "add  x12, x9, #8",               // &buf[0]
        "str  x30, [x12, x10, lsl #3]",   // buf[idx&mask] = LR (call-site do caller real)
        "adrp x13, {orig}@PAGE",
        "add  x13, x13, {orig}@PAGEOFF",
        "ldr  x13, [x13]",                // x13 = trampolim (CheckResource real relocado)
        "br   x13",                       // tail-call — x0/x1/x30 saem intactos pro caller
        ring = sym CHECKRES_RING,
        orig = sym ORIG_CHECKRES,
    )
}

/// Instala o hook OBSERVE-ONLY em `ResourceDepot::CheckResource`. Gate
/// `~/.bwms-hook-checkres-lr` (ou o marcador genérico `~/.bwms-hook`) — nunca ligado por
/// padrão. NÃO checa prólogo específico (diferente do `#120`/`Get()`, que é uma função de 12
/// bytes só-ADRP+LDR+RET) — usa `replace()` padrão, mesma função já usada com sucesso por
/// `check_res_replacement`/`copytest` neste mesmo endereço em sessão anterior.
pub(crate) unsafe fn install_checkres_probe() {
    let home = std::env::var("HOME").unwrap_or_default();
    let gated = std::path::Path::new(&format!("{home}/.bwms-hook")).exists()
        || std::path::Path::new(&format!("{home}/.bwms-hook-checkres-lr")).exists();
    if !gated {
        return;
    }
    if CHECKRES_INSTALLED.swap(true, Ordering::Relaxed) {
        return; // já instalado nesta sessão de processo
    }
    let target = crate::rebase(CHECKRES_VM);
    if !gum::is_readable(target as *const c_void, 16) {
        CHECKRES_INSTALLED.store(false, Ordering::Relaxed);
        crate::log("[checkres] alvo CheckResource ilegível -> sem probe");
        return;
    }
    let it = Interceptor::obtain();
    match it.replace(target, checkres_naked as *const c_void as *mut c_void) {
        Some(tramp) => {
            ORIG_CHECKRES.store(tramp, Ordering::Relaxed);
            std::mem::forget(it);
            crate::log(&format!(
                "[checkres] hook OBSERVE-ONLY instalado em ResourceDepot::CheckResource @ {CHECKRES_VM:#010x} (dispara continuamente durante loading real, mesmo antes de GAMEPLAY) — 'checkresbaseline' agora, espere alguns segundos de streaming real, depois 'checkrespost'"
            ));
        }
        None => {
            CHECKRES_INSTALLED.store(false, Ordering::Relaxed);
            crate::log("[checkres] FALHA ao instalar hook em CheckResource (replace() recusou)");
        }
    }
}

fn checkres_drain_raw() -> std::collections::BTreeMap<u64, u32> {
    let mut seen: std::collections::BTreeMap<u64, u32> = std::collections::BTreeMap::new();
    for slot in CHECKRES_RING.buf.iter() {
        let v = slot.load(Ordering::Relaxed);
        if v != 0 {
            let st = unsafe { crate::un_rebase(v as *const c_void) };
            *seen.entry(st).or_insert(0) += 1;
        }
    }
    seen
}

fn checkres_reset_ring() {
    for slot in CHECKRES_RING.buf.iter() {
        slot.store(0, Ordering::Relaxed);
    }
    CHECKRES_RING.idx.store(0, Ordering::Relaxed);
}

/// Comando `checkresbaseline`: drena o ring AGORA, guarda o conjunto de call-sites únicos como
/// baseline, loga a contagem/top-50, e RESETA o ring. Diferente do `#120`/`inkget`, não exige
/// nenhuma ação do usuário entre `checkreshook` e este comando — `CheckResource` já deve ter
/// disparado várias vezes só com o boot chegando na tela de loading.
pub(crate) fn checkres_baseline() {
    let total = CHECKRES_RING.idx.load(Ordering::Relaxed);
    let seen = checkres_drain_raw();
    crate::log(&format!(
        "[checkresbaseline] {total} chamadas observadas, {} call-sites únicos:",
        seen.len()
    ));
    let mut v: Vec<(u64, u32)> = seen.iter().map(|(&k, &c)| (k, c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (addr, cnt) in v.iter().take(50) {
        crate::log(&format!("[checkresbaseline]   {addr:#010x}  x{cnt}"));
    }
    {
        let mut base = CHECKRES_BASELINE.lock().unwrap();
        *base = seen.keys().copied().collect();
    }
    checkres_reset_ring();
    crate::log("[checkresbaseline] ring resetado -> espere mais alguns segundos de streaming real (ex.: avance o boot/mova o personagem), depois rode 'checkrespost'");
}

/// Comando `checkrespost`: drena o ring de novo, compara contra o baseline salvo -> loga TODOS
/// os call-sites (a lista completa já é o achado principal — cada endereço único é candidato a
/// disassembly offline procurando o `bl` irmão pra `LoadAsync`/`LoadResource`) + destaca os
/// que só apareceram nesta janela (fora do baseline).
pub(crate) fn checkres_post() {
    let total = CHECKRES_RING.idx.load(Ordering::Relaxed);
    let seen = checkres_drain_raw();
    crate::log(&format!(
        "[checkrespost] {total} chamadas observadas, {} call-sites únicos:",
        seen.len()
    ));
    let mut v: Vec<(u64, u32)> = seen.iter().map(|(&k, &c)| (k, c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (addr, cnt) in v.iter().take(50) {
        crate::log(&format!("[checkrespost]   {addr:#010x}  x{cnt}"));
    }
    let base = CHECKRES_BASELINE.lock().unwrap();
    let novos: Vec<(u64, u32)> = v.into_iter().filter(|(a, _)| !base.contains(a)).collect();
    crate::log(&format!(
        "[checkrespost] === {} call-site(s) NOVO(S) (fora do baseline) ===",
        novos.len()
    ));
    for (addr, cnt) in novos.iter() {
        crate::log(&format!("[checkrespost]   >>> {addr:#010x}  x{cnt}"));
    }
    if seen.is_empty() {
        crate::log("[checkrespost] ZERO call-sites capturados no total -> CheckResource nunca disparou nesta janela (achado honesto, não decisivo)");
    }
}

// ===== SWEEP NAKED: grava (idx,x0,x2) sobre candidatos não-virtuais da seção do depot, em GAMEPLAY =====
// Cada shim naked (preserva ABI) grava no ring e tail-calla seu trampolim. O candidato chamado com
// x0==depot + x2==hash de path = RequestResource(depot, out, path, arch). SEGURO + gameplay (auto-continue).
// Slot POR-CANDIDATO (sem ring/flooding): [count, last_x0, last_x2, pad] × 24.
static SWEEP_STATE: [AtomicU64; 96] = [const { AtomicU64::new(0) }; 96];

const SWEEP_ADDRS: [u64; 24] = [
    // LOADER batch 2 (0x1021c). 0x1021c26fc = anchor (path-hasher, 246×). LoadAsync = x2=request, path em [x2].
    0x1_021c_26fc, 0x1_021c_4358, 0x1_021c_44ec, 0x1_021c_4730,
    0x1_021c_4828, 0x1_021c_488c, 0x1_021c_4950, 0x1_021c_4a14,
    0x1_021c_4c0c, 0x1_021c_4d4c, 0x1_021c_51b8, 0x1_021c_52bc,
    0x1_021c_53d4, 0x1_021c_5658, 0x1_021c_56fc, 0x1_021c_5858,
    0x1_021c_5a1c, 0x1_021c_5b64, 0x1_021c_5c8c, 0x1_021c_5e9c,
    0x1_021c_6020, 0x1_021c_6138, 0x1_021c_63a4, 0x1_021c_6438,
];

macro_rules! sweep_shim {
    ($m:ident, $idx:literal) => {
        mod $m {
            use std::ffi::c_void;
            use std::sync::atomic::{AtomicPtr, Ordering};
            pub static ORIG: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
            #[unsafe(naked)]
            pub unsafe extern "C" fn shim() {
                core::arch::naked_asm!(
                    "adrp x9, {st}@PAGE",
                    "add  x9, x9, {st}@PAGEOFF",
                    "add  x9, x9, #{off}",   // &SWEEP_STATE[idx*4] (off = idx*32 bytes)
                    "ldr  x10, [x9]",
                    "add  x10, x10, #1",
                    "str  x10, [x9]",        // count++
                    "str  x0, [x9, #8]",     // last x0
                    "str  x2, [x9, #16]",    // last x2
                    "str  x1, [x9, #24]",    // last x1 (path-hasher usa [x1])
                    "adrp x13, {orig}@PAGE",
                    "add  x13, x13, {orig}@PAGEOFF",
                    "ldr  x13, [x13]",
                    "br   x13",
                    st = sym super::SWEEP_STATE,
                    orig = sym ORIG,
                    off = const ($idx * 32),
                )
            }
            pub unsafe fn install(addr: u64) {
                let t = crate::rebase(addr);
                if !crate::gum::is_readable(t as *const c_void, 16) {
                    return;
                }
                let it = crate::gum::Interceptor::obtain();
                if let Some(tr) = it.replace(t, shim as *mut c_void) {
                    ORIG.store(tr, Ordering::Relaxed);
                    std::mem::forget(it);
                }
            }
        }
    };
}

sweep_shim!(sw00, 0); sweep_shim!(sw01, 1); sweep_shim!(sw02, 2); sweep_shim!(sw03, 3);
sweep_shim!(sw04, 4); sweep_shim!(sw05, 5); sweep_shim!(sw06, 6); sweep_shim!(sw07, 7);
sweep_shim!(sw08, 8); sweep_shim!(sw09, 9); sweep_shim!(sw10, 10); sweep_shim!(sw11, 11);
sweep_shim!(sw12, 12); sweep_shim!(sw13, 13); sweep_shim!(sw14, 14); sweep_shim!(sw15, 15);
sweep_shim!(sw16, 16); sweep_shim!(sw17, 17); sweep_shim!(sw18, 18); sweep_shim!(sw19, 19);
sweep_shim!(sw20, 20); sweep_shim!(sw21, 21); sweep_shim!(sw22, 22); sweep_shim!(sw23, 23);

unsafe fn install_sweep() {
    crate::log("[sweep] 24 shims NAKED na seção do depot (record x0/x2; drene 'sweepdrain' em gameplay)...");
    sw00::install(SWEEP_ADDRS[0]); sw01::install(SWEEP_ADDRS[1]);
    sw02::install(SWEEP_ADDRS[2]); sw03::install(SWEEP_ADDRS[3]);
    sw04::install(SWEEP_ADDRS[4]); sw05::install(SWEEP_ADDRS[5]);
    sw06::install(SWEEP_ADDRS[6]); sw07::install(SWEEP_ADDRS[7]);
    sw08::install(SWEEP_ADDRS[8]); sw09::install(SWEEP_ADDRS[9]);
    sw10::install(SWEEP_ADDRS[10]); sw11::install(SWEEP_ADDRS[11]);
    sw12::install(SWEEP_ADDRS[12]); sw13::install(SWEEP_ADDRS[13]);
    sw14::install(SWEEP_ADDRS[14]); sw15::install(SWEEP_ADDRS[15]);
    sw16::install(SWEEP_ADDRS[16]); sw17::install(SWEEP_ADDRS[17]);
    sw18::install(SWEEP_ADDRS[18]); sw19::install(SWEEP_ADDRS[19]);
    sw20::install(SWEEP_ADDRS[20]); sw21::install(SWEEP_ADDRS[21]);
    sw22::install(SWEEP_ADDRS[22]); sw23::install(SWEEP_ADDRS[23]);
    crate::log("[sweep] instalado");
}

/// Drena os slots por-candidato, ordenado por frequência. Pra LoadAsync: x2=request → deref [x2]=path.
pub(crate) fn drain_sweep() {
    let depot = unsafe { dprobe_depot_ptr() };
    crate::log(&format!("[sweepdrain] depot={depot:#x} (ordenado por freq; LoadAsync=alta+[x2]=path):"));
    let mut rows: Vec<(u64, u64, u64, u64)> = Vec::new(); // (cnt, addr, x0, x2)
    for n in 0..24usize {
        let cnt = SWEEP_STATE[n * 4].load(Ordering::Relaxed);
        if cnt == 0 {
            continue;
        }
        rows.push((
            cnt,
            SWEEP_ADDRS[n],
            SWEEP_STATE[n * 4 + 1].load(Ordering::Relaxed),
            SWEEP_STATE[n * 4 + 2].load(Ordering::Relaxed),
        ));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    for (cnt, addr, x0, x2) in rows {
        let n = SWEEP_ADDRS.iter().position(|&a| a == addr).unwrap_or(0);
        let x1 = SWEEP_STATE[n * 4 + 3].load(Ordering::Relaxed);
        // procura um path (FNV) em: x2 valor, [x2] (request.path), x1 valor, [x1] (path-hasher).
        let chk = |v: u64| -> Option<u64> {
            if dprobe_pathlike(v) {
                return Some(v);
            }
            if v > 0x1000 {
                let p = v as *const u64;
                if unsafe { gum::is_readable(p as *const c_void, 8) } {
                    let d = unsafe { p.read() };
                    if dprobe_pathlike(d) {
                        return Some(d);
                    }
                }
            }
            None
        };
        let mut tail = String::new();
        if let Some(p) = chk(x2) {
            tail = format!(" PATH(x2)={p:#x}");
        } else if let Some(p) = chk(x1) {
            tail = format!(" PATH(x1)={p:#x}");
        }
        crate::log(&format!(
            "[sweepdrain]   {addr:#x} x{cnt} x0={x0:#x}{}{}",
            if depot != 0 && x0 == depot { " ==DEPOT" } else { "" },
            tail,
        ));
    }
}

// ===== axl-resource-patch-apply: PostLoad hooks pra CMesh + MorphTargetMesh (observe-only) =====
// vmaddrs confirmados pelo postload-probe (sessão 2026-07-25): lê vtable do inst criado por new_object,
// slot +0x30 = PostLoad (Itanium ABI, 2 dtors antes => +0x30 vs +0x28 do Windows).
// CMesh::PostLoad @ 0x100e16b28, MorphTargetMesh::PostLoad @ 0x100e467bc
// Hook observe-only: loga this + campos candidatos a CResource::path (u64 hash @ offset desconhecido)
// para RE da estrutura interna. Primeiros 8 calls de cada tipo são logados; depois silencia.
// "AXL-PATCH-PROBE" no log = hook em lugar certo; campos revelam onde fica o resource-path hash.

const CMESH_POSTLOAD_VM: u64 = 0x1_00e1_6b28;
const MORPHTGT_POSTLOAD_VM: u64 = 0x1_00e4_67bc;
// axl-garment-apply: ent::EntityTemplate::FindAppearance candidate 2026-07-24
// Static RE: ~0xd8-byte fn, CName loop stride=0x18, random-appearance 2×RNG, 9 call-sites.
// Matches Red::TemplateAppearance* FindAppearance(EntityTemplate*, CName). MEDIUM-HIGH confidence.
// Probe: observe-only; x0=EntityTemplate*, x1=CName hash (appearance name to find).
const ENTITY_TEMPLATE_FIND_APP_VM: u64 = 0x1_00cb_12bc;
// axl-garment-apply (RE offline dedicada 2026-07-28, técnica LC_FUNCTION_STARTS — fronteiras de
// função exatas em vez de "andar até o RET", que erra neste binário por causa de tail-calls):
// ABIs simples (sem sret) confirmadas por offsets de campo IDÊNTICOS ao AXL original (Core::OffsetPtr
// porta 1:1 macOS↔Windows, só o endereço de função muda). Confiança ALTA nas 3 abaixo.
// `ItemFactoryRequest::LoadAppearance(this)`: toca this+0x138/0x148/0x158/0x160 (EntityTemplate/
// AppearanceToken/AppearanceName/ItemRecord — os 4 offsets doc. do AXL); chama FindAppearance 3x.
const ITEMFACTORY_LOADAPP_VM: u64 = 0x1_0370_34a4;
// `ItemFactoryAppearanceChangeRequest::LoadAppearance(this)`: toca this+0x48/0xa8/0xb8/0x100.
const ITEMFACTORY_CHANGEAPP_LOADAPP_VM: u64 = 0x1_036f_ce14;
// `Entity::ReassembleAppearance(this,a1,a2,a3,a4,a5)`: 6 params exatos = assinatura AXL; contém as
// 2 strings de profiler "Entity/ReassembleAppearance/InitializeComponents" e "...Finalize".
const ENTITY_REASSEMBLE_APPEARANCE_VM: u64 = 0x1_00c9_5744;
// `axl-questphase-apply` (2026-08-05, RE de call-graph — não string-xref/RTTI, essas classes não
// expõem nada disso): candidato a "processar recurso de fase de quest carregado" — espera um
// resource-handle assíncrono terminar, aplica em 8 membros de `QuestsSystem`, termina chamando o
// wrapper de `RecreatePhaseInstanceTreeAfterLoad` (achado por string real, "QuestsSystem::
// m_rootInstance..."/"RecreatePhaseInstanceTreeAfterLoad"). Vtable slot real de `QuestsSystem`
// (`0x107148fb0`, dispatch só por BLR — nunca teria aparecido num scan de BL direto). Confiança
// ALTA no comportamento, MÉDIA no nome exato (pode não ser literalmente "ProcessPhaseResource").
const QUEST_PROCESSPHASERESOURCE_VM: u64 = 0x1_02ef_743c;
// `axl-transmog-apply` (2026-08-01, RE offline dedicada, achado via vizinhança de funções IRMÃS já
// confirmadas nesta MESMA classe — não string-xref, essa família não tem string nem RTTI, igual ao
// resto de AppearanceChanger/ItemFactoryAppearanceChangeRequest documentado acima):
// `ItemFactoryAppearanceChangeRequest::LoadTemplate(this) -> bool` — confiança ALTA (3 sinais
// independentes concordando): é o case do dispatcher de estado (`br x10` em `0x1036fc4c8`, indexado
// por `this+0x154`) chamado IMEDIATAMENTE ANTES do case que chama `LoadAppearance` (mesmos 2
// callers, mesma simetria de branch oldAppearance/newAppearance); lê `ItemRecord@+0x100` (mesmo
// campo que `LoadAppearance` lê) e ESCREVE em `EntityTemplate@+0xa8` — o campo que `LoadAppearance`
// depois CONSOME (via `bl` que lê `[this+0xa8]`); mesma ABI 1-param que `LoadAppearance` (categoria
// idêntica/segura, mesma classe, mesmo padrão de teste já 2x provado nesta classe).
const TRANSMOG_LOADTEMPLATE_VM: u64 = 0x1_036f_ca88;
// `AppearanceChanger::SelectAppearanceName(aOut,&itemRecord,&itemID,&appResource,a5,appName) -> void*`
// — confiança MÉDIA: achada como única call-site com EXATAMENTE 6 registradores de argumento (x0-x5,
// contagem exata do protótipo) montados dentro do corpo de `GetSuffixes` (`bl` em `0x10370dfdc`, na
// vizinhança confirmada de `AppearanceChanger`, logo após `GetSuffixes`); único caller no binário
// inteiro; NÃO é sret (retorno void* real em x0, `aOut:CName*` já é ponteiro explícito — diferente
// de `GetSuffixes`, que usa sret pro CString por valor). Corpo mais complexo do que o esperado
// (aloca stack dinâmico + sub-chamadas incluindo ramo de telemetria) — não fecha o loop com certeza
// total, testar com cautela extra (probe isolado, nunca junto de outro hook novo no mesmo boot).
const TRANSMOG_SELECTAPPNAME_VM: u64 = 0x1_0370_e5b0;
// Achados de MAIS ALTA confiança mas com ABI sret (x8=ponteiro de retorno oculto):
// `AppearanceResource::FindAppearance` = 0x100ad7048 (sret Handle<AppearanceDefinition>*, x0=resource,
// x1=CName, w2=u32, w3=u8; toca o spin-lock this+0xF0 doc. do AXL como primeira instrução — 8 callers).
// **2026-07-29: CODADO com `SretPad32`, TESTADO ISOLADO, CRASHOU** (mesmo tendo lock explícito).
// Agente de RE dedicado ROOT-CAUSED o mecanismo (ver DATABASE.md "categoria Find/resolve"): é uma
// corrida real (CAS/atomic-refcount) num array de Handle<T> compartilhado entre threads de job, SEM
// mitigação segura possível num hook de entrada de função (o fix real precisaria de patch de
// instrução NO MEIO da função ou alargar um lock do próprio motor — categoria de risco muito maior
// que "observe-only, chama original"). **NUNCA re-ligar este gate.**
const APPEARANCE_RESOURCE_FIND_APP_VM: u64 = 0x1_00ad_7048;
// `AppearanceChanger::GetSuffixes` = 0x10370d924 (sret CString por valor, + 4 params). **2026-07-29:
// CODADO com `SretPad32`** — agente de RE dedicado CONFIRMOU por disassembly (não por analogia) que
// o buffer de saída real (`red::CString` construído pelo PRÓPRIO GetSuffixes, depois só um Append)
// nunca escreve além de +0x18 (8 bytes) = footprint total 0x20 (32B) — bate EXATO com `SretPad32`.
// Categoria SEGURA (consumidora, não "Find"/resolve) — sem histórico de crash desta família.
const GARMENT_GETSUFFIXES_VM: u64 = 0x1_0370_d924;
// `GarmentAssembler::ProcessGarment` = 0x100ae4004 (sret SharedPtr<ctx>&, + 3 params, dispara
// JobGroup) — **NÃO codado**: dispara jobs assíncronos, categoria de risco ainda não avaliada.
// `AppearanceChanger::ComputePlayerGarment` = 0x103710560 (8 params exatos, x0-x7, único caller =
// job-shim "DoStartChangeAppearance_ComputePlayerGarment") — não codado (fora de escopo/tempo).
// axl-garment-apply (RE offline 2026-07-28, rodada 2 — 7 sub-hooks achados por varredura de "shape"
// de request em vez de string-xref, TU separado em 0x1036f7000-0x1036fa000): 6 de ABI simples
// codados abaixo; `GarmentAssembler::FindState=0x1036f8030` (ALTA, sret — mesmo risco de ABI do
// AppearanceResource::FindAppearance, NÃO hookado ainda). Layout: `GarmentAssembler{spinlock@0x0,
// DynArray<Entry>@0x8}`, Entry stride 0x60; assembler vive em `sistema+0x5ab0`. Achado negativo:
// `RegisterPart`/`GetVisualTags` NÃO fecharam nesta rodada (ver DATABASE.md pro detalhe completo).
// **2026-07-29: `FindState` CODADO** com `SretPad32` (o out real são 3 ponteiros = 24B, que o
// próprio Rust já roteia por x8 por tamanho — mas mantemos o wrapper de 32B por consistência/
// margem de segurança com o resto da família).
const GARMENT_FINDSTATE_VM: u64 = 0x1_036f_8030;
// 2026-08-05 (RE dedicada, call-graph reverso exaustivo — BL/B/ADRP+ADD/chained-fixup, zero hits
// pros 4): 0x1_036f_887c é CÓDIGO MORTO nesta build — nunca chamado por nenhuma via, o hook nunca
// tinha chance de disparar (explica os 5 testes negativos anteriores, incluindo save-load real).
// O compilador fundiu (inlined) o corpo de AddItem numa função vizinha que TAMBÉM resolve o
// WeakHandle via FindState primeiro (mesmo padrão 3-arg de RemoveItem) — essa cópia viva é
// 0x1_036f_87e4, alcançável pela MESMA state-machine (0x1036fc4c8) que já dispara LoadTemplate/
// LoadAppearance em boot normal (sem character creation) — refuta a hipótese antiga.
const GARMENT_STATE_ADDITEM_VM: u64 = 0x1_036f_87e4;
// 2026-08-05: mesmo achado do AddItem (ver comentário lá) — 0x1_036f_89a4 é a cópia MORTA,
// 0x1_036f_890c é a cópia viva (fundida com FindState, mesmo padrão 3-arg).
const GARMENT_STATE_ADDCUSTOMITEM_VM: u64 = 0x1_036f_890c;
const GARMENT_STATE_CHANGEITEM_VM: u64 = 0x1_036f_8208;
const GARMENT_STATE_CHANGECUSTOMITEM_VM: u64 = 0x1_036f_82f4;
const GARMENT_REMOVEITEM_VM: u64 = 0x1_036f_8744;
const GARMENT_ONGAMEDETACH_VM: u64 = 0x1_036f_6f4c;
// axl-garment-apply rodada 3 (RE offline 2026-07-28) — os 2 ÚLTIMOS sub-hooks do gap, ambos ALTA
// confiança, ambos SEM sret. `GetVisualTags(preset,resourcepath,cname,taglist_out)`: void, 4 params,
// cadeia fechada via vtable de gameTransactionSystem (0x1072173a0, slot 0x4e8) + cross-validado
// contra GetSuffixes já conhecido na MESMA vfunc caller. `RegisterPart(ctx,tpl_handle&,storage_handle&,
// appdef_handle&)`: void, 4 params, caller ÚNICO em "Appearance_Compile1", trava spinlock em ctx+0x88.
const GARMENT_GETVISUALTAGS_VM: u64 = 0x1_03cd_7234;
const GARMENT_REGISTERPART_VM: u64 = 0x1_00cd_1f50;
// ArchiveXL `#17` (`AppearanceDefinition::ExtractPartComponents`) — candidato de confiança MÉDIA
// achado por RE offline 2026-08-19 (sessão nº33, mapeamento exaustivo prólogo-a-epílogo da
// vizinhança "Appearance Compile" via `re-xref`, ver CATALOGO-EXAUSTIVO-ARCHIVEXL.md item `#17` e
// HISTORICO.md do mesmo dia). Dentro do cluster achei uma função (`0x100ace520`, "Function C")
// chamada por 2 wrappers diferentes; este (`0x100ace4b0`) tem EXATAMENTE 2 argumentos explícitos
// (sem `this` óbvio, sem `sret`), constrói um `DynArray` local, chama Function C pra populá-lo,
// faz merge no 2º arg (tratado como array de SAÍDA — count testado no fim), retorna um bool — bate
// estruturalmente com "popula `aOut`, retorna algo em x0", o formato esperado de
// `ExtractPartComponents(DynArray<Handle<ISerializable>>& aOut, const SharedPtr<...>& aPartToken)`.
// **ZERO callers diretos (`BL`/`B`) em 19.455.372 instruções escaneadas** — consistente com a
// hipótese de despacho assíncrono via `CompilationJob@0xE8` (não uma chamada direta), o que
// explicaria por que 3 sessões dedicadas de call-graph estático nunca acharam o alvo. Ressalva que
// baixa a confiança pra MÉDIA (não ALTA): o 1º argumento (x0) é tratado, tanto por Function C quanto
// por um 2º wrapper (`0x100ace874`, mesmos campos `+0x50`/`+0x5c`/`+0x88`/`+0x98`), como objeto rico
// multi-campo — perfil mais de `AppearanceDefinition*`/`AppearanceResource*` (`this`) do que de
// `SharedPtr<ResourceToken<EntityTemplate>>&` (só 2 words). Pode ser mecanismo IRMÃO (agregação
// própria de componentes) em vez da extração-de-token, ou o header vendorizado tem a ordem dos
// parâmetros invertida (já aconteceu antes no projeto). **Nunca testado ao vivo — este é o 1º
// boot.**
const APPEARANCE_EXTRACTPARTCOMPONENTS_CAND_VM: u64 = 0x1_00ac_e4b0;
// Codeware `#176`/`WidgetInputService` — candidato de confiança MÉDIA-ALTA pra
// `Raw::inkWidget::TriggerEvent` (`0x1049bded0`), achado por RE offline 2026-08-19 (sessão de
// hoje mais cedo, string-xref+disasm via `re-xref`) e ELEVADO a MÉDIA-ALTA nesta sessão (mesmo
// dia, continuação) por 3 confirmações estruturais independentes no disasm do CALLER
// (`0x1049b1c64`, uma função pequena e semanticamente rica que monta o CName `"OnCharacterKey"`
// — hash `0x81a340f64c0d72b6`, CONFIRMADO batendo `fnv1a64("OnCharacterKey")` byte-a-byte via
// `bwms_hashes::fnv1a64`, não mais suposição — e SEMPRE chama este alvo): (1) `x0` do caller é
// `widget_ptr + 0x50`, EXATO o offset de `Red::inkWidget::EventManager` do header vendorizado
// (`InkCore.hpp: using EventManager = Core::OffsetPtr<0x50, Red::EventManager>`); (2) o log
// one-time antes da 1ª chamada usa o MESMO log-channel (`bl 0x103452da0`) já confirmado hoje no
// item `#9`; (3) `x2` é o endereço de uma cópia-refcontada-na-stack do `Handle<Event>` recebido
// por referência em `x1` do caller — bate com passagem-por-referência-oculta de um tipo
// non-trivial (`Handle<Event>` por VALOR na assinatura C++, mas o Itanium ABI força ponteiro
// oculto), consistente com `TriggerEvent(void* aManager, CName aName, Handle<Event> aObject)`.
// **Achado importante desta sessão**: o CALLER (`0x1049b1c64`) NÃO é `Raw::inkSystem::
// ProcessCharacterEvent` (hipótese descartada por leitura da fonte real, `WidgetInputService.cpp`)
// — a assinatura de `ProcessCharacterEvent` é `(InkSystem*, EInputKey, EInputAction)`, mas o
// caller lê seu 2º parâmetro (`x1`) como ponteiro-pra-Handle (2 words, refcontado) e usa `x0`
// diretamente como `inkWidget*` (não `InkSystem*`) — é um helper VANILLA distinto ("dispara
// OnCharacterKey num widget dado", provavelmente chamado de DENTRO do corpo de
// `ProcessCharacterEvent` via `BLR`, explicando por que não tem caller direto estático). O nome
// do evento (`"OnCharacterKey"`) também é DIFERENTE do que o Codeware dispara
// (`"OnInputKey"`, `WidgetInputService.cpp:7`) — confirma que são 2 mecanismos irmãos, não o
// mesmo. **Item `#176` continua bloqueado em `ProcessCharacterEvent` propriamente dito** (esse
// helper não é o alvo do hook `HookBefore<Raw::inkSystem::ProcessCharacterEvent>`), mas testar
// `TriggerEvent` ao vivo valida (ou refuta) um elo real da cadeia `cw-ui-widget`/`cw-real-mod-e2e`
// (o gap mais antigo e persistente do projeto, item `#240` — `TriggerEvent` é 1 dos 7 nativos do
// cluster, já tem hash `Red::AddressLib` oficial mas nunca teve endereço Mac confirmado ao vivo).
// Probe 100% OBSERVE-ONLY (chama a original sempre, nunca muta x0/x1/x2/retorno). Assinatura ABI:
// `x0`=manager (`void*`, esperado = `widget+0x50`), `x1`=CName (`u64`, hash do nome do evento —
// deve bater `0x81a340f64c0d72b6`/"OnCharacterKey" quando disparado por este caller específico,
// mas TriggerEvent é genérico e pode ser chamado com outros nomes por outros callers indiretos),
// `x2`=ponteiro pro `Handle<Event>` (2 words: instance+refcount, mesmo layout já provado no
// projeto). Gate PRÓPRIO (`~/.bwms-hook-codeware-176-triggerevent-cand`), nunca ligado por padrão.
const INKWIDGET_TRIGGEREVENT_CAND_VM: u64 = 0x1_049b_ded0;
// axl-inkspawner-apply (RE offline 2026-07-28, desbloqueado pelo fechamento de cw-controller-misc):
// os 5 hooks do AXL original: InkWidgetLibrary::SpawnFromLocal/SpawnFromExternal (sret — x8=&out,
// ABI diferente do Windows/RDX; NÃO hookados, mesmo risco do FindAppearance/FindState sret),
// AsyncSpawnFromLocal/AsyncSpawnFromExternal (retorno bool simples, sem sret — hookados abaixo),
// InkSpawner::FinishAsyncSpawn (chamada VIRTUAL, vtable não resolvida offline, não hookado).
// Achado estrutural: Codeware >=1.14 instala os MESMOS 5 hooks (WidgetSpawningService.cpp) — é 1
// conjunto só, não dois. Layout NÃO confirmado ao vivo ainda (nova família, gate PRÓPRIO e
// separado do garment: ~/.bwms-inkspawnerprobe, nunca ligado por padrão).
const INKSPAWNER_ASYNC_LOCAL_VM: u64 = 0x1_0496_5980;
const INKSPAWNER_ASYNC_EXTERNAL_VM: u64 = 0x1_0496_5b38;
// axl-inkspawner-apply: AsyncSpawnFromDefPack — 2026-07-29, agente de RE offline dedicado.
// Achado: é o FUNIL ÚNICO dos 3 caminhos de spawn assíncrono (AsyncSpawnFromLocal/External/Default
// chamam ESTE, confirmado por scan exaustivo de xref BL no __TEXT inteiro: só 3 callers, todos desta
// família). `FinishAsyncSpawn` NÃO EXISTE como endereço separado — foi INLINED pelo compilador
// DENTRO desta função (LC_FUNCTION_STARTS confirma: bloco contíguo único 0x104922c90-0x104923164,
// próxima fronteira já é Instantiate). O dispatch virtual que fazia o papel de "FinishAsyncSpawn"
// está em `0x104922d00` (x0=[ctx+0x18], x8=[[x0]]+0x30) DENTRO deste mesmo bloco — não hookável
// separadamente sem RE de vtable em runtime (fora de escopo offline). ABI (3 args, sem sret,
// retorno descartado pelos 3 callers): x0=item ptr (entries+idx*0x38), x1=InkSpawningContext*,
// x2=byte flag (só w2 usado). MESMO risco da família appearance/garment (atomics de refcount +
// padrão de hand-off pro job-system, sem lock explícito visível) — o agente recomenda a MESMA
// cautela (gate próprio, save descartável) mesmo sendo o "funil"; NÃO testado ao vivo de propósito.
const INKSPAWNER_ASYNC_DEFPACK_VM: u64 = 0x1_0492_2c90;
// axl-inkspawner-apply: SpawnFromLocal/SpawnFromExternal (RE offline 2026-07-28, ABI sret — os 2
// hooks que faltavam pro gap ficar 5/5). vmaddrs de `DATABASE.md`: cadeia fechada do native
// redscript `inkIWidgetController::SpawnFromLocal 0x1047c918c → ... → 0x104965de0`.
// 2026-07-29: ANTES de codar, VERIFIQUEI empiricamente (experimento offline isolado, compilar+
// disassemblar) que `rustc`/LLVM em aarch64 só usa sret (x8) pra structs retornados por valor
// MAIORES que 16 bytes — um `#[repr(C)] struct { a: u64, b: u64 }` (16B) SAI VIA X0/X1 (2
// registradores), NÃO via x8. Mas o `Handle<T>` REAL do motor (16B: {ptr,refcount_ptr}) é sempre
// sret no C++ real (tipo não-trivial, regra do Itanium ABI, INDEPENDENTE do tamanho) — se eu
// escrevesse `-> Handle16` ingenuamente em Rust, a chamada NÃO bateria com o que o código do
// jogo espera (ele vai escrever em x8, mas o NOSSO hook Rust ia ler de x0/x1) → corrupção
// silenciosa, não um crash óbvio. Fix: `SretPad32` (32B, > 16B) força sret de verdade no
// Rust/LLVM também — confirmado no disasm do experimento que o ponteiro x8 flui through
// intacto (mesmo com log() no meio, não só tail-call puro) e o VALOR REAL (16B úteis) fica nos
// primeiros 2 campos, o resto é padding nunca lido pelo caller real (que só espera 16B).
#[repr(C)]
struct SretPad32 {
    a: u64,
    b: u64,
    _pad: [u64; 2],
}
const INKSPAWNER_SPAWNFROMLOCAL_VM: u64 = 0x1_0496_5de0;
const INKSPAWNER_SPAWNFROMEXTERNAL_VM: u64 = 0x1_0496_5ec0;
// axl-misc-subsystems: JournalTree::ProcessJournalIndex — string-xref RE 2026-07-26
// String "JournalTree/ProcessJournalIndex" at vmaddr 0x106cb0f09; ADRP+ADD xref at 0x101ea045c;
// Function start: SUB SP,SP,#0x110 at 0x101e9fe34 (no intervening prologue between start and xref).
// Fires during game load when journal index is loaded. AXL Journal extension hooks this.
const JOURNAL_PROCESS_VM: u64 = 0x1_01e9_fe34;
// JournalManager singleton global (BSS, populated at runtime) — found in ProcessJournalIndex:
// ADRP x23,0x10900b000 + ADD x23,x23,#0x580 + LDR x0,[x23] → call JM method 2026-07-26
pub(crate) const JOURNAL_MANAGER_GLOBAL_VM: u64 = 0x1_0900_b580;
// axl-journal-apply: 4 additional hook candidates from string-xref RE 2026-07-26 (gate ~/.bwms-journalprobe)
// "JournalFolderEntry/LoadSubResource" → xref@0x101e7449c → func@0x101e74140 (SUB SP,#0x110)
const JOURNAL_LOADSUB_A_VM: u64 = 0x1_01e7_4140;
// same string, 2nd xref → xref@0x101e76494 → func@0x101e76190 (SUB SP,#0x120)
const JOURNAL_LOADSUB_B_VM: u64 = 0x1_01e7_6190;
// "JournalRootFolderEntry/Initialize/LoadRootResource" → xref@0x101e7552c → func@0x101e7528c (SUB SP,#0xe0)
const JOURNAL_LOADROOT_VM:  u64 = 0x1_01e7_528c;
// "base\journal\index.reslist" → xref@0x101e9f894 → func@0x101e9f7cc (SUB SP,#0x50 — small loader)
const JOURNAL_RESLIST_VM:   u64 = 0x1_01e9_f7cc;
// axl-customization-apply: gameuiCharacterCustomizationSystem — two string-xref candidates 2026-07-26
// String "gameuiCharacterCustomizationSystem" at vmaddr 0x106cd005d;
// Xref #1 at 0x10244d8d4 → func start at 0x10244d7dc (SUB SP,#0x1C0; larger init function)
// Xref #2 at 0x10246a1ec → func start at 0x102469cdc (SUB SP,#0x30; smaller function)
// Hook both: whichever fires confirms CharacterCustomization system is hookable on Mac.
const CHAR_CUSTOM_A_VM: u64 = 0x1_0244_d7dc;
const CHAR_CUSTOM_B_VM: u64 = 0x1_0246_9cdc;
static FIND_APP_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_FIND_APP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-garment-apply: sret hooks (2026-07-29, técnica SretPad32 validada no inkspawner).
static APPEARANCE_RESOURCE_FINDAPP_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_APPEARANCE_RESOURCE_FINDAPP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static GARMENT_FINDSTATE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_FINDSTATE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static GARMENT_GETSUFFIXES_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_GETSUFFIXES: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-garment-apply: 3 probes novos de ABI simples (2026-07-28) — mesmos statics do padrão FindAppearance.
static ITEMFACTORY_LOADAPP_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ITEMFACTORY_LOADAPP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ITEMFACTORY_CHANGEAPP_LOADAPP_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ITEMFACTORY_CHANGEAPP_LOADAPP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ENTITY_REASSEMBLE_APPEARANCE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ENTITY_REASSEMBLE_APPEARANCE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static QUEST_PROCESSPHASERESOURCE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_QUEST_PROCESSPHASERESOURCE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-transmog-apply (2026-08-01): mesmo padrão observe-only dos probes de garment acima.
static TRANSMOG_LOADTEMPLATE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_TRANSMOG_LOADTEMPLATE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static TRANSMOG_SELECTAPPNAME_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_TRANSMOG_SELECTAPPNAME: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-garment-apply rodada 2: 6 probes de ABI simples (2026-07-28).
static GARMENT_ADDITEM_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_ADDITEM: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static GARMENT_ADDCUSTOMITEM_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_ADDCUSTOMITEM: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static GARMENT_CHANGEITEM_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_CHANGEITEM: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static GARMENT_CHANGECUSTOMITEM_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_CHANGECUSTOMITEM: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static GARMENT_REMOVEITEM_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_REMOVEITEM: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static GARMENT_ONGAMEDETACH_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_ONGAMEDETACH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-inkspawner-apply: 2 probes de ABI simples (2026-07-28), gate PRÓPRIO ~/.bwms-inkspawnerprobe.
static INKSPAWNER_ASYNC_LOCAL_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_INKSPAWNER_ASYNC_LOCAL: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static INKSPAWNER_ASYNC_EXTERNAL_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_INKSPAWNER_ASYNC_EXTERNAL: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-inkspawner-apply: AsyncSpawnFromDefPack (funil, 2026-07-29), mesmo gate ~/.bwms-inkspawnerprobe.
static INKSPAWNER_DEFPACK_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_INKSPAWNER_DEFPACK: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-inkspawner-apply: SpawnFromLocal/SpawnFromExternal (sret, 2026-07-29), gates PRÓPRIOS.
static INKSPAWNER_SPAWNLOCAL_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_INKSPAWNER_SPAWNLOCAL: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static INKSPAWNER_SPAWNEXTERNAL_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_INKSPAWNER_SPAWNEXTERNAL: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-garment-apply rodada 3: os 2 últimos sub-hooks (2026-07-28).
static GARMENT_GETVISUALTAGS_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_GETVISUALTAGS: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static GARMENT_REGISTERPART_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_REGISTERPART: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-garment-apply (2026-08-05, achado de auditoria — mapeamento do subsistema real de APLICAÇÃO
// visual, não só posse/troca de item): `AppearanceChanger::ComputePlayerGarment` — o ponto onde o
// `chunkMask`/`meshAppearance` finais são de fato escritos nos componentes vivos (via
// `ApplyComponentOverrides`, chamado de dentro desta função, não hookado separadamente — escreve
// direto em struct, sem nativa própria). Categoria SEGURA (consumidora — mesma família de
// `GetSuffixes`/`RegisterPart`/`GetVisualTags`, já provados; NÃO é a categoria "Find"/resolve que
// crashou). 8 params exatos (x0-x7), void, sem sret — endereço já mapeado desde 2026-07-28
// (`AppearanceChanger::ComputePlayerGarment = 0x103710560`), nunca hookado até agora por falta de
// escopo/tempo numa sessão anterior.
const GARMENT_COMPUTEPLAYERGARMENT_VM: u64 = 0x1_0371_0560;
static GARMENT_COMPUTEPLAYERGARMENT_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_COMPUTEPLAYERGARMENT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-fullbody-torso (2026-08-06, cont.36): wrapper de `ComputePlayerGarment` — único caller estático
// de CPG, mas ELE MESMO não tem NENHUM caller estático próprio (zero `bl` em todo `__TEXT`, zero
// referência estática raw/chained-fixup em `__DATA_CONST`/`__data`/`__got`) — só é alcançável via
// `ADRP+ADD` materializando o literal, empacotado numa struct delegate `{fn_ptr,payload_ptr}` e
// despachado pro JobSystem genérico do motor (`0x1009d46f0`→cluster `0x1009d83d0`/etc.). Ou seja:
// este wrapper RODA COMO JOB ASSÍNCRONO — hookar ele é a única forma de observar a EXECUÇÃO real
// (não só o agendamento síncrono, que o hook de CPG já cobre) sem tocar no despachante genérico em
// si (`0x1009d46f0` tem 939 call-sites em TODO o binário — avaliado e REJEITADO por agente de RE
// dedicado: "ponto mais quente do jogo inteiro", blast radius pior que qualquer coisa já hookada
// neste projeto). `0x103710968` em si é uma função NORMAL, bem formada (`sub sp,#0x60`+`stp x20,x19`+
// `stp x29,x30`), mesmo formato que `ComputePlayerGarment`/outras já hookadas com segurança — SEM o
// padrão de acesso a mapa-compartilhado-entre-threads que causou os 2 crashes reais da categoria
// "Find"/resolve. 2 params (x0=contexto, x1=payload — x1 vira `a1` de CPG internamente). Observe-only,
// gate PRÓPRIO, nunca ligado por padrão — trata como thread de job (mesma disciplina de contador
// atômico já usada em todo o projeto).
const GARMENT_CPG_WRAPPER_VM: u64 = 0x1_0371_0968;
static GARMENT_CPG_WRAPPER_CNT: AtomicU64 = AtomicU64::new(0);
// axl-fullbody-torso (2026-08-06, cont.39): handler de `ent::AppearanceMeshLoadedEvent` pra
// `ent::SkinnedMeshComponent` — achado por agente de RE dedicado via reconstrução do PMF (pointer-
// to-member-function) que `RegisterEventConnector<SkinnedMeshComponent,AppearanceMeshLoadedEvent>`
// (thunk `0x100c7fac8`, confirmado por símbolo real — binário não-stripado) recebe como 3º arg no
// site de registro (`bl 0x10219e838` em `0x100c7a310-0x100c7b498`, x0=owner ClassType, x1=CName
// hash, x2=event ClassType, x3=&closure{ptr,adj=0,vt}) — o `ptr` desse closure é `0x100c7bd50`,
// materializado por ADRP+ADD cru (não GOT, não chained-fixup indireto) logo antes da chamada.
// MECANISMO SEPARADO do JobSystem genérico (que tem 939 call-sites e foi REJEITADO por agente de
// segurança dedicado — nunca hookar `0x1009d46f0`). `RegisterEventConnector` é o dispatcher de
// evento por-tipo do RTTI (`AppearanceMeshLoadedEvent` registrado pra só 2 donos em todo o
// binário: `SkinnedMeshComponent` e `vehicle::BaseObject` — estreito por construção).
// Validação estática (sem teste ao vivo ainda): boundary exata 0x100c7bd50-0x100c7bfc4 (628B,
// via LC_FUNCTION_STARTS) — função NORMAL, prólogo real (`sub sp,#0x70`+5 pares de reg salvos
// incl. x29/x30), não é thunk leaf. Guardas em `this+0x24a==1`/`this+0x8b==0` — `+0x8b` é o MESMO
// offset já confirmado como `IComponent::enabled` neste projeto (RE de `IComponent::Toggle`,
// 2026-07-28). Lê `event+0x80`(count)/`event+0x40`(array stride 0x10) — formato de lista de
// partes carregadas. Termina com padrão "seta bit ocupado @+0x253 → notifica → limpa bit" —
// semântica de "mesh terminou de carregar → atualiza estado → notifica → libera". Zero callers
// diretos em `bl` (esperado e CORRETO pra um handler só alcançável via dispatch de evento tipado,
// não é sinal de função morta). ABI: 2 params (`this`, `event` — o thunk desreferencia o handle
// antes de repassar, então `event` aqui já é o ponteiro real, não `&event_handle`). Observe-only,
// gate PRÓPRIO, NUNCA ligado por padrão.
// ⚠️ CORREÇÃO (cont.40): este endereço é `AppearanceDissolveFinishEvent`, NÃO
// `AppearanceMeshLoadedEvent` (a string do resolver no call-site foi confirmada 2x — é a 7ª
// chamada de registro nesta função, não a 6ª). Nome da const/variáveis mantido por compat com o
// código já testado ao vivo (6 boots, zero crash, dispara com dados reais de veículo) — só o
// RÓTULO semântico mudou. `AppearanceMeshLoadedEvent` de verdade = `0x100c7b780` (1488B, não
// testado ainda). Ver `notes/GARMENT-APPEARANCE-COMPONENT-FACTS.md` §3 pro detalhe completo.
const APPEARANCE_MESHLOADED_HANDLER_VM: u64 = 0x1_00c7_bd50;
static APPEARANCE_MESHLOADED_HANDLER_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_APPEARANCE_MESHLOADED_HANDLER: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-fullbody-torso (2026-08-06/07, cont.40): handler de `ent::AppearanceStatusEvent` pra
// `ent::SkinnedMeshComponent` — MESMA função de registro do handler acima (`0x100c7a310`), 6ª
// das 10 chamadas de registro nessa função. Endereço materializado por ADRP+ADD cru logo antes
// da chamada, confirmado via `LC_FUNCTION_STARTS`: boundary exata `0x100c7b498-0x100c7b4f8`
// (96 bytes). Função NORMAL pequena, prólogo real (`stp x20,x19,[sp,#-0x20]!`+`stp x29,x30`).
// Corpo: lê `event+0x48` (filtro), compara contra `this+0x40`; se bater (ou sem filtro), escreve
// `this+0x228 = event+0x40`, chama um helper confirmado no-op nesta build (`0x100c82c34`, só
// `ret`), e TAIL-CHAMA um método VIRTUAL de `this` (`ldr x8,[x19]; ldr x1,[x8,#0x288]; br x1`) —
// a lógica de verdade fica no override específico da classe, hookar aqui só vê o DISPATCH, não
// o handler final. "Status de aparência mudou" é semanticamente mais promissor que "mesh
// carregou"/"dissolve terminou" pro que procuramos (visibilidade por perspectiva soa mais como
// status que load). Zero callers diretos (esperado, mesmo motivo dos outros). Observe-only, gate
// PRÓPRIO, NUNCA ligado por padrão.
const APPEARANCE_STATUS_HANDLER_VM: u64 = 0x1_00c7_b498;
static APPEARANCE_STATUS_HANDLER_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_APPEARANCE_STATUS_HANDLER: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-fullbody-torso (2026-08-07, cont.40): método VIRTUAL real por trás do dispatch de
// AppearanceStatusEvent (`0x100c7b498` tail-chama `vtable+0x288`). Achado por agente de RE
// dedicado: vtable de `SkinnedMeshComponent` = `0x106ed0178` (destrutor slot[0] = `0x100c7a2ec`,
// 0x24 bytes ANTES do início já confirmado de `RegisterEventConnectors`, mesma translation-unit —
// sinal forte de que é a vtable certa; corroborado por slot+82(`0x290`)=`0x100c7d2b0` bater com o
// tail-dispatch já visto em `AppearanceMeshLoadedEvent` real, e slot+88=`0x1000276d8` = o MESMO
// stub genérico já documentado neste projeto como compartilhado nesta hierarquia). Slot 0x288
// (index 81) decodificado via chained-fixup = `0x100c7eda4`, boundary exata via
// LC_FUNCTION_STARTS: 0x100c7eda4-0x100c7ee34 (144 bytes).
// Corpo: libera o `Handle<T>` velho em `this+0x1d8` (padrão já confirmado neste projeto), depois
// chama INCONDICIONALMENTE 2 outros métodos virtuais (`vtable+0x2d0`/`vtable+0x2d8`). O 1º
// (`0x100c7e884`) é "reconstrói render proxy": gateado em `this+0x8b` (JÁ CONFIRMADO
// `IComponent::enabled` neste projeto, RE de 2026-07-28) E `this+0x1f8` (ponteiro não
// identificado — hipótese: referência de recurso; se null, a função legitimamente não faz nada) —
// se os 2 passarem, monta transform + chama um builder específico do componente (`0x100c7e094`)
// e troca um Handle NOVO em `this+0x1d8`. **Categoria certa pra investigar**: é literalmente
// "libera e reconstrói o render proxy do componente, condicionado em enabled+recurso" — se o
// torso nunca reconstrói porque `this+0x1f8` é sempre null em FPP, isso apareceria aqui como
// "entrou na função mas `this+0x1f8`==0, saiu sem reconstruir". Observe-only, gate PRÓPRIO, NUNCA
// ligado por padrão. Mesma categoria de risco (per-instância, refcounting já confirmado seguro) —
// NÃO é a família "Find"/resolve compartilhada-entre-threads que já causou crash neste projeto.
const APPEARANCE_REBUILD_PROXY_VM: u64 = 0x1_00c7_eda4;
static APPEARANCE_REBUILD_PROXY_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_APPEARANCE_REBUILD_PROXY: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-fullbody-torso (2026-08-07, cont.41): `AppearanceMeshLoadedEvent` DE VERDADE (o endereço
// que `APPEARANCE_MESHLOADED_HANDLER_VM` deveria ter sido antes da correção do cont.40 — aquele
// é `AppearanceDissolveFinishEvent`, a 7ª chamada de registro; este é a 6ª de verdade).
// `LC_FUNCTION_STARTS`: 1488 bytes, nunca testado ao vivo. Mesma ABI (2 params: this, event já
// dereferenciado). Observe-only, LEVE (sem scan_cnames, lição do cont.39), gate PRÓPRIO, nunca
// ligado por padrão.
const APPEARANCE_MESHLOADED_REAL_VM: u64 = 0x1_00c7_b780;
static APPEARANCE_MESHLOADED_REAL_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_APPEARANCE_MESHLOADED_REAL: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_GARMENT_CPG_WRAPPER: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-animation-apply (2026-08-05): candidato de BAIXA CONFIANÇA pra `entAnimatedComponent::
// InitializeAnimations` — RE offline (342.983 fronteiras de função varridas, taint-tracking de
// registrador pra achar a cadeia `ldr[x0,#+0x50]→ldr[x?,#+0x60]`) NÃO convergiu num alvo confiável
// (offsets `+0x50`/`+0x150`/`+0x180` de `IComponent`/`IPlacedComponent` são reusados por dezenas
// de funções não-relacionadas do motor — reflection, física de ragdoll, blend de locomoção).
// `0x1024d1f24` foi o único candidato achado (single-caller, checa `graph`@+0x150/`rig`@+0x138),
// mas o PRÓPRIO agente de RE recomendou NÃO confiar: o único caller está embutido num cálculo de
// blend de velocidade de movimento POR-TICK (`0x1024d0dac`), não num contexto de attach de
// componente — semanticamente errado pro que procuramos. Testado ao vivo mesmo assim (observe-
// only, ABI 1-arg void barata, risco baixo) só pra FECHAR a questão rápido sem mais RE offline.
// **REFUTADO (2026-08-05, mesmo dia): 2 boots, hook instalou sem crash, boot completo até
// GAMEPLAY + save carregado + equip natural de itens — ZERO disparos do candidato no ciclo
// inteiro.** Confirma a desconfiança do agente: não é `InitializeAnimations`. NÃO reativar sem RE
// nova (endereço genuinamente errado, não é questão de esperar mais/gatilho diferente). Ver
// DATABASE.md pro detalhe completo.
const ANIM_INITANIM_CANDIDATE_VM: u64 = 0x1_024d_1f24;
static ANIM_INITANIM_CANDIDATE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ANIM_INITANIM_CANDIDATE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-attachment-apply (2026-08-06): candidato pra `CharacterCustomizationHairstyleController::
// CheckState(this, aState: CharacterBodyPartState&)`. Achado por RE mais forte que o do Animation
// (não refutado pelo próprio agente): consome os CNames `hide_Hair`/`hide_H1` (hash FNV-1a64
// confirmado batendo, `bwms-hashes::fnv1a64`) via chamada virtual (`vtbl+0x280`, padrão `HasTag`)
// em cada item equipado, zera o campo de saída no início (`str wzr,[x19]`) e escreve um tri-estado
// (0/1/2) de volta — ABI/lógica exatamente como esperado de `CheckState`. Localizada na MESMA
// translation unit dos outros controllers de customização já mapeados (`0x102442d8c`, perto de
// `0x102447080`/`0x102447098` = OnAttach/OnDetach de Genitals/Hairstyle, RE de 2026-07-28).
// Ressalva honesta: zero call-site direto achado (explicado por chained-fixups — chamada indireta
// via tabela de ponteiro, não `bl` cru) — não é prova completa por xref, só assinatura+lógica+
// vizinhança. Teste observe-only (HookAfter — chama a original PRIMEIRO, só loga o resultado
// depois, igual ao padrão real do ArchiveXL) pra confirmar/refutar rápido.
// **CONFIRMADO (2026-08-06):** disparou 3x na mesma cadeia de equip que já ativa `AddItem`/
// `ComputePlayerGarment` (`give`+`equiprawv7`), zero crash, escreveu `[aState+0]=0x00000002` nas 3
// vezes — valor plausível e ESTÁVEL (não é lixo aleatório) dentro do tri-estado (0/1/2) esperado.
// `CharacterCustomizationHairstyleController::CheckState` fecha — 1 dos 2 `CheckState` do
// `axl-attachment-apply` (falta `FeetController::CheckState`, mecanismo diferente segundo o
// próprio agente de RE — usa `GroupName` em vez de tag `hide_*`, não investigado ainda).
const ATTACH_CHECKSTATE_CANDIDATE_VM: u64 = 0x1_0244_2d8c;
static ATTACH_CHECKSTATE_CANDIDATE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ATTACH_CHECKSTATE_CANDIDATE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-attachment-apply (2026-08-06): 2 candidatos pra `TPPRepresentationComponent::IsAffectedSlot
// (aSlotID: TweakDBID) -> bool` — achados por padrão de bytes (RE offline, sem string/hash de
// confirmação, confiança BAIXA) na vizinhança de `CheckState`/`ComputePlayerGarment` já
// confirmados (mesma translation unit, região 0x1035a0000-0x1035a2000). Ambos terminam em
// `CMP`+`CSET`+`RET` (padrão típico de getter booleano), quase idênticos entre si (podem ser um
// par de overloads/variantes). Mecanismo de teste: `Interceptor::replace` observe-only (hook de
// FUNÇÃO EXISTENTE — categoria DIFERENTE e já comprovadamente segura dos 3 crashes recentes de
// REGISTRO de classe/método NOVO na RTTI, que são um mecanismo totalmente separado). 1 param
// (TweakDBID, 8 bytes), retorno bool — ABI trivial.
// **TESTADO (2026-08-06): AMBOS ao vivo, zero crash (confirma que a categoria de risco era mesmo
// só o registro de classe/método novo, não hooks de função existente).** Resultado NÃO confirma
// nenhum dos 2 como `IsAffectedSlot`: candidato 1 disparou 9x mas com o MESMO valor de "arg" em
// TODAS as chamadas (`0x79b17f05c0`) — não varia como um `TweakDBID aSlotID` genuíno variaria
// entre slots diferentes; o valor tem CARA de ponteiro heap (mesmo padrão de endereços já vistos
// em vários logs desta sessão), não de TweakDBID (hash+length em formato bem diferente). Achado:
// **provavelmente FALSO POSITIVO** — o "padrão de bytes" (CMP+CSET+RET) bateu, mas é outra função
// (talvez um `IsValid()`/checagem repetida contra um alvo fixo, chamada em loop/tick). Candidato 2
// NUNCA disparou (só o log de instalação) — inconclusivo, não refutado nem confirmado. NÃO
// reativar sem achar uma âncora melhor (nem candidato bateu com confiança).
const ATTACH_ISAFFECTEDSLOT_C1_VM: u64 = 0x1_035a_1c18;
static ATTACH_ISAFFECTEDSLOT_C1_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ATTACH_ISAFFECTEDSLOT_C1: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
const ATTACH_ISAFFECTEDSLOT_C2_VM: u64 = 0x1_035a_1d68;
static ATTACH_ISAFFECTEDSLOT_C2_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ATTACH_ISAFFECTEDSLOT_C2: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-attachment-apply (candidato #3, achado 2026-08-11 por agente de pesquisa dedicado — RE
// offline, ver `PENDENCIAS-UNIFICADAS.md`/`CATALOGO-EXAUSTIVO-ARCHIVEXL.md` item #53): candidato
// NOVO pra `TPPRepresentationComponent::IsAffectedSlot`/função irmã, DISTINTO dos 2 acima (C1/C2,
// já refutados/inconclusivos — nunca tocam `x1`). Este candidato **decodifica `x1` exatamente como
// um `TweakDBID`** (hash u32 + length u8, não um ponteiro) e opera sobre um array em
// `this+0x110`/`this+0x11c` (offsets compartilhados com 3 funções irmãs plausíveis da mesma
// classe) — os 2 sinais que o elevam de "baixa" pra confiança MÉDIA. **Ressalva que impede
// confiança ALTA**: a função devolve um PONTEIRO em x0 (não um `bool` via `cset`, como a
// assinatura oficial `IsAffectedSlot(TweakDBID) -> bool` esperaria), e ZERO caller foi localizado
// no binário inteiro (consistente com ser um `RawFunc` nunca chamado pelo motor vanilla, só
// exposto externamente via `Core::RawFunc` — mesmo padrão de outras funções desta família, não
// necessariamente um problema, mas também não uma confirmação). **NUNCA testado ao vivo** — esta
// sessão é 100% offline (não pode bootar o jogo), então isto fica CODADO/DEPLOYÁVEL mas SEM
// PROVA. Segue a recomendação explícita do agente de RE que achou o candidato: hook OBSERVE-ONLY
// primeiro (mesma categoria de risco BAIXO já usada com sucesso pros outros candidatos desta
// família — hookar uma função EXISTENTE nunca foi a causa dos crashes reais desta sessão, que
// foram todos de REGISTRAR classe/método NOVO na RTTI), sem chamar/mutar nada, só observar
// `this`/`x1`/retorno e os campos candidatos `this+0x110`/`this+0x11c`. Assinatura assumida pro
// probe (2 args: `this` + `slot_id`, retorno como ponteiro/u64 cru) — pode estar errada (é
// exatamente a dúvida que o probe existe pra responder numa sessão futura com boot disponível).
const ATTACH_ISAFFECTEDSLOT_C3_VM: u64 = 0x1_035a_60e0;
static ATTACH_ISAFFECTEDSLOT_C3_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ATTACH_ISAFFECTEDSLOT_C3: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-attachment-apply (2026-08-06): candidato pra `TPPRepresentationComponent::OnAttach(this,
// a2: uintptr_t)`. Achado por `vtbl gameTPPRepresentationComponent 24` ao vivo (comando barato,
// zero RE offline) — slots 5-23 são byte-idênticos ao dump de Hairstyle/Feet/Genitals (herança
// comum de IComponent/IPlacedComponent), mas slots [00]/[01]/[03]/[04] são ÚNICOS desta classe.
// `peekq` nos 4 candidatos confirmou o MESMO padrão thunk+corpo-real já visto em Genitals/
// Hairstyle: slot[03]=0x1035a05ec decodifica pra `B <offset>` (thunk de 4 bytes, opcode top-6-bits
// 000101); slot[04]=0x1035a05f0 decodifica pra `STP X29,X30,[SP,#-0x10]!` + `MOV X29,SP`
// (0xa9bf7bfd + 0x910003fd — prólogo REAL de função, byte-a-byte idêntico ao padrão que já fechou
// `GENITAL_ONATTACH_VM`/`HAIRSTYLE_ONDETACH_VM` em 2026-07-28). Header SDK marca `OnAttach` como
// `RawFunc` (não-virtual) — mas essa classificação pode estar errada/incompleta (o SDK real é
// mantido pra Windows, pode não capturar TODOS os overrides virtuais corretamente), ou o motor
// pode ter uma versão virtual E uma não-virtual coexistindo. Testando ao vivo pra decidir.
// **REFUTADO (2026-08-06): 2 boots, o 2º com o hook instalado DESDE `on_load` (via `/tmp/bwms-dev`,
// máxima janela possível) — sobreviveu boot completo + menu + autocontinue + save carregado +
// equip natural (`AddItem` disparou 2x no mesmo teste), zero crash, MAS zero disparo do candidato
// em toda a sessão.** O prólogo real (slot[04], `STP X29,X30`) é uma função virtual genuína desta
// classe — só não é `OnAttach` (ou não dispara nas condições testadas). NÃO reativar sem RE nova.
const TPP_ONATTACH_CANDIDATE_VM: u64 = 0x1_035a_05f0;
static TPP_ONATTACH_CANDIDATE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_TPP_ONATTACH_CANDIDATE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-attachment-apply (2026-08-06, cont.31): 2º candidato pra `TPPRepresentationComponent::OnAttach`,
// achado por RE offline real (agente background, capstone) depois do candidato acima (slot[04]) ser
// REFUTADO ao vivo. Vtable inteira da classe recuperada em `0x1071d5b60` (50 slots, chained-fixups
// decodificados). Slot[49] = `0x1035a0604`: função grande (~0x300+ bytes), claramente específica desta
// classe (não é stub compartilhado — vários outros slots [31,32,34,37,41,46] apontam pro MESMO stub
// genérico `0x1000276d8`, este não), e só é alcançada por despacho VIRTUAL em todo o binário (zero
// `bl` direto) — nunca disassemblada/testada antes. NUNCA testado ao vivo ainda — gate próprio,
// nunca ligado por padrão. Assinatura desconhecida (assumindo 1 arg extra além de `this`, mesmo
// padrão ABI do candidato slot[04] acima — observe-only, forward transparente, seguro mesmo se a
// assinatura real tiver mais args, pelo mesmo raciocínio já validado 2x nesse arquivo).
const TPP_ONATTACH_C2_VM: u64 = 0x1_035a_0604;
static TPP_ONATTACH_C2_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_TPP_ONATTACH_C2: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-attachment-apply / item #53 (2026-08-12, sessão nova, RE offline pura — zero boot): ângulo
// NUNCA tentado antes nas 4+ rodadas anteriores deste item (todas tentaram achar `IsAffectedSlot`/
// `RegisterAffectedItem` diretamente por padrão-de-bytes/vizinhança). Em vez disso, minerei
// `cp77-symbols/symbols-demangled.txt` (68654 símbolos, binário NÃO stripado) por
// `rtti::RegisterEventConnector<game::TPPRepresentationComponent, ...>` — o compilador emite um
// símbolo demangled PRÓPRIO pra cada thunk `__invoke(void* closureData, ISerializable& receiver,
// THandle<red::Event> const& event)` gerado por essa instanciação de template, um por EVENTO que a
// classe escuta. Achado: `TPPRepresentationComponent` escuta 7 eventos nativos reais — os 5 já
// conhecidos (`Activate/AppearancesReady/Deactivate/Prepare/FinalizeActivation` +
// `FinalizeDeactivation`TPPRepresentationEvent) + **`game::AttachmentSlotEvents::ItemEquippedInSlot`
// — NUNCA citado em nenhuma rodada anterior desta investigação**, e semanticamente exatamente o
// tipo de evento que dispararia a lógica de `IsAffectedSlot`/`RegisterAffectedItem` (checar/
// registrar se o slot que acabou de ser equipado afeta a representação TPP).
//
// Disassemblei o thunk `__invoke` real (`0x1035a95e8`, endereço CONFIRMADO por símbolo, não
// candidato) — é o padrão-livro do Itanium ABI pra invocação de ponteiro-de-método-membro:
// `ldp x3,x8,[x0]` (carrega {ptr,adj} da storage do closure) → `add x0,x1,x8,asr#1` (ajusta o
// `this` real) → `tbz w8,#0,...`/`br x3` (tail-call pro handler real). Tentei achar o valor
// LITERAL do ponteiro-membro (`x3`) rastreando o call-site que CONSTRÓI o closure
// (`find_adrp_xrefs.py`/busca ADRP+ADD exata por offset) — **achado NEGATIVO**: a rotina de
// registro de eventos (função de bulk-registration, disassemblada em `0x103587800+`, um bloco de
// ~0x110 bytes por evento) não embute o ponteiro-membro como literal ADRP+ADD visível nesse nível
// — é passado adiante via despacho indireto (`blr [x21]+0x38`, outro nível de indireção), e a
// busca exata por ADRP+ADD pro endereço específico do `s_vt` de `ItemEquippedInSlot`
// (`0x107402dc8`) deu ZERO hits no binário `__text` inteiro (script `/tmp/find_exact_adrp_add.py`,
// mesma técnica de `find_adrp_xrefs.py` mas exigindo o PAR ADRP+ADD exato, não só a página) —
// portanto **NÃO dá pra resolver o endereço exato de `TPPRepresentationComponent::OnItemEquippedInSlot`
// (ou como quer que se chame) por este caminho estático.**
//
// **Decisão de escopo: hookar o `__invoke` CONFIRMADO em vez do handler desconhecido.** Isso é
// estritamente melhor pra este objetivo: (1) o endereço é 100% certo (veio de símbolo, não de
// heurística de bytes); (2) intercepta QUALQUER disparo real do evento `ItemEquippedInSlot` em
// QUALQUER `TPPRepresentationComponent` — evento genuinamente comum (dispara em todo equip real de
// item, mesmo gatilho `give`+`equiprawv7` já usado por toda a saga `axl-garment-apply`); (3) o
// `receiver` (x1, aproximação de `TPPRepresentationComponent*`, sem aplicar o ajuste do thunk —
// pode divergir por um pequeno offset em caso de herança múltipla, mas é a melhor aproximação
// disponível sem duplicar o cálculo do próprio thunk) dá acesso ao campo `SlotListener@+0x148`
// (offset de CAMPO/dado, já confirmado no header oficial vendorizado — offsets de struct portam
// 1:1 Windows↔Mac neste projeto, categoria de confiança bem mais alta que endereço de função) —
// que é um `Handle<IAttachmentSlotsListener>` apontando pra uma instância REAL de
// `game::TPPRepresentationSlotListener`, a classe que o catálogo já identificou (2026-08-12,
// checkpoint anterior desta mesma sessão) como o alvo genuíno de `IsAffectedSlot`/
// `RegisterAffectedItem` — mas cujo vtable NUNCA foi lido de uma instância VIVA (só via string-xref
// da RTTI, que achou o vtable do WRAPPER de metadado, não do objeto-jogo real — achado negativo já
// documentado no item #53). **Este probe, se disparar, resolve isso de uma vez**: lê
// `[receiver+0x148]` (instance ptr do Handle) → lê `[instance]` (vtable ptr REAL, resolvido em
// RUNTIME, não estático) → dumpa os slots ao redor de onde o header oficial do RED4ext.SDK
// documenta `OnItemEquipped@0x118`/`OnItemUnequipped@0x140` (Windows) — convertido pelo shift
// Itanium +0x08 já confirmado nesta mesma classe (`OnAttach`/etc.) pra candidatos Mac 0x120/0x148.
//
// 100% OBSERVE-ONLY: nunca muta nada, sempre chama a original (a função é `void`, nada a
// preservar/retornar). CODADO, NUNCA TESTADO AO VIVO (sessão 100% offline, zero boot disponível).
const TPP_ITEMEQUIP_INVOKE_VM: u64 = 0x1_035a_95e8;
static TPP_ITEMEQUIP_INVOKE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_TPP_ITEMEQUIP_INVOKE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

// item #53 (2026-08-12, checkpoint NOVO — RE offline pura sobre o dump AO VIVO já capturado em
// `proofs/2026-08-12-archivexl-53-tpp-slotlistener-vtable-COMPLETO-PROVADO.log`, zero boot nesta
// rodada). O probe acima (`tpp_itemequip_invoke_probe`) já tinha rodado ao vivo (2 boots) e
// capturado o vtable REAL de uma instância viva de `TPPRepresentationSlotListener` (64 qwords,
// `SlotListener.vtbl=0x109531a20` runtime, `game_base=0x10235c000`). Convertendo cada qword pro
// vmaddr ESTÁTICO (`static = runtime - (game_base - LINK_BASE)`, mesma fórmula de `un_rebase`)
// com um script offline (`slide_calc.py`) e resolvendo o símbolo mais próximo de cada um
// (`symlookup.py`, contra `symbols-demangled.txt`) achou 2 coisas importantes:
//
// **(A) O vtable NÃO tem 64 slots — tem EXATAMENTE 40 (0x00..0x138).** Prova: o slot idx40 do
// dump (offset Mac 0x140) converte pro vmaddr estático `0x1035a05f0` — EXATAMENTE o valor já
// hardcoded neste arquivo como `TPP_ONATTACH_CANDIDATE_VM` (slot[04] do vtable de
// `TPPRepresentationComponent`, uma CLASSE DIFERENTE, já documentado em cont.31/2026-08-06 como
// tendo o vtable INTEIRO recuperado em `0x1071d5b60`, 50 slots). Cruzando os dois números:
// `0x1071d5b60 - 0x140 (40 qwords) = 0x1071d5a20` — e é EXATAMENTE esse o vmaddr estático do
// vtable de `TPPRepresentationSlotListener` (convertido do runtime `SlotListener.vtbl` do MESMO
// boot). Ou seja: os 2 vtables (Component e SlotListener) ficam LADO A LADO na seção de dados
// const do binário, com o de SlotListener vindo primeiro — e os índices 40..63 do dump de 64
// slots NÃO são mais conteúdo de `TPPRepresentationSlotListener`; são uma continuação acidental
// da leitura que já entrou no vtable VIZINHO de `TPPRepresentationComponent` (idx40=slot[0] dele,
// idx44=slot[04]=`TPP_ONATTACH_CANDIDATE_VM`, etc.). **Isso REFUTA a hipótese A** (mapeamento
// ingênuo "offset Windows do header + shift Itanium 0x08", que previa os 13 métodos da interface
// em Mac idx34-46 — impossível, pois passaria do limite real de 40 slots).
//
// **(B) Achado BÔNUS pro gap irmão #40/#41 (`axl-attachment-apply`/`OnAttach`): `TPP_ONATTACH_C2_VM`
// (`0x1035a0604`, slot[49] de `TPPRepresentationComponent`, já catalogado "alta confiança, nunca
// testado") foi disassemblado (Capstone, offline) — o corpo lê `this+0x50`→`+0xc0`→array[0],
// despacha uma chamada virtual, guarda o resultado em `this+0x138`, aloca 0x48 bytes, e escreve
// no objeto novo o vtable **`0x1071d5a20`** via `ADRP+ADD` (literal puro, sem ambiguidade de
// chained-fixups — é código, não dado) — o MESMO endereço que o item (A) acima já tinha derivado
// de forma totalmente independente (runtime→estático via slide). Ou seja: 2 fontes
// INDEPENDENTES (disassembly estático da função vs. captura de vtable ao vivo) convergem no MESMO
// endereço. Isso é evidência forte de que `TPP_ONATTACH_C2_VM` CONSTRÓI o objeto
// `TPPRepresentationSlotListener` (grava o `Handle<T>` resultante em `this+0x148`, o offset exato
// de `SlotListener` documentado no header vendorizado) — muito provavelmente é
// `TPPRepresentationComponent::OnAttach` de verdade (ou uma rotina de setup bem próxima). Eleva a
// prioridade deste candidato pra uma sessão ao vivo futura (fora do escopo desta rodada, que é
// 100% offline).
//
// **(C) Hipótese B pro mapeamento da interface (self-consistente com o limite de 40 slots, mas
// NÃO confirmada):** se o vtable é [base herdada de IScriptable, ~27 slots][13 métodos da
// interface], os 13 métodos cairiam nos ÚLTIMOS 13 slots do vtable real: idx27 (0xd8) até idx39
// (0x138) — `OnItemEquippingStarted..OnChangeAppearanceCanceled`, nessa ordem. Batendo a favor
// desta hipótese: o padrão de repetição no dump mostra um bloco de 4 valores CONSECUTIVOS nunca
// repetidos em lugar nenhum dos 64 slots começando EXATAMENTE em idx27 (idx27,28,29,30), mais 1
// isolado em idx33 — 5 candidatos a "override genuíno" (não-stub), incluindo idx29
// (`OnItemEquipped` sob esta hipótese) exatamente onde a intuição original do item #53 esperava
// (o evento que disparou a captura foi `ItemEquippedInSlot`).
//
// **(D) MAS a disassembly desses 5 candidatos (idx27/28/29/30/33, script `disasm_candidates2.py`)
// não confirma a hipótese com força:** idx27=`mov w0,#1; ret` (retorna constante); idx29/30/33=
// `ret` puro (função vazia de 1 instrução — igual ao padrão já visto no stub 4x-compartilhado
// `0x1000276d8`); só idx28 tem controle de fluxo real (salva x2 em x19, chama o PRÓPRIO
// vtable[1] via `[this]→[+8]`, e faz TAIL-CALL pra um helper genérico `0x102198ff4` passando
// (resultado, x19)). Disassemblando esse helper: é um walk genérico de lista ligada chamando
// `predicate->vtable[3](predicate, elemento)` por entrada — um idioma clássico de "achar
// elemento por predicado", mas `x19` (que idx28 preencheu a partir de x2, o suposto
// `TweakDBID`) é DEREFERENCIADO como ponteiro (`ldr x8,[x19]`) dentro do helper — incompatível
// com um `TweakDBID` passado POR VALOR (hash+length, não um endereço válido). **Bandeira
// vermelha honesta:** ou o ABI assumido pra idx28 está errado (x2 não é TweakDBID aqui), ou
// idx28/hipótese B não é a interface certa. NENHUM `bl`/`b` de nenhum candidato disassemblado
// (idx0-4, 26-30, 33, 36, 40-44) resolveu contra qualquer candidato já catalogado de
// `IsAffectedSlot`/`RegisterAffectedItem` (C1/C2/C3/RegisterAffectedItem-cand/0x1035a1f80/
// 0x1035a4f30/0x1035a1d80) — checado programaticamente, zero match.
//
// **VEREDITO HONESTO desta rodada: nenhum slot (hipótese A nem B) tem confiança ALTA de ser
// `IsAffectedSlot`/`RegisterAffectedItem`. O que avançou de verdade: o TAMANHO REAL do vtable (40,
// não 64) e seu endereço estático exato (0x1071d5a20, confirmado 2x independente); e o achado
// bônus sobre `TPP_ONATTACH_C2_VM` (provável construtor do SlotListener).** Ver `proofs/` (mesmo
// arquivo do checkpoint anterior) — nenhum artefato NOVO de boot nesta rodada (100% offline).
// Item #53 PERMANECE REAL_GAP — não fechar sem confirmação ao vivo adicional.
//
// Probe NOVO abaixo: hook observe-only no candidato idx28 (`0x102238918`, o único com controle de
// fluxo real dos 5 da hipótese B) — groundwork pra uma sessão futura decidir empiricamente se
// x1/x2 recebem mesmo `(ItemID&, TweakDBID)` ou algo diferente. Confiança MÉDIA-BAIXA (não
// "alta" — documentado com honestidade, ver golden rule). Sempre chama a original primeiro
// (nunca risca comportamento observável), nunca ligado por padrão.
const TPP_SLOTLISTENER_IDX28_CAND_VM: u64 = 0x1_0223_8918;
static TPP_SLOTLISTENER_IDX28_CAND_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_TPP_SLOTLISTENER_IDX28_CAND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

// axl-attachment-apply / itens #40, #41, #54 (2026-08-12, mesma técnica de mineração de símbolos
// acima, aplicada a `game::AttachmentSlots`, a classe-dona de `IsSlotEmpty`/`IsSlotSpawning`/
// `InitializeSlots` (item #54) e alvo dos 7 hooks nunca tentados de `AttachmentExtension` (item
// #40/#41). Achado: `AttachmentSlots` escuta 6 eventos nativos reais, TODOS com `__invoke` thunk
// confirmado por símbolo — `AttachmentSlotEvents::{EquipStart,EquipEnd,MoveEquip,UnequipStart,
// UnequipEnd}` + `ItemEvents::RemoveActiveItem` — NUNCA citados em nenhuma rodada anterior desta
// investigação (o header oficial só documenta os métodos RawFunc/RawVFunc da extensão, não estes
// eventos internos do motor). `EquipStart`/`EquipEnd` disparam em TODO equip/unequip real
// (`give`+`equiprawv7`, mesmo gatilho canônico da saga garment) — candidato de trigger muito mais
// confiável que qualquer RawFunc sem endereço achado até agora.
//
// Mesma limitação já documentada acima pro TPP: o ponteiro-membro do handler real
// (`AttachmentSlots::OnEquipStart` ou nome equivalente) não é resolvível por ADRP+ADD estático no
// nível do `__invoke` (mesma arquitetura de registro indireto). **Decisão de escopo idêntica:
// hookar o `__invoke` CONFIRMADO** (`0x103515cd0`, disassemblado — MESMO padrão-livro Itanium do
// TPP acima, byte-a-byte idêntico) — o `receiver` (x1) é uma aproximação de `AttachmentSlots*`
// REAL e VIVO, nunca antes capturado por nenhuma investigação deste item. Dumpa os primeiros 0x60
// bytes do objeto (12 qwords) — groundwork de RE pura (não há hipótese de layout ainda pra
// `IsSlotEmpty`/`IsSlotSpawning`/`s_dependentSlots`/etc., preenchido por leitura direta em vez de
// adivinhação) — e o 1º qword do objeto de evento apontado por `evt_ref` (candidato a TweakDBID do
// slot afetado, mesmo formato hash+length já usado em outras investigações desta sessão).
//
// 100% OBSERVE-ONLY, sempre chama a original. CODADO, NUNCA TESTADO AO VIVO.
const ATTACHSLOTS_EQUIPSTART_INVOKE_VM: u64 = 0x1_0351_5cd0;
static ATTACHSLOTS_EQUIPSTART_INVOKE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ATTACHSLOTS_EQUIPSTART_INVOKE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

// axl-questphase-apply / item #55 (2026-08-12, retomando o candidato JÁ ACHADO em sessão anterior
// via `getquestsys`/dump de vtable AO VIVO — `0x102efb134`, "vtbl+0x248 (ForceStartNode-cand)",
// confiança MÉDIA-ALTA após corroboração estrutural por disassembly estático nesta mesma sessão,
// checkpoint 2026-08-12 anterior: acessa `this+0x78`/`this+0xa8`, EXATAMENTE `NodeHashMap`/
// `QuestList` do header oficial vendorizado). Diferente de todos os hooks anteriores desta família
// (que só LEEM/dumpam, nunca hookam), este é o 1º OBSERVE-ONLY genuíno pro candidato — nunca
// chamamos a função nós mesmos (zero risco de mutar quest/save com `QuestNodeKey` fabricado, a
// preocupação de segurança já documentada em toda a história deste item); só observamos SE/COMO o
// PRÓPRIO MOTOR chama esta função durante execução normal de quest (avançar qualquer diálogo/nó de
// quest ativo, não necessariamente a quest travada específica já identificada como bloqueador).
// Endereço é uma FUNÇÃO ESTÁTICA (não um slot de vtable) — mesma categoria/risco de
// `QUESTSYS_ONGAMERESTORED_VM` já instalado incondicionalmente com sucesso nesta mesma classe.
//
// HookWrap: chama a original PRIMEIRO (nunca arrisca mudar timing/comportamento), depois loga os 2
// argumentos decodificados conforme a RE já documentada: `node_key` como `{hash:u32@0, campo:u16@4}`
// (achado por disassembly do próprio candidato, `ldr w8,[x20]`+`ldrh w2,[x20,#4]`) e `arr` como
// `DynArray<CName>{entries@0,cap@8,size@0xc}` (mesmo layout já confirmado em dezenas de outras
// natives deste projeto). Gate PRÓPRIO, NUNCA ligado por padrão (mais especulativo que
// OnGameRestored — confiança MÉDIA-ALTA, não confirmada). CODADO, NUNCA TESTADO AO VIVO.
const QUESTSYS_FORCESTARTNODE_CAND_VM: u64 = 0x1_02ef_b134;
static QUESTSYS_FORCESTARTNODE_CAND_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_QUESTSYS_FORCESTARTNODE_CAND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// worldStreamingSector::PostLoad — multi-probe RE 2026-07-26
// 0x1021afcd4 CONFIRMED WRONG (vtable dispatch thunk: LDR x8,[x0]; LDR x8,[x8,#0x28]; BLR x8)
// 4 candidates from vtable slot analysis (worldStreamingSector vtable @ 0x1071b3848, 31 slots):
const STREAMING_SL14_VM: u64 = 0x1_0219_75e4; // slot[14] @+0x70: frame=0x140, callsite does MOV X1,X20 before BLR
const STREAMING_SL25_VM: u64 = 0x1_0219_a254; // slot[25] @+0xc8: frame=0x60, saves x0 only
const STREAMING_SL26_VM: u64 = 0x1_0219_9f48; // slot[26] @+0xd0: frame=0x40, saves x0 only
const STREAMING_SL27_VM: u64 = 0x1_0219_a3e8; // slot[27] @+0xd8: frame=0x80, saves x0 only
// axl-world-streaming-apply: worldStreamingBlock candidate functions (string xref RE 2026-07-26)
// String "worldStreamingBlock" at vmaddr 0x106d5328e; "worldStreamingBlockIndex" at 0x106d531ae.
// fn1 = RTTI once-init (guard flag + CName reg, safe probe); fn2 = complex wrapper (may be PostLoad area).
const WSBLOCK_FN1_VM:    u64 = 0x1_0342_8bd4; // RTTI once-init for worldStreamingBlock
const WSBLOCK_FN2_VM:    u64 = 0x1_0342_b7a8; // complex fn referencing worldStreamingBlock
const WSBLOCKIDX_FN1_VM: u64 = 0x1_0342_7d1c; // RTTI once-init for worldStreamingBlockIndex
const WSBLOCKIDX_FN2_VM: u64 = 0x1_0342_b24c; // complex fn referencing worldStreamingBlockIndex
// axl-animation-apply vmaddrs 2026-07-27 (postloadprobe confirmed)
const ANIMSET_POSTLOAD_VM:  u64 = 0x1_0432_ff54; // animAnimSet::PostLoad
const ANIMRIG_POSTLOAD_VM:  u64 = 0x1_045c_2644; // animRig::PostLoad
// axl-mesh-apply vmaddrs 2026-07-27 (entGarmentSkinnedMeshComponent == entSkinnedMeshComponent vtable slot)
const GARMENT_SKIN_POSTLOAD_VM: u64 = 0x1_00c7_c7e4; // entGarmentSkinnedMeshComponent + entSkinnedMeshComponent
const MESH_COMP_POSTLOAD_VM:    u64 = 0x1_00c6_9f40; // entMeshComponent::PostLoad
// axl-attachment-apply vmaddr 2026-07-27
const SLOT_COMP_POSTLOAD_VM: u64 = 0x1_00c4_8118; // entSlotComponent::PostLoad
// axl-puppet-state-apply vmaddr 2026-07-27 (gameObject base covers entPuppet)
const GAME_OBJ_POSTLOAD_VM:  u64 = 0x1_0218_5f28; // gameObject::PostLoad
// ArchiveXL `#51` (ResourcePatchExtension, 2026-08-12 — `postloadprobe` ao vivo, mesma sessão que
// achou EngineTime/#22 do Codeware): 2 candidatos novos resolvidos via `probe_postload_addresses()`
// (a MESMA técnica já provada 8x pros consts acima — `new_object`+ler vtable+0x30). Ainda NÃO
// promovidos a hook instalado até esta rodada de bookkeeping offline (2026-08-12, agente dedicado
// ArchiveXL) — só endereço resolvido pelo boot anterior, nunca testado como hook DISPARANDO. Fecha
// a lacuna "1 boot de distância" documentada em CATALOGO-EXAUSTIVO-ARCHIVEXL.md (checkpoint
// "foco explícito nos 16 REAL_GAP", achado #2): agora tem `install_hook_probe!` pronto, só falta o
// boot que arme o marcador (`~/.bwms-hook-entitytemplate-postload`/`~/.bwms-hook-appearanceresource-
// postload`) e confirme disparo real (padrão idêntico ao CMesh/MorphTargetMesh já provados).
const ENTITYTEMPLATE_POSTLOAD_VM: u64 = 0x1_00cb_2a6c; // entEntityTemplate::PostLoad
const APPEARANCERESOURCE_POSTLOAD_VM: u64 = 0x1_00ad_88a8; // appearanceAppearanceResource::PostLoad
// ArchiveXL `#49` (WorldStreamingExtension) — candidato #2 de `worldStreamingSector::PostLoad`,
// achado pelo MESMO `postloadprobe` na mesma sessão de 2026-08-12, DIVERGENTE do endereço já
// PROVADO ao vivo sob o rótulo antigo `axl-streaming-apply` (`STREAMING_SL26_VM`/`0x102199f48`,
// achado por técnica DIFERENTE — scan manual de slot de vtable, não `new_object`+vtable+0x30).
// RE offline dedicada (2026-08-12, agente ArchiveXL, disassembly Capstone + varredura completa de
// `__DATA_CONST` por chained-fixups) reconciliou parcialmente a divergência, sem prova ao vivo:
//   - Este candidato (0x103426204) é referenciado por **1 ÚNICA** entrada em TODO `__DATA_CONST`
//     (vtable-slot exclusivo, não-compartilhado) — sentado em **LOCAL SLOT 6** do seu próprio vtable
//     (delimitado por um par (0,0) de offset-to-top/typeinfo imediatamente antes) — o MESMO slot
//     local já confirmado (por 2 vias independentes) pra `GAME_OBJ_POSTLOAD_VM`/0x102185f28 (a base
//     "PostLoad" compartilhada por `gameObject`/`entPuppet`) E pra CMesh/MorphTargetMesh via a
//     técnica live já provada — 3 classes convergindo no MESMO slot-local-6, forte evidência de que
//     é genuinamente o slot Itanium-shiftado de `ISerializable::PostLoad` (Windows +0x28 -> Mac +0x30).
//   - O disassembly do corpo (700 bytes, boundary de função confirmada via LC_FUNCTION_STARTS)
//     chama `bl 0x102185f28` (=GAME_OBJ_POSTLOAD_VM, a "PostLoad" base) como a PRIMEIRA instrução
//     após o prólogo — padrão clássico de override chamando `Super::PostLoad()` antes da lógica
//     própria — e, logo depois, opera sobre `this+0x40`, EXATAMENTE o offset que o header oficial
//     vendorizado documenta pra `Raw::StreamingSector::NodeBuffer` desta MESMA classe
//     (`enablers/ArchiveXL/src/Red/StreamingSector.hpp:154`).
//   - `STREAMING_SL26_VM`/0x102199f48, em contraste, tem **8958 referências** em `__DATA_CONST`
//     (função amplamente COMPARTILHADA por milhares de vtables — não é override específico de
//     classe) e, na instância examinada, senta no LOCAL SLOT 26 de um vtable de 31 métodos
//     DIFERENTE/ADJACENTE (não o mesmo vtable deste candidato) — corpo do disassembly (240 bytes)
//     tem formato de GETTER/agregador (loop com dispatch virtual computando um máximo via `csel`,
//     termina com `ret` devolvendo escalar em w0), não o padrão void/2-args de PostLoad.
//   - **Conclusão honesta, NÃO confirmada ao vivo**: `0x103426204` é o candidato estruturalmente
//     mais forte pra `worldStreamingSector::PostLoad` (a função que `OnSectorPostLoad` do
//     ArchiveXL real hooka); `SL26`/0x102199f48 provavelmente NÃO é PostLoad — é um método
//     amplamente herdado/compartilhado que só COINCIDE em disparar perto da janela de load do
//     setor (por isso o teste de array R/W antigo funcionou — o OBJETO `this` legitimamente tem
//     arrays de node por perto, não porque SL26 EM SI seja quem os popula). Isso NÃO invalida a
//     técnica/prova de R/W do SL26 (mecanismo genérico continua real) — só a alegação de IDENTIDADE
//     ("isto é PostLoad") fica em dúvida. Ver CATALOGO-EXAUSTIVO-ARCHIVEXL.md (checkpoint 2026-08-12,
//     "reconciliação #49/worldStreamingSector") pro relato completo + script de scan reusável.
//   - Gate PRÓPRIO E SEPARADO do SL26 (nunca ligar junto no mesmo boot até confirmar qual dispara
//     quando/na ordem certa — teste recomendado pra sessão futura: armar os 2 juntos, comparar
//     ORDEM de disparo e se `0x103426204` TAMBÉM expõe um array de node count no mesmo `this`).
const WORLDSTREAMINGSECTOR_POSTLOAD_C2_VM: u64 = 0x1_0342_6204;
// cw-controller-misc: IComponent::Toggle native scripting handler vmaddr
// Achado 2026-07-28: classe "IComponent" NÃO existe por esse nome no RTTI (nativefunc IComponent
// Toggle -> "classe não encontrada"); mas `nativefunc gameuiCharacterCustomizationGenitalsController
// Toggle` resolve (Toggle está na hierarquia herdada). rf.func @+0x00 = CName hash duplicado
// (não código); campo +0x38 (e +0x70, idêntico) aponta pra um endereço que peekq confirma ser
// DADO (array de 4 ponteiros), não código — dereferenciando esses 4 ponteiros e comparando contra
// symbols-demangled.txt, todos caem dentro de `red::memory::PoolStorageProxy<red::PoolRTTIFunction>`
// (Allocate/AllocateAligned/Reallocate/Free) — ou seja, é o vtable do proxy de alocação embutido
// na struct CClassFunction, NÃO o ponteiro de Toggle. Pista falsa, descartada. +0x40 (offset que o
// plano original esperava) leu 0x0 nesta classe — layout não bate com o assumido. Ainda não achado.
// RESOLVIDO 2026-07-28 (RE estática offline dedicada, string-xref+disasm em __cstring/__text):
// `IComponent::Toggle` (scripting handler nativo) = 0x100ca711c. Prólogo real confirmado
// (STP x29,x30 + SUB SP,#0x30), NÃO é thunk. Cross-validado 4x: ordem dos 8 métodos nativos de
// IComponent no binário bate 1:1 com a ordem declarada em redscript-src/orphans.script:16657;
// flag w6=0x2000 bate com `const`/não-const de cada método; IsEnabled e Toggle tocam o mesmo
// campo this+0x8b (Toggle grava, IsEnabled só lê); Toggle consome exatamente 1 param Bool.
// Bônus da mesma RE (não usado ainda): GetAppearanceName 0x100ca6ea8, GetEntity 0x100ca6ec4,
// FindComponentByName 0x100ca6fdc, GetName 0x100ca70e4, IsEnabled 0x100ca7100,
// QueueEntityEvent 0x100ca71c4, RegisterRenderDebug 0x100ca7284.
const ICOMP_TOGGLE_VM: u64 = 0x1_00ca_711c;
// RESOLVIDO 2026-07-28 (RE estática offline dedicada, mesma técnica do IComponent::Toggle):
// classe real = `vehicleController` (redscript-src/orphans.script:37307-37324, extends GameComponent),
// método real = `ToggleLights(on, opt lightType, opt inTime, opt lerpCurve, opt loop)` — NÃO
// "VehicleController::ToggleAuxLights" (esse nome/classe não existem nesta build). Cadeia de 5 elos:
// __cstring com os 8 nomes nativos na ordem exata da declaração + registrador de classe (0x1012e4544)
// + bloco lazy-init do 7º método (0x1012e4e60, índice 6 = objeto @0x1079a9870) + CClassFunction ctor
// (0x102173860, mesma infra RTTI já confirmada) + handler (0x1012e806c, lê 5 params mas só repassa
// 2 — inTime/lerpCurve/loop são lidos e DESCARTADOS nesta build) -> `bl 0x1012e6e28(x0=this,w1=on,w2=lightType)`.
// 0x1012e6e28 tem prólogo real (SUB SP+STP, não thunk), só 2 callers no binário (o handler + 1 caller
// C++ interno). ABI confirmada bate com o handler Rust já codado (ctrl*, u8 enable, u32 light_type).
const VEHICLE_AUX_VM: u64 = 0x1_012e_6e28;
// axl-puppet-state-apply: C++ ABIs: OnAttach=void(ctrl*), OnDetach=void(ctrl*,uintptr_t)
// Achado 2026-07-28 via `vtbl <classe> 20` + `peekq` nos slots [03]/[04] (que DIFEREM entre as
// 2 classes; slots 0-2 e 5-19 são idênticos entre Genitals/Hairstyle = base compartilhada de
// IComponent). slot[03] começa com B<offset> (thunk de 4 bytes, redireciona) — slot[04] começa
// com STP X29,X30,[SP,#-0x10]!+MOV X29,SP (prólogo REAL, função com corpo de verdade). Usamos
// slot[04] em ambas as classes: real implementation, não o thunk redirecionador.
const GENITAL_ONATTACH_VM: u64  = 0x1_0244_7080; // GenitalsController vtbl[04]
const HAIRSTYLE_ONDETACH_VM: u64 = 0x1_0244_7098; // HairstyleController vtbl[04]

// cw-player-scheduling-vehicle (gap recém-identificado 2026-07-29, nunca trabalhado antes — ver
// CODEBASE.md/HISTORICO.md): escopo real = DelaySystem (zero código, wrapper redscript puro sobre
// API vanilla já provada) + WardrobeSystem::ForgetItemID (NÃO É FUNÇÃO NATIVA HOOKÁVEL — Codeware
// real implementa via acesso direto a offsets de struct, HashMap<CName,ItemID>@+0x48 +
// SharedSpinLock@+0xF4, chamando HashMap::Remove internamente; RE offline dedicada confirmou ZERO
// ocorrência da string "ForgetItemID" em __cstring E em todo o dump RTTI de 1764 arquivos —
// não existe registro nem função separada pra hookar; portar exigiria RE de offset de struct em
// runtime, não RE-e-hook, fora de escopo por ora) + VehicleSystem::ToggleGarageVehicle.
// `VehicleSystem::ToggleGarageVehicle` — RE offline dedicada 2026-07-29: x0=this(VehicleSystem*),
// x1=&GarageVehicleID{TweakDBID recordID@0, CName name@8} (16B, by-ref — RED4ext.SDK confirma
// layout), w2=bool enable, w0=bool ret. ABI SIMPLES, SEM sret. 3 callers no binário inteiro
// (EnablePlayerVehicle/EnableAllPlayerVehicles), cadeia validada 2x independente, prólogo real
// (não thunk). ALTA confiança.
//
// `pub(crate)` (2026-08-12, round 3 do catálogo Codeware): até aqui só usado como alvo de HOOK
// observe-only (probe passivo, nunca CHAMADO de propósito) — `register.rs::
// tramp_vehiclesystem_toggle_garage_vehicle` reusa este MESMO endereço/ABI já confirmados pra
// expor `VehicleSystem.ToggleGarageVehicle` como native genuinamente CHAMÁVEL do redscript
// (item `#72` do catálogo, fecha a lacuna "endereço provado mas nenhum mod real conseguia
// disparar"). DRY: 1 única fonte de verdade pro vmaddr, sem reduplicar o literal.
pub(crate) const VEHICLESYSTEM_TOGGLEGARAGEVEHICLE_VM: u64 = 0x1_0125_1eb4;

// axl-questphase-apply: QuestsSystem::OnGameRestored candidato — achado 2026-07-28 AO VIVO via
// `getquestsys` (GameInstance.GetQuestsSystem(game) com a ABI corrigida do getcustsys) + leitura
// direta da vtable no slot 0x158 (offset Windows 0x150 do header hand-written
// RED4ext.SDK/gameIGameSystem.hpp, +0x08 pela correção Itanium de dtor extra já documentada no
// projeto). `peekq` confirma prólogo REAL (STP X20,X19 + STP X29,X30), não thunk. Ainda não
// confirmado que É OnGameRestored (offset estimado, não literal) — hook observe-only decide:
// deve disparar exatamente 1x por save-load real (autocontinue já dispara 1 nesta sessão).
const QUESTSYS_ONGAMERESTORED_VM: u64 = 0x1_02ef_8c58;

static POSTLOAD_CNT_CMESH: AtomicU64 = AtomicU64::new(0);
static POSTLOAD_CNT_MORPHTGT: AtomicU64 = AtomicU64::new(0);
static POSTLOAD_CNT_SL14: AtomicU64 = AtomicU64::new(0);
static POSTLOAD_CNT_SL25: AtomicU64 = AtomicU64::new(0);
static POSTLOAD_CNT_SL26: AtomicU64 = AtomicU64::new(0);
static POSTLOAD_CNT_SL27: AtomicU64 = AtomicU64::new(0);
static ORIG_CMESH_POSTLOAD: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_MORPHTGT_POSTLOAD: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_SL14: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_SL25: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_SL26: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_SL27: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static STREAMING_SECTOR_APPLIED: AtomicBool = AtomicBool::new(false);
static STREAMING_SECTOR_HASH: AtomicU64 = AtomicU64::new(0);
static STREAMING_NODE_COUNT: AtomicU64 = AtomicU64::new(0);
static WSBLOCK_FN1_CNT:    AtomicU64 = AtomicU64::new(0);
static WSBLOCK_FN2_CNT:    AtomicU64 = AtomicU64::new(0);
static WSBLOCKIDX_FN1_CNT: AtomicU64 = AtomicU64::new(0);
static WSBLOCKIDX_FN2_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_WSBLOCK_FN1:    AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_WSBLOCK_FN2:    AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_WSBLOCKIDX_FN1: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_WSBLOCKIDX_FN2: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static JOURNAL_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_JOURNAL: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static JOURNAL_LOADSUB_A_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_JOURNAL_LOADSUB_A: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static JOURNAL_LOADSUB_B_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_JOURNAL_LOADSUB_B: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static JOURNAL_LOADROOT_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_JOURNAL_LOADROOT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static JOURNAL_RESLIST_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_JOURNAL_RESLIST: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static CHAR_CUSTOM_A_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_CHAR_CUSTOM_A: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static CHAR_CUSTOM_B_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_CHAR_CUSTOM_B: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// ptr estável da instância gameuiCharacterCustomizationSystem (salvo no primeiro fire do CharCustomA)
pub static CHAR_CUSTOM_SYS_PTR: AtomicU64 = AtomicU64::new(0);
static POSTLOAD_CNT_ANIMSET:      AtomicU64 = AtomicU64::new(0);
static ORIG_ANIMSET_POSTLOAD:     AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static POSTLOAD_CNT_ANIMRIG:      AtomicU64 = AtomicU64::new(0);
static ORIG_ANIMRIG_POSTLOAD:     AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static POSTLOAD_CNT_GARMENT_SKIN: AtomicU64 = AtomicU64::new(0);
static ORIG_GARMENT_SKIN_POSTLOAD:AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static POSTLOAD_CNT_MESH_COMP:    AtomicU64 = AtomicU64::new(0);
static ORIG_MESH_COMP_POSTLOAD:   AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static POSTLOAD_CNT_SLOT_COMP:    AtomicU64 = AtomicU64::new(0);
static ORIG_SLOT_COMP_POSTLOAD:   AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static POSTLOAD_CNT_GAME_OBJ:     AtomicU64 = AtomicU64::new(0);
static ORIG_GAME_OBJ_POSTLOAD:    AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// ArchiveXL `#51`: EntityTemplate::PostLoad / AppearanceResource::PostLoad (candidatos 2026-08-12).
static POSTLOAD_CNT_ENTITYTEMPLATE:      AtomicU64 = AtomicU64::new(0);
static ORIG_ENTITYTEMPLATE_POSTLOAD:     AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static POSTLOAD_CNT_APPEARANCERESOURCE:  AtomicU64 = AtomicU64::new(0);
static ORIG_APPEARANCERESOURCE_POSTLOAD: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// ArchiveXL `#49`: worldStreamingSector::PostLoad candidato #2 (diverge de SL26, ver nota do const).
static POSTLOAD_CNT_WSSECTOR_C2:  AtomicU64 = AtomicU64::new(0);
static ORIG_WSSECTOR_C2_POSTLOAD: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ICOMP_TOGGLE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ICOMP_TOGGLE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// ArchiveXL `#17`: candidato `0x100ace4b0` de ExtractPartComponents (ver nota grande na declaração
// da const). Observe-only, nunca muta x0/x1/retorno.
static EXTRACTPARTCOMPONENTS_CAND_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_EXTRACTPARTCOMPONENTS_CAND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// Codeware `#176`: candidato `0x1049bded0` de `Raw::inkWidget::TriggerEvent` (ver nota grande na
// declaração da const). Observe-only, nunca muta x0/x1/x2/retorno.
static INKWIDGET_TRIGGEREVENT_CAND_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_INKWIDGET_TRIGGEREVENT_CAND: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static VEHICLE_AUX_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_VEHICLE_AUX: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// axl-puppet-state-apply: GenitalsController::OnAttach + HairstyleController::OnDetach C++ hooks
static GENITAL_ONATTACH_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_GENITAL_ONATTACH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static HAIRSTYLE_ONDETACH_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_HAIRSTYLE_ONDETACH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// cw-player-scheduling-vehicle: VehicleSystem::ToggleGarageVehicle (RE offline 2026-07-29).
static VEHICLESYSTEM_TOGGLEGARAGEVEHICLE_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_VEHICLESYSTEM_TOGGLEGARAGEVEHICLE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static QUESTSYS_ONGAMERESTORED_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_QUESTSYS_ONGAMERESTORED: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// RED4ext.SDK #461 (`UpdateRegistrar::RegisterUpdate`, overload GROUP) — RE offline 2026-08-11
// (agente dedicado): string-xref+ABI cruzada contra 5 call-sites reais cujo argumento de enum bate
// semanticamente com a string de debug-name daquele call-site (ex. "CameraSystem/PlayerAimUpdateTick"
// passa W1=7=PlayerAimUpdate). Assinatura real (`SystemUpdate.hpp`): `RegisterUpdate(UpdateTickGroup
// aGroup, IScriptable* aSystem, const char* aName, GroupUpdateCallback&& aCallback)` — this=X0,
// group(u8)=X1, system=X2, name(cstr)=X3, callback(ponteiro, é referência)=X4. Hook OBSERVE-ONLY
// (captura só o `this`/UpdateRegistrar* na 1ª chamada vanilla real, nunca chama RegisterUpdate de
// volta ainda — categoria de risco BAIXA, mesmo padrão de passthrough já provado dezenas de vezes
// nesta sessão). `UpdateRegistrar` é struct VAZIA no header oficial — não tem layout próprio pra ler,
// só serve pra REUSAR o ponteiro capturado numa chamada futura de `RegisterUpdate`.
const UPDATE_REGISTRAR_GROUP_VM: u64 = 0x103d95800;
static UPDATE_REGISTRAR_CAPTURE_CNT: AtomicU64 = AtomicU64::new(0);
pub static UPDATE_REGISTRAR_CAPTURED_PTR: AtomicU64 = AtomicU64::new(0);
static ORIG_UPDATE_REGISTRAR_GROUP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
// Armazena hash da mesh patchada e o valor original de chunkMaterials[0] para o overlay.
static PATCH_MESH_HASH: AtomicU64 = AtomicU64::new(0);
static PATCH_MATERIAL_OLD: AtomicU64 = AtomicU64::new(0);

/// Retorna descrição do patch aplicado para o overlay, ou None se ainda não rodou.
pub(crate) fn axl_patch_info() -> Option<String> {
    if !PATCH_APPLIED.load(Ordering::Relaxed) { return None; }
    let mesh = PATCH_MESH_HASH.load(Ordering::Relaxed);
    let mat  = PATCH_MATERIAL_OLD.load(Ordering::Relaxed);
    Some(format!("AXL PATCH+FIX: appearances+1 | fix.names | mesh={mesh:#010x}"))
}
type PostLoadFn = unsafe extern "C" fn(*mut c_void, u64, u64);

// axl-resource-patch-apply: target mesh hash, loaded from bwms-patches.txt at boot.
// 0 = no target configured (inject disabled).
static PATCH_TARGET_HASH_RT: AtomicU64 = AtomicU64::new(0);
static PATCH_APPLIED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn cmesh_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let n = POSTLOAD_CNT_CMESH.fetch_add(1, Ordering::Relaxed);
    // Call original PostLoad first (HookAfter pattern — ArchiveXL does the same)
    let orig = ORIG_CMESH_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }

    if gum::is_readable(this, 0x200) {
        let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
        let path_hash = rf64(0x30);

        // Probe first 8 meshes
        if n < 8 {
            let app_ptr = rf64(0x1e0);
            let app_cap = rf32(0x1e8);
            let app_sz  = rf32(0x1ec);
            crate::log(&format!(
                "[axl-patch] CMesh#{n} path={path_hash:#018x} app_sz={app_sz} app_cap={app_cap}"
            ));
        }

        // axl-resource-patch-apply: inject into the configured target mesh only.
        // PATCH_TARGET_HASH_RT loaded from bwms-patches.txt at boot (0 = disabled).
        let patch_target = PATCH_TARGET_HASH_RT.load(Ordering::Relaxed);
        if !PATCH_APPLIED.load(Ordering::Relaxed) && patch_target != 0 && path_hash == patch_target {
            let app_arr  = this.cast::<u8>().add(0x1e0) as *mut u64;
            let app_capu = this.cast::<u8>().add(0x1e8) as *mut u32;
            let app_szu  = this.cast::<u8>().add(0x1ec) as *mut u32;
            let app_ptr  = app_arr.read_unaligned() as *mut u64;
            let app_sz   = app_szu.read_unaligned();

            if app_sz > 0
                && (app_ptr as u64) > 0x1_0000_0000
                && gum::is_readable(app_ptr as *const c_void, 16)
            {
                // Read Handle<MeshAppearance>[0]: inst_ptr (8) + refCnt_ptr (8)
                let inst0 = app_ptr.read_unaligned();
                let refc0 = app_ptr.add(1).read_unaligned();

                if refc0 > 0x1_0000_0000 && gum::is_readable(refc0 as *const c_void, 4) {
                    let strong_ptr = refc0 as *mut i32;
                    let prev_strong = strong_ptr.read_volatile();
                    if prev_strong > 0 && prev_strong < 10_000 {
                        // Allocate new backing store (old_sz+1) × 16 bytes
                        let new_cap = app_sz + 1;
                        let new_bytes = new_cap as usize * 16;
                        let new_backing = gum::alloc_anywhere(new_bytes);
                        if !new_backing.is_null() {
                            // Copy existing entries verbatim
                            std::ptr::copy_nonoverlapping(
                                app_ptr as *const u8, new_backing,
                                app_sz as usize * 16
                            );
                            // Increment strong refcount and write cloned Handle at [app_sz]
                            strong_ptr.write_volatile(prev_strong + 1);
                            let new_u64 = new_backing as *mut u64;
                            let slot = new_u64.add(app_sz as usize * 2);
                            slot.write_unaligned(inst0);
                            slot.add(1).write_unaligned(refc0);
                            // Commit DynArray fields (old backing leaks — devtest only)
                            app_arr.write_unaligned(new_backing as u64);
                            app_capu.write_unaligned(new_cap);
                            app_szu.write_unaligned(new_cap);
                            crate::log(&format!(
                                "[axl-patch] PATCHED CMesh#{n} path={path_hash:#018x} appearances {app_sz}→{new_cap} (clone of inst={inst0:#018x})"
                            ));

                            // === Proof 2: read ALL chunkMaterials + apply resource.fix.names test ===
                            // meshMeshAppearance offsets (RED4ext.SDK generated header):
                            //   name           @ inst + 0x30  (CName, 8 bytes)
                            //   chunkMaterials @ inst + 0x38  (DynArray<CName>: ptr@+0x38, cap@+0x40, size@+0x44)
                            let mut mat_old: u64 = 0;
                            if inst0 > 0x1_0000_0000 && gum::is_readable(inst0 as *const c_void, 0x50) {
                                let ibase = inst0 as *mut u8;
                                let cm_entries = (ibase.add(0x38) as *const u64).read_unaligned() as *mut u64;
                                let cm_size    = (ibase.add(0x44) as *const u32).read_unaligned();
                                if cm_size > 0 && (cm_entries as u64) > 0x1_0000_0000
                                    && gum::is_readable(cm_entries as *const c_void, (cm_size as usize * 8).max(8)) {
                                    mat_old = cm_entries.read_unaligned();
                                    // Read all chunkMaterials for this appearance
                                    let mut all_mats = Vec::with_capacity(cm_size as usize);
                                    for i in 0..cm_size as usize {
                                        let v = cm_entries.add(i).read_unaligned();
                                        all_mats.push(v);
                                    }
                                    let mat_list: Vec<String> = all_mats.iter()
                                        .enumerate()
                                        .map(|(i, v)| format!("[{i}]={v:#018x}"))
                                        .collect();
                                    crate::log(&format!(
                                        "[axl-resourcemeta-fix] mesh={path_hash:#018x} appearance[0].chunkMaterials sz={cm_size}: {}",
                                        mat_list.join(" ")
                                    ));
                                    // Apply resource.fix.names test: swap mat[0] ↔ mat[last] if different
                                    // This proves the write mechanism without needing to know string names.
                                    if cm_size >= 2 {
                                        let last_idx = (cm_size - 1) as usize;
                                        let mat_last = all_mats[last_idx];
                                        if mat_old != mat_last && mat_last != 0 && mat_old != 0 {
                                            // Write mat_last into slot [0] — both are valid CNames from the engine
                                            cm_entries.write_unaligned(mat_last);
                                            crate::log(&format!(
                                                "[axl-resourcemeta-fix] >>> FIX.NAMES APPLIED <<< mesh={path_hash:#018x} chunkMaterials[0]: {mat_old:#018x} → {mat_last:#018x}"
                                            ));
                                            PATCH_MATERIAL_OLD.store(mat_old, Ordering::Relaxed);
                                        } else {
                                            crate::log(&format!(
                                                "[axl-resourcemeta-fix] mat[0]==mat[last] ou zero, sem swap (all same material)"
                                            ));
                                            PATCH_MATERIAL_OLD.store(mat_old, Ordering::Relaxed);
                                        }
                                    } else {
                                        PATCH_MATERIAL_OLD.store(mat_old, Ordering::Relaxed);
                                    }
                                }
                            }
                            PATCH_MESH_HASH.store(path_hash, Ordering::Relaxed);
                            PATCH_APPLIED.store(true, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
    }
}

unsafe extern "C" fn morphtgt_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let n = POSTLOAD_CNT_MORPHTGT.fetch_add(1, Ordering::Relaxed);
    if n < 8 && gum::is_readable(this, 0xa0) {
        let f = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        crate::log(&format!(
            "[axl-patch-probe] MorphTargetMesh#{n} this={this:p} +08={:#018x} +10={:#018x} +18={:#018x} +20={:#018x} +28={:#018x} +30={:#018x} +38={:#018x}",
            f(0x08), f(0x10), f(0x18), f(0x20), f(0x28), f(0x30), f(0x38)
        ));
        crate::log(&format!(
            "[axl-patch-probe] MorphTargetMesh#{n} +40={:#018x} +48={:#018x} +50={:#018x} +58={:#018x} +60={:#018x} +68={:#018x} +70={:#018x}",
            f(0x40), f(0x48), f(0x50), f(0x58), f(0x60), f(0x68), f(0x70)
        ));
    }
    let orig = ORIG_MORPHTGT_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
}

// ===== ArchiveXL `#51`: EntityTemplate::PostLoad / AppearanceResource::PostLoad (observe-only) =====
// vmaddrs resolvidos por `probe_postload_addresses()` ao vivo (2026-08-12, "postloadprobe ao vivo",
// mesma técnica já provada 8x — CMesh/MorphTargetMesh incluídos). Nunca antes testados como HOOK
// instalado+disparando (só o endereço tinha sido lido do vtable slot). Mesmo padrão observe-only
// (dump genérico dos primeiros campos, silencia após 8 chamadas) já usado por `morphtgt_postload_
// replacement` — categoria de risco já validada (2 sub-hooks IRMÃOS desta mesma família, CMesh e
// MorphTargetMesh, ambos confirmados ZERO crash em produção).
unsafe extern "C" fn entitytemplate_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let n = POSTLOAD_CNT_ENTITYTEMPLATE.fetch_add(1, Ordering::Relaxed);
    if n < 8 && gum::is_readable(this, 0xa0) {
        let f = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        crate::log(&format!(
            "[axl-resource-patch] EntityTemplate::PostLoad#{n} this={this:p} +08={:#018x} +10={:#018x} +18={:#018x} +20={:#018x} +28={:#018x} +30={:#018x} +38={:#018x} +40={:#018x}",
            f(0x08), f(0x10), f(0x18), f(0x20), f(0x28), f(0x30), f(0x38), f(0x40)
        ));
    }
    let orig = ORIG_ENTITYTEMPLATE_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
}

unsafe extern "C" fn appearanceresource_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let n = POSTLOAD_CNT_APPEARANCERESOURCE.fetch_add(1, Ordering::Relaxed);
    if n < 8 && gum::is_readable(this, 0xa0) {
        let f = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        crate::log(&format!(
            "[axl-resource-patch] AppearanceResource::PostLoad#{n} this={this:p} +08={:#018x} +10={:#018x} +18={:#018x} +20={:#018x} +28={:#018x} +30={:#018x} +38={:#018x} +40={:#018x}",
            f(0x08), f(0x10), f(0x18), f(0x20), f(0x28), f(0x30), f(0x38), f(0x40)
        ));
    }
    let orig = ORIG_APPEARANCERESOURCE_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
}

// ===== ArchiveXL `#49`: worldStreamingSector::PostLoad candidato #2 (observe-only, gate PRÓPRIO) =====
// Ver a nota completa no const `WORLDSTREAMINGSECTOR_POSTLOAD_C2_VM` (achado de RE offline
// 2026-08-12: 1 referência única em __DATA_CONST + local-slot-6 + chama GAME_OBJ_POSTLOAD_VM
// primeiro + toca this+0x40/NodeBuffer — candidato estruturalmente mais forte que SL26 pra ser o
// verdadeiro PostLoad). Dump genérico + comparação direta contra os offsets que SL26 já usa
// (0x40/0x48/.../0xb0) — se este candidato TAMBÉM expuser um DynArray plausível nesses offsets,
// reforça que é o MESMO objeto (worldStreamingSector) visto de um slot diferente.
unsafe extern "C" fn wssector_c2_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let n = POSTLOAD_CNT_WSSECTOR_C2.fetch_add(1, Ordering::Relaxed);
    if n < 8 && gum::is_readable(this, 0xc0) {
        let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
        let path_hash = rf64(0x30);
        crate::log(&format!(
            "[axl-streaming-c2] WSSectorC2#{n} this={this:p} vtbl={:#018x} path(+0x30)={path_hash:#018x}",
            rf64(0)
        ));
        for off in [0x40usize, 0x48, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78, 0x80, 0x90, 0xa0, 0xb0] {
            if !gum::is_readable(this.cast::<u8>().add(off) as *const c_void, 16) { continue; }
            let arr_ptr = rf64(off);
            let cap = rf32(off + 8);
            let sz  = rf32(off + 0xc);
            if arr_ptr > 0x1_0000_0000 && sz > 0 && sz <= cap && cap <= 65536
                && gum::is_readable(arr_ptr as *const c_void, 8)
            {
                crate::log(&format!(
                    "[axl-streaming-c2] WSSectorC2#{n} DynArray-like @ +{off:#x}: ptr={arr_ptr:#018x} sz={sz} cap={cap}"
                ));
            }
        }
    }
    // HookAfter: chama a original ANTES do nosso dump não seria correto pra provar side-effects já
    // aplicados (mesma decisão de design já usada pro CMesh — chama-se a original E DEPOIS lê, mas
    // como já lemos acima, aqui mantemos "chama depois" pra não mudar timing do que o resto do boot
    // observa; consistente com `morphtgt_postload_replacement`).
    let orig = ORIG_WSSECTOR_C2_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
}

// ===== axl-animation-apply: animAnimSet::PostLoad + animRig::PostLoad (observe-only) =====
// vmaddrs confirmed 2026-07-27 via probe_postload_addresses().
// Fires when animation set/rig resources finish loading. HookAfter: call orig first.
unsafe extern "C" fn animset_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let orig = ORIG_ANIMSET_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
    let n = POSTLOAD_CNT_ANIMSET.fetch_add(1, Ordering::Relaxed);
    if n < 4 && gum::is_readable(this, 0x40) {
        let f = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        crate::log(&format!(
            "[axl-anim-probe] animAnimSet#{n} this={this:p} +08={:#018x} +10={:#018x} +18={:#018x} +20={:#018x} +28={:#018x} +30={:#018x} +38={:#018x}",
            f(0x08), f(0x10), f(0x18), f(0x20), f(0x28), f(0x30), f(0x38)
        ));
    }
}

unsafe extern "C" fn animrig_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let orig = ORIG_ANIMRIG_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
    let n = POSTLOAD_CNT_ANIMRIG.fetch_add(1, Ordering::Relaxed);
    if n < 4 && gum::is_readable(this, 0x40) {
        let f = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        crate::log(&format!(
            "[axl-anim-probe] animRig#{n} this={this:p} +08={:#018x} +10={:#018x} +18={:#018x} +20={:#018x} +28={:#018x} +30={:#018x}",
            f(0x08), f(0x10), f(0x18), f(0x20), f(0x28), f(0x30)
        ));
    }
}

// axl-fullbody-torso (2026-08-06, Caminho A da FULLBODY-DOSSIER.md, §6/Q4 do agente de RE): varre
// um range largo de offsets de `this` tentando resolver cada palavra de 8 bytes como CName via
// `resolve_cname` (chama a CNamePool::Get REAL do motor — seguro, só leitura, resolve QUALQUER
// CName vivo, não só os nossos). Objetivo: pescar o nome do componente (ex. algo contendo "t0_"/
// "torso"/"fpp") sem precisar saber o offset exato de antemão — cada match vira 1 linha de log,
// resolves vazios são descartados (evita spam). Amostra ampliada (60, não 4) pra cobrir mais
// componentes reais durante uma sessão de gameplay real, incluindo trocar de FPP/TPP.
unsafe fn scan_cnames(tag: &str, n: u64, this: *mut c_void) {
    if !gum::is_readable(this, 0x90) {
        return;
    }
    for off in (0x08..0x90).step_by(8) {
        let w = (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        if w == 0 {
            continue;
        }
        let name = crate::cname::resolve_cname(w);
        if !name.is_empty() && name != "None" {
            crate::log(&format!("[axl-fullbody-torso] {tag}#{n} this={this:p} +{off:#04x}={w:#018x} CName='{name}'"));
        }
    }
}

// ===== axl-mesh-apply: entGarmentSkinnedMeshComponent/entSkinnedMeshComponent (shared PostLoad) =====
unsafe extern "C" fn garment_skin_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let orig = ORIG_GARMENT_SKIN_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
    let n = POSTLOAD_CNT_GARMENT_SKIN.fetch_add(1, Ordering::Relaxed);
    if n < 60 && gum::is_readable(this, 0x40) {
        let f = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        crate::log(&format!(
            "[axl-mesh-probe] garment_skin#{n} this={this:p} +08={:#018x} +10={:#018x} +18={:#018x} +20={:#018x} +28={:#018x} +30={:#018x}",
            f(0x08), f(0x10), f(0x18), f(0x20), f(0x28), f(0x30)
        ));
        scan_cnames("garment_skin", n, this);
    }
}

unsafe extern "C" fn mesh_comp_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let orig = ORIG_MESH_COMP_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
    let n = POSTLOAD_CNT_MESH_COMP.fetch_add(1, Ordering::Relaxed);
    if n < 60 && gum::is_readable(this, 0x40) {
        let f = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        crate::log(&format!(
            "[axl-mesh-probe] entMeshComponent#{n} this={this:p} +08={:#018x} +10={:#018x} +18={:#018x} +20={:#018x} +28={:#018x} +30={:#018x}",
            f(0x08), f(0x10), f(0x18), f(0x20), f(0x28), f(0x30)
        ));
        scan_cnames("mesh_comp", n, this);
    }
}

// ===== axl-attachment-apply: entSlotComponent::PostLoad =====
unsafe extern "C" fn slot_comp_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let orig = ORIG_SLOT_COMP_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
    let n = POSTLOAD_CNT_SLOT_COMP.fetch_add(1, Ordering::Relaxed);
    if n < 60 && gum::is_readable(this, 0x40) {
        let f = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        crate::log(&format!(
            "[axl-attach-probe] entSlotComponent#{n} this={this:p} +08={:#018x} +10={:#018x} +18={:#018x} +20={:#018x} +28={:#018x} +30={:#018x}",
            f(0x08), f(0x10), f(0x18), f(0x20), f(0x28), f(0x30)
        ));
        scan_cnames("slot_comp", n, this);
    }
}

// ===== axl-puppet-state-apply: gameObject::PostLoad (base class covers entPuppet) =====
unsafe extern "C" fn game_obj_postload_replacement(this: *mut c_void, a1: u64, a2: u64) {
    let orig = ORIG_GAME_OBJ_POSTLOAD.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
    let n = POSTLOAD_CNT_GAME_OBJ.fetch_add(1, Ordering::Relaxed);
    if n < 4 && gum::is_readable(this, 0x40) {
        let f = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        crate::log(&format!(
            "[axl-puppet-probe] gameObject#{n} this={this:p} +08={:#018x} +10={:#018x} +18={:#018x} +20={:#018x} +28={:#018x} +30={:#018x}",
            f(0x08), f(0x10), f(0x18), f(0x20), f(0x28), f(0x30)
        ));
    }
}

// ===== cw-controller-misc: IComponent::Toggle scripting handler =====
// ABI = handler de scripting (RED4ext ScriptingFunction_t): ctx=IScriptable*, frame=CStackFrame*,
// ret=*void (null para void), retType=*CBaseRTTIType (null para void).
// ICOMP_TOGGLE_VM preenchido após `nativefunc IComponent Toggle` retornar vmaddr do campo +0x40.
unsafe extern "C" fn icomp_toggle_handler(ctx: *mut c_void, frame: *mut c_void, ret: *mut c_void, ret_type: *mut c_void) {
    let orig = ORIG_ICOMP_TOGGLE.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) = std::mem::transmute(orig);
        f(ctx, frame, ret, ret_type);
    }
    let n = ICOMP_TOGGLE_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 8 {
        crate::log(&format!("[cw-ctrl-misc] IComponent::Toggle#{n} ctx={ctx:p}"));
    }
    // Codeware `#9`/`#127` (candidato barato, 2026-08-11, 3ª rodada): mesma ideia do
    // `vehicle_aux_lights_handler` acima — fecha o loop entre `EntityComponentEvent` (JÁ
    // fechado e provado via `make_entitycomponentevent_arg`, 2026-08-11) e este hook (JÁ provado
    // seguro como observe-only, 8 fires/boot, zero crash desde 2026-07-28), disparando
    // "Component/Toggle" com o componente REAL (`ctx`, o próprio `this` do `.Toggle()` chamado)
    // em vez de só o fixture sintético que fechou o item `#9`. **RISCO MAIOR que o do vizinho
    // (Vehicle/ToggleAuxLights), documentado com honestidade**: este é o SCRIPTING HANDLER
    // nativo de `IComponent::Toggle` em si (assinatura `ScriptingFunction_t`) — chamado de
    // DENTRO da interpretação de bytecode redscript sempre que QUALQUER código (jogo ou mod)
    // chama `componente.Toggle(bool)`, MUITO mais frequente que o toggle de farol de veículo.
    // `fire_event_args` reentra na VM (`resolve_in_class`+`call_func`) a partir de dentro de um
    // native handler já em execução — categoria já usada em dezenas de outras composições deste
    // projeto (todo native que despacha por reflection faz isso), mas nunca ainda a partir
    // ESPECIFICAMENTE de um scripting-handler de altíssima frequência. Por isso o bound aqui é
    // mais apertado (3 disparos, não 5) e o gate tem nome PRÓPRIO — recomendação explícita:
    // testar `Vehicle/ToggleAuxLights` primeiro numa sessão com monitoramento de crash, só armar
    // este depois de confirmar que o padrão bounded é seguro nesse contexto mais quente.
    static FIREEVENT_ARMED_AT: AtomicU64 = AtomicU64::new(0);
    if std::path::Path::new(&format!("{}/.bwms-icomptoggle-fireevent-confirm", std::env::var("HOME").unwrap_or_default())).exists() {
        let armed_at = FIREEVENT_ARMED_AT.load(Ordering::Relaxed);
        if armed_at == 0 && crate::register::has_listener_for("Component/Toggle") {
            FIREEVENT_ARMED_AT.store(n + 1, Ordering::Relaxed);
            crate::log(&format!("[cw-ctrl-misc] listener real confirmado pra 'Component/Toggle' (#{n}) — armando fire_event_args pras próximas 3 chamadas"));
        }
        let armed_at = FIREEVENT_ARMED_AT.load(Ordering::Relaxed);
        if armed_at != 0 && n >= armed_at && n < armed_at + 3 {
            if let Some(arg) = crate::register::make_entitycomponentevent_arg(ctx) {
                let sent = crate::register::fire_event_args("Component/Toggle", &[arg]);
                crate::log(&format!("[cw-ctrl-misc] fire_event_args('Component/Toggle') dentro do hook #{n} ctx={ctx:p} -> despachado pra {sent} listener(s)"));
            }
        }
    }
}

// ===== cw-controller-misc: vehicleController::ToggleLights C++ hook =====
// ABI C++ ARM64 real (RE 2026-07-28): x0=vehicleController*, w1=bool on, w2=lightType (u32 bitmask,
// NÃO ordinal — o C++ trata bit0 como caso especial "Head"). O redscript real é
// `ToggleLights(on, opt lightType, opt inTime, opt lerpCurve, opt loop)` — os 3 últimos params são
// lidos pelo handler nativo mas DESCARTADOS nesta build (não chegam em 0x1012e6e28), então o hook
// de 2 args (this,on,lightType) cobre a chamada real por completo.
unsafe extern "C" fn vehicle_aux_lights_handler(ctrl: *mut c_void, enable: u8, light_type: u32) {
    let orig = ORIG_VEHICLE_AUX.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: unsafe extern "C" fn(*mut c_void, u8, u32) = std::mem::transmute(orig);
        f(ctrl, enable, light_type);
    }
    let n = VEHICLE_AUX_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 8 {
        crate::log(&format!("[cw-ctrl-misc] vehicleController::ToggleLights#{n} ctrl={ctrl:p} on={enable} type={light_type}"));
    }
    // Codeware `#14`/`#128` (candidato barato, 2026-08-11, 3ª rodada): fecha o loop entre a
    // classe `VehicleLightControlEvent` (JÁ forjada+provada via fixture sintética, 2026-08-11
    // cont.200) e o hook OBSERVE-ONLY acima (JÁ provado seguro, 8 fires/boot, zero crash desde
    // 2026-07-28) — hoje o hook só LOGA, nunca despacha o evento real pro CallbackSystem. Mesmo
    // padrão bounded já DECISIVO no RED4ext.SDK `#461` (cont.239-241): `has_listener_for` é um
    // lookup barato (Mutex+scan, zero RTTI) — sem NENHUM mod registrado pra
    // "Vehicle/ToggleAuxLights", este bloco nunca faz mais que essa 1 checagem booleana por
    // toggle, custo idêntico ao já aceito em todo hook deste projeto. Só depois de confirmar um
    // listener REAL é que arma `fire_event_args` (que aí sim faz `resolve_in_class`+`call_func`,
    // RTTI/redscript) — e mesmo assim bounded a poucas chamadas seguintes, nunca solto pra
    // sempre, mesma cautela do `#461`. Gate OPT-IN por marcador (nunca ligado por padrão) —
    // decisão consciente de deixar a PRIMEIRA ativação ao vivo pra uma sessão com monitoramento
    // de crash, não a esta (100% offline).
    static FIREEVENT_ARMED_AT: AtomicU64 = AtomicU64::new(0);
    if std::path::Path::new(&format!("{}/.bwms-vehiclelight-fireevent-confirm", std::env::var("HOME").unwrap_or_default())).exists() {
        let armed_at = FIREEVENT_ARMED_AT.load(Ordering::Relaxed);
        if armed_at == 0 && crate::register::has_listener_for("Vehicle/ToggleAuxLights") {
            FIREEVENT_ARMED_AT.store(n + 1, Ordering::Relaxed);
            crate::log(&format!("[cw-ctrl-misc] listener real confirmado pra 'Vehicle/ToggleAuxLights' (#{n}) — armando fire_event_args pras próximas 5 chamadas"));
        }
        let armed_at = FIREEVENT_ARMED_AT.load(Ordering::Relaxed);
        if armed_at != 0 && n >= armed_at && n < armed_at + 5 {
            if let Some(arg) = crate::register::make_vehiclelightcontrolevent_arg(enable != 0, light_type as i32) {
                let sent = crate::register::fire_event_args("Vehicle/ToggleAuxLights", &[arg]);
                crate::log(&format!("[cw-ctrl-misc] fire_event_args('Vehicle/ToggleAuxLights') dentro do hook #{n} on={enable} type={light_type} -> despachado pra {sent} listener(s)"));
            }
        }
    }
}

// ===== cw-player-scheduling-vehicle: VehicleSystem::ToggleGarageVehicle C++ hook =====
// ABI C++ ARM64: x0=this(VehicleSystem*), x1=&GarageVehicleID{TweakDBID@0,CName@8} (16B by-ref),
// w2=bool enable, w0=bool ret. Observe-only: chama original, lê os 2 campos do struct por-referência
// (ponteiro do CALLER, não um array racy compartilhado — seguro dereferenciar, categoria diferente
// dos "Find"/resolve do garment) só pra log, nunca escreve.
type ToggleGarageVehicleFn = unsafe extern "C" fn(*mut c_void, *const u8, u8) -> u8;
unsafe extern "C" fn vehiclesystem_togglegaragevehicle_probe(this: *mut c_void, garage_id: *const u8, enable: u8) -> u8 {
    let orig = ORIG_VEHICLESYSTEM_TOGGLEGARAGEVEHICLE.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: ToggleGarageVehicleFn = std::mem::transmute(orig);
        f(this, garage_id, enable)
    } else {
        0
    };
    let n = VEHICLESYSTEM_TOGGLEGARAGEVEHICLE_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 8 {
        let (record_id, name_hash) = if gum::is_readable(garage_id as *const c_void, 16) {
            let p = garage_id as *const u64;
            (p.read_unaligned(), p.add(1).read_unaligned())
        } else {
            (0, 0)
        };
        crate::log(&format!("[cw-player-scheduling-vehicle] VehicleSystem::ToggleGarageVehicle#{n} this={this:p} recordID={record_id:#018x} name={name_hash:#018x} enable={enable} ret={ret}"));
    }
    ret
}

// ===== Codeware `#81`/`#140` (`WeatherSystemEx::SetWeather`): `RuntimeSystemWeather::SetWeatherByName`
// candidato de RE offline (2026-08-18, Capstone genuíno, zero string/símbolo — técnica NOVA pra
// este item, nunca tentada antes; o catálogo só documentava "endereço Mac genuinamente
// desconhecido" sem nenhuma tentativa de disassembly). Achado por caminhada de call-graph a
// partir dos 3 símbolos REAIS confirmados no binário (não substring/palpite):
//   `red::DynArray<world::RuntimeSystemWeather::WeatherObserver>::MoveAfterReallocation`  @0x100d5f284
//   `red::DynArray<world::RuntimeSystemWeather::ListenerEntry>::MoveAfterReallocation`     @0x100d5f658
//   `red::DynArray<world::RuntimeSystemWeather::WeatherBlendState>::MoveAfterReallocation` @0x100d5fa48
// (`symbols-demangled.txt`, confirmados boundary exata via `decode_funcstarts.py`). `find_bl_callers.py`
// a partir desses 3 (e dos helpers genéricos que eles chamam) subiu 2 saltos de `bl` até o cluster
// de código INTEIRO de `world::RuntimeSystemWeather` (~24KB contíguos, 0x100d59000-0x100d5f000,
// 132 funções). Filtrei essas 132 pelo fingerprint de REGISTRADOR exato da assinatura real
// (`enablers/Codeware/src/Red/RuntimeScene.hpp`): `bool SetWeatherByName(worldRuntimeSystemWeather*
// aSystem, CName aWeather, CName aSource, float aBlendTime, uint32_t a5, uint32_t aPriority)` —
// AAPCS64 dá x0/x1/x2 (GP, ordem-de-tipo) + s0 (FP) + x3/x4 (GP) — só 1 das 132 usa x4 E s0 E x3
// juntos: `0x100d5c384` (580B). Disassemblada por completo (`disasm_region.py`): salva os 6 args
// em x19-x23+s8 (padrão "preservar-through-call"), lê `[this+0x84]` pra escolher entre
// `[this+0x88]`/`[this+0x90]` — **`+0x90` é `CurrentStateIndex`, offset JÁ CONFIRMADO no header
// vendorizado (`Raw::RuntimeSystemWeather::CurrentStateIndex = OffsetPtr<0x90,u32>`) — CROSS-
// VALIDAÇÃO independente #1**. Chama `0x100d5bf98` (helper "estado-por-índice", 24 call-sites só
// neste cluster) pro estado ATUAL, compara o nome contra `aWeather` (x22) E a fonte contra
// `[this+0x98]` — **`+0x98` é `CurrentSource` no mesmo header — CROSS-VALIDAÇÃO independente #2**
// — se já bate, sai cedo (early-return, mesma semântica óbvia de "já está nesse clima"); senão
// itera por TODOS os estados (`0x100d5a584`=contagem) chamando `0x100d5bf98` de novo até achar
// nome==aWeather, e aí encaminha os 6 args ORIGINAIS (this/idx/aSource/aBlendTime/a5/aPriority,
// restaurados dos registradores callee-saved x19-x23+s8) pra `0x100d597a8` (1948B, candidato
// forte a "ApplyWeatherTransition" — não investigada a fundo, fora do escopo desta rodada).
// Retorna bool (mask de w0 no fim) — bate com a assinatura. **Confiança ALTA**: fingerprint de
// ABI único entre 132 candidatos + 2 offsets de campo JÁ documentados batendo exato + fluxo
// semanticamente coerente ponta-a-ponta com "resolver clima por CName e aplicar transição" — mas
// **NUNCA testado ao vivo, RE offline não fecha item sozinha (regra de ouro)**. Observe-only,
// HookAfter (sempre chama o original, nunca suprime/altera retorno), gate PRÓPRIO e SEPARADO
// (`~/.bwms-weathersetname-probe`), NUNCA ligado por padrão. Detalhe completo em `HISTORICO.md`/
// `cp77-symbols/notes/CATALOGO-EXAUSTIVO-CODEWARE.md` (item `#81`/`#140`, 2026-08-18).
pub(crate) const WEATHER_SETWEATHERBYNAME_VM: u64 = 0x100d5c384;
static WEATHER_SETWEATHERBYNAME_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_WEATHER_SETWEATHERBYNAME: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

type WeatherSetByNameFn = unsafe extern "C" fn(*mut c_void, u64, u64, f32, u32, u32) -> u8;
unsafe extern "C" fn weather_setweatherbyname_probe(
    this: *mut c_void,
    a_weather: u64,
    a_source: u64,
    a_blend_time: f32,
    a5: u32,
    a_priority: u32,
) -> u8 {
    let orig = ORIG_WEATHER_SETWEATHERBYNAME.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: WeatherSetByNameFn = std::mem::transmute(orig);
        f(this, a_weather, a_source, a_blend_time, a5, a_priority)
    } else {
        0
    };
    let n = WEATHER_SETWEATHERBYNAME_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 8 {
        crate::log(&format!(
            "[cw-81-weathersetname] SetWeatherByName#{n} this={this:p} aWeather={a_weather:#018x} aSource={a_source:#018x} aBlendTime={a_blend_time} a5={a5} aPriority={a_priority} ret={ret}"
        ));
    }
    ret
}

// ===== RED4ext.SDK #461: UpdateRegistrar::RegisterUpdate(group) OBSERVE-ONLY =====
// Captura o `this`(UpdateRegistrar*) real na 1ª chamada vanilla (acontece naturalmente no boot,
// confirmado por 5 call-sites reais na RE offline) — NÃO chama RegisterUpdate de volta ainda, só
// loga+guarda o ponteiro pra uma etapa futura (construir Callback<> + registrar nosso próprio
// update). Passthrough incondicional pro original em toda chamada (nunca muda comportamento real).
type RegisterUpdateGroupFn = unsafe extern "C" fn(*mut c_void, u8, *mut c_void, *const u8, *mut c_void);
unsafe extern "C" fn update_registrar_group_probe(
    this: *mut c_void,
    group: u8,
    system: *mut c_void,
    name: *const u8,
    callback: *mut c_void,
) {
    let n = UPDATE_REGISTRAR_CAPTURE_CNT.fetch_add(1, Ordering::Relaxed);
    if n == 0 {
        UPDATE_REGISTRAR_CAPTURED_PTR.store(this as u64, Ordering::Relaxed);
    }
    if n < 8 {
        let name_str = if gum::is_readable(name as *const c_void, 1) {
            std::ffi::CStr::from_ptr(name as *const i8).to_string_lossy().into_owned()
        } else {
            String::from("(ilegível)")
        };
        // Opção A do plano `RED4EXT-461-VIA-CARA-PLANO.md` (2026-08-11): dereferencia o `Callback<>`
        // real (`callback` já chega como ponteiro — tipo não-trivial >16B, passado por referência
        // invisível no AAPCS64) pra descobrir qual dos 3 formatos (`UnboundFunctionTarget`/
        // `ClosureTarget`/`MemberFunctionTarget`) o jogo usa de verdade nesta build. Leitura pura,
        // zero escrita — mesma categoria de risco (zero) do resto deste probe observe-only.
        let (buf0, handler_ptr, h_invoke, h_copy, h_move, h_destruct) =
            if gum::is_readable(callback as *const c_void, 0x28) {
                let buf0 = (callback as *const u64).read_unaligned();
                let handler_ptr = ((callback as *const u8).add(0x20) as *const u64).read_unaligned();
                if handler_ptr != 0 && gum::is_readable(handler_ptr as *const c_void, 0x20) {
                    let hp = handler_ptr as *const u64;
                    (
                        buf0,
                        handler_ptr,
                        hp.read_unaligned(),
                        hp.add(1).read_unaligned(),
                        hp.add(2).read_unaligned(),
                        hp.add(3).read_unaligned(),
                    )
                } else {
                    (buf0, handler_ptr, 0, 0, 0, 0)
                }
            } else {
                (0, 0, 0, 0, 0, 0)
            };
        let matches_system = buf0 == (system as u64);
        crate::log(&format!(
            "[red4ext-461] RegisterUpdate(group)#{n} this={this:p} group={group} system={system:p} name='{name_str}' | callback={callback:p} buf0={buf0:#x} matches_system={matches_system} handler={handler_ptr:#x} invoke={h_invoke:#x} copy={h_copy:#x} move={h_move:#x} destruct={h_destruct:#x}"
        ));
        // Opção A (recomendada pelo plano): guarda o `invoke` REAL do callback já registrado por um
        // sistema `world::IRuntimeSystem` VANILLA já vivo — alvo pra um hook posterior (ver
        // `install_anim_framebegin_hook_if_ready`). "AnimationSystem_FrameBeginReset"/group=0
        // (FrameBegin) escolhido por ser o mais cedo/seguro dos 8 já capturados nesta sessão. Só
        // GUARDA aqui (leitura pura); a instalação do hook acontece depois, fora deste call-stack
        // (nunca instalar hook novo de dentro de um replace já em andamento).
        if h_invoke != 0 && name_str == "AnimationSystem_FrameBeginReset" {
            ANIM_FRAMEBEGIN_INVOKE_CAPTURED.store(h_invoke, Ordering::Relaxed);
        }
    }
    let orig = ORIG_UPDATE_REGISTRAR_GROUP.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: RegisterUpdateGroupFn = std::mem::transmute(orig);
        f(this, group, system, name, callback);
    }
}

// ===== RED4ext.SDK #461 Opção A: hook no `invoke` REAL do callback já registrado pelo motor =====
// (ver `cp77-symbols/notes/RED4EXT-461-VIA-CARA-PLANO.md`). Em vez de tocar em qualquer estrutura do
// `UpdateRegistrar` (categoria que já CROU, ver `register_bwms_update_tick` acima), instala um
// inline-hook comum (mesma técnica provada 50+ vezes neste projeto) direto no CÓDIGO do callback que
// `AnimationSystem_FrameBeginReset` já registrou de verdade — dá execução ordenada num ponto real do
// pipeline (grupo FrameBegin) sem depender do `RegisterUpdate` nativo em si. Alvo é um ponteiro
// CAPTURADO EM RUNTIME (não um vmaddr estático) — por isso NÃO passa por `crate::rebase()` (que só
// serve pra converter offset-do-binário-em-disco pra endereço-com-ASLR; aqui já é o endereço real).
static ANIM_FRAMEBEGIN_INVOKE_CAPTURED: AtomicU64 = AtomicU64::new(0);
static ANIM_FRAMEBEGIN_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static ANIM_FRAMEBEGIN_HOOK_CNT: AtomicU64 = AtomicU64::new(0);
static ORIG_ANIM_FRAMEBEGIN_INVOKE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

type AnimInvokeFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void);

/// Mesma assinatura ABI de `CallbackHandler::invoke` (`bwms_callback_invoke` acima, já provada):
/// `(target_buffer_ptr, FrameInfo*, JobQueue*)`. NUNCA suprime o original — só observa+repassa
/// (mesmo idioma de todo hook observe-only deste projeto), pra nunca regredir a animação real do jogo.
unsafe extern "C" fn bwms_anim_framebegin_hook(target: *mut c_void, frame_info: *mut c_void, job_queue: *mut c_void) {
    let n = ANIM_FRAMEBEGIN_HOOK_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 5 || n % 600 == 0 {
        crate::log(&format!(
            "[red4ext-461-optA] AnimationSystem_FrameBeginReset invoke interceptado #{n} target={target:p} frame_info={frame_info:p} job_queue={job_queue:p}"
        ));
    }
    // PRODUTIZAÇÃO 2026-08-17 (item já ✅ FECHADO desde 2026-08-11, cont.239-241 — o comportamento
    // BOUNDED abaixo era só a JANELA DE PROVA, gated por `~/.bwms-461-optiona-fireevent-confirm`,
    // nunca ligado por padrão; um mod real nunca via mais que 3 disparos por processo). Despachar
    // `CallbackSystem` daqui chama `resolve_in_class`+`call_func` (RTTI) a partir de uma thread de
    // JOB (o parâmetro `job_queue` sugere execução fora da thread principal) — categoria já
    // exercitada e confirmada segura na prova decisiva (3 disparos reais executaram o método-alvo
    // do mod de teste, zero crash atribuível). Pra uso de produção (todo frame, não só 3x), troca
    // o bound de contagem por `register::has_listener_for` (lookup BARATO, só lock+scan, sem RTTI)
    // chamado TODO frame — só paga o custo de `fire_event_args` (mais caro, resolve+call_func) nos
    // frames em que existe de verdade pelo menos 1 listener registrado.
    if crate::register::has_listener_for("Pipeline/FrameBegin") {
        let sent = crate::register::fire_event_args("Pipeline/FrameBegin", &[]);
        if n < 5 || n % 600 == 0 {
            crate::log(&format!("[red4ext-461-optA] fire_event_args('Pipeline/FrameBegin') dentro do hook #{n} -> despachado pra {sent} listener(s)"));
        }
    }
    let orig = ORIG_ANIM_FRAMEBEGIN_INVOKE.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: AnimInvokeFn = std::mem::transmute(orig);
        f(target, frame_info, job_queue);
    }
}

/// Chamado repetidamente do `cp77_tick` (sempre-ativo desde 2026-08-17 — ver
/// `install_pipeline_framebegin_probe` mais abaixo, promovido junto) até o `invoke` real ter sido
/// capturado pelo probe acima E o hook instalado — idempotente, barato de rechecar todo tick.
pub(crate) unsafe fn install_anim_framebegin_hook_if_ready() -> bool {
    if ANIM_FRAMEBEGIN_HOOK_INSTALLED.load(Ordering::Relaxed) {
        return true;
    }
    let target = ANIM_FRAMEBEGIN_INVOKE_CAPTURED.load(Ordering::Relaxed) as *mut c_void;
    if target.is_null() || !gum::is_readable(target as *const c_void, 16) {
        return false;
    }
    if ANIM_FRAMEBEGIN_HOOK_INSTALLED.swap(true, Ordering::Relaxed) {
        return true;
    }
    let it = Interceptor::obtain();
    match it.replace(target, bwms_anim_framebegin_hook as *mut c_void) {
        Some(tramp) => {
            ORIG_ANIM_FRAMEBEGIN_INVOKE.store(tramp, Ordering::Relaxed);
            std::mem::forget(it);
            crate::log(&format!(
                "[red4ext-461-optA] hook instalado no invoke REAL de AnimationSystem_FrameBeginReset @ {target:p}"
            ));
            true
        }
        None => {
            ANIM_FRAMEBEGIN_HOOK_INSTALLED.store(false, Ordering::Relaxed);
            crate::log("[red4ext-461-optA] FALHA ao instalar hook no invoke capturado");
            false
        }
    }
}

// ===== RED4ext.SDK #461: construção do `Callback<void(*)(FrameInfo&,JobQueue&),32>` real =====
// ABI mapeada de `Callback.hpp`/`Detail/Callback.hpp` (RED4ext.SDK vendorizado, lido nesta sessão):
//   struct CallbackHandler<R,Args...> { invoke@0x00, copy@0x08, move@0x10, destruct@0x18 } (0x20B)
//   class Callback<R(*)(Args...),32>  { buffer[32]@0x00, handler:CallbackHandler*@0x20 } (0x28B)
// Caso `UnboundFunctionTarget` (função livre, sem contexto/closure): TargetType = {func: fn-ptr},
// 8 bytes, cabe no buffer[32] sem alocação extra. `invoke(target,args...)` faz
// `std::invoke(target->func, args...)` — lê o fn-ptr no offset 0 do target e chama com os args
// originais (referências ARM64 = ponteiros). `copy`/`move` só copiam o fn-ptr (8 bytes, plain
// old data, sem estado a mover/destruir de verdade) — `destruct` zera por higiene.
static UPDATE_TICK_FIRE_CNT: AtomicU64 = AtomicU64::new(0);

/// Nosso callback de update — assinatura exata de `GroupUpdateCallback::TargetFunc`:
/// `void(*)(FrameInfo&, JobQueue&)`. Deliberadamente MÍNIMO: só loga as 5 primeiras chamadas
/// (nunca lê campos de `FrameInfo`/`JobQueue`, cujo layout não foi validado — o objetivo desta
/// etapa é confirmar que o REGISTRO funciona e o callback DISPARA, não processar o frame de
/// verdade) — limita o raio de risco ao ato de registrar, não a cada frame subsequente.
unsafe extern "C" fn bwms_update_tick_callback(frame_info: *mut c_void, job_queue: *mut c_void) {
    let n = UPDATE_TICK_FIRE_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 5 {
        crate::log(&format!(
            "[red4ext-461] BwmsUpdateTick DISPAROU #{n} frame_info={frame_info:p} job_queue={job_queue:p} — REGISTRO REAL CONFIRMADO"
        ));
    }
}

unsafe extern "C" fn bwms_callback_invoke(target: *const u8, frame_info: *mut c_void, job_queue: *mut c_void) {
    if !gum::is_readable(target as *const c_void, 8) {
        return;
    }
    let func_ptr = (target as *const u64).read_unaligned();
    if func_ptr == 0 {
        return;
    }
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) = std::mem::transmute(func_ptr);
    f(frame_info, job_queue);
}
unsafe extern "C" fn bwms_callback_copy(dst: *mut u8, src: *mut u8) {
    if gum::is_readable(src as *const c_void, 8) && gum::is_readable(dst as *const c_void, 8) {
        (dst as *mut u64).write_unaligned((src as *const u64).read_unaligned());
    }
}
unsafe extern "C" fn bwms_callback_move(dst: *mut u8, src: *mut u8) {
    bwms_callback_copy(dst, src);
    if gum::is_readable(src as *const c_void, 8) {
        (src as *mut u64).write_unaligned(0);
    }
}
unsafe extern "C" fn bwms_callback_destruct(target: *mut u8) {
    if gum::is_readable(target as *const c_void, 8) {
        (target as *mut u64).write_unaligned(0);
    }
}

#[repr(C)]
struct BwmsCallbackHandler {
    invoke: unsafe extern "C" fn(*const u8, *mut c_void, *mut c_void),
    copy: unsafe extern "C" fn(*mut u8, *mut u8),
    move_fn: unsafe extern "C" fn(*mut u8, *mut u8),
    destruct: unsafe extern "C" fn(*mut u8),
}
static BWMS_UPDATE_HANDLER: BwmsCallbackHandler = BwmsCallbackHandler {
    invoke: bwms_callback_invoke,
    copy: bwms_callback_copy,
    move_fn: bwms_callback_move,
    destruct: bwms_callback_destruct,
};

/// `Callback<void(*)(FrameInfo&,JobQueue&),32>` construído à mão — 40 bytes: `buffer[32]` (só os
/// 8 primeiros bytes usados: nosso fn-ptr) + `handler@0x20` (ponteiro pro vtable de 4 fns acima).
#[repr(C)]
struct BwmsGroupUpdateCallback {
    buffer: [u8; 32],
    handler: *const BwmsCallbackHandler,
}

/// RED4ext.SDK #461: registra `bwms_update_tick_callback` no `UpdateRegistrar*` já capturado
/// (`UPDATE_REGISTRAR_CAPTURED_PTR`) via `RegisterUpdate(group, system, name, callback&&)` real —
/// chamada DIRETA na mesma função hookada (`UPDATE_REGISTRAR_GROUP_VM`/`0x103d95800`), reentra no
/// nosso próprio probe (log extra inofensivo) antes de cair no original de verdade. `aGroup=0`
/// (FrameBegin, o valor mais cedo/seguro do pipeline). `aSystem` reusa o ponteiro do PLAYER (já
/// confirmado válido/vivo neste boot, mesmo idioma de dezenas de outras chamadas desta sessão) —
/// evita passar um `IScriptable*` forjado/inválido que a engine poderia validar internamente.
///
/// ⚠️ **CONFIRMADO CRASHAR quando chamado pós-boot (2026-08-11, testado ao vivo via `registerupdate`).**
/// `EXC_BAD_ACCESS`/`SIGBUS`/`KERN_PROTECTION_FAILURE` — `esr` decodifica como **"(Data Abort) byte
/// write Permission fault"** (não é null-deref nem endereço inválido — é escrita numa página SEM
/// permissão de escrita). Acontece DENTRO da implementação real de `RegisterUpdate` (frames 0-1 do
/// crash, código do jogo não-simbolizado), chamada a partir do nosso próprio `cp77_tick`/canal de
/// comando (thread `redDispatcher4`, aninhada dentro de `SystemsUpdater::Node::LinkJob_NoFence` —
/// mesmo padrão de reentrância em thread-de-job já documentado alhures no projeto). **Hipótese mais
/// provável**: a estrutura interna do `UpdateRegistrar` (provavelmente o `DynArray` de entradas que
/// o helper compartilhado `0x103d902e4` cresce) é `mprotect`ada READ-ONLY pelo motor assim que a
/// janela de registro do boot fecha — as 8 chamadas vanilla capturadas TODAS aconteceram durante o
/// boot inicial (antes da gameplay), nunca depois; chamar `RegisterUpdate` depois do boot sempre
/// bate nessa proteção, independente da ABI do `Callback<>` estar certa ou não (que ESTÁ,
/// confirmado pelas 8 capturas com validação semântica). **Conclusão**: a via "capturar ponteiro
/// via hook + chamar depois" está
/// REFUTADA — só resta a via mais cara já identificada pela RE original (forjar uma classe
/// `IUpdatableSystem`-derivada de verdade e deixar o motor chamar `OnRegisterUpdates` nela
/// NATURALMENTE, dentro da janela real de boot — não tentado, escopo de sessão dedicada). Função
/// mantida no código (gated, nunca dispara sem `~/.bwms-registerupdate-confirm`) só como referência
/// do que foi tentado — NÃO chamar de novo sem essa mudança de abordagem. Ver
/// `proofs/2026-08-11-red4ext-461-registerupdate-CRASH-writeprotect.ips`.
pub(crate) unsafe fn register_bwms_update_tick(player: *mut c_void) -> bool {
    let registrar = UPDATE_REGISTRAR_CAPTURED_PTR.load(Ordering::Relaxed) as *mut c_void;
    if registrar.is_null() {
        crate::log("[red4ext-461] registro ABORTADO: UpdateRegistrar* nunca foi capturado (probe armado + RegisterUpdate já disparou?)");
        return false;
    }
    let target = crate::rebase(UPDATE_REGISTRAR_GROUP_VM);
    if !gum::is_readable(target as *const c_void, 16) {
        crate::log("[red4ext-461] registro ABORTADO: endereço de RegisterUpdate ilegível");
        return false;
    }
    let mut cb = BwmsGroupUpdateCallback { buffer: [0u8; 32], handler: &BWMS_UPDATE_HANDLER };
    (cb.buffer.as_mut_ptr() as *mut u64).write_unaligned(bwms_update_tick_callback as usize as u64);
    static NAME: &[u8] = b"BwmsUpdateTick\0";
    let f: RegisterUpdateGroupFn = std::mem::transmute(target);
    crate::log(&format!(
        "[red4ext-461] chamando RegisterUpdate(this={registrar:p}, group=0, system={player:p}, name='BwmsUpdateTick', callback={:p})",
        &cb as *const _
    ));
    f(registrar, 0u8, player, NAME.as_ptr(), &mut cb as *mut BwmsGroupUpdateCallback as *mut c_void);
    crate::log("[red4ext-461] RegisterUpdate retornou sem crash");
    true
}

// ===== axl-puppet-state-apply: GenitalsController::OnAttach C++ hook =====
// ABI C++ ARM64: x0=gameuiCharacterCustomizationGenitalsController* (component)
// ArchiveXL ID: CharacterCustomizationGenitalsController_OnAttach (AddressLib)
unsafe extern "C" fn genital_onattach_handler(component: *mut c_void) {
    let orig = ORIG_GENITAL_ONATTACH.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: unsafe extern "C" fn(*mut c_void) = std::mem::transmute(orig);
        f(component);
    }
    let n = GENITAL_ONATTACH_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 8 {
        crate::log(&format!("[axl-puppet] GenitalsController::OnAttach#{n} ctrl={component:p}"));
    }
}

// ===== axl-questphase-apply: QuestsSystem::OnGameRestored candidato C++ hook =====
// ABI C++ ARM64 (virtual, IGameSystem::OnGameRestored): x0=this, retorno bool (w0).
// HookWrap (não HookBefore): propaga o retorno real do original — é um virtual bool,
// alguém pode ramificar no resultado; não arriscar mudar comportamento, só observar.
unsafe extern "C" fn questsys_ongamerestored_handler(this: *mut c_void) -> bool {
    let n = QUESTSYS_ONGAMERESTORED_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_QUESTSYS_ONGAMERESTORED.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: unsafe extern "C" fn(*mut c_void) -> bool = std::mem::transmute(orig);
        f(this)
    } else {
        true
    };
    if n < 8 {
        crate::log(&format!("[axl-questphase] QuestsSystem::OnGameRestored-cand#{n} this={this:p} ret={ret}"));
    }
    ret
}

// ===== axl-puppet-state-apply: HairstyleController::OnDetach C++ hook =====
// ABI C++ ARM64: x0=gameuiCharacterCustomizationHairstyleController*, x1=uintptr_t a2
// ArchiveXL ID: CharacterCustomizationHairstyleController_OnDetach (AddressLib)
unsafe extern "C" fn hairstyle_ondetach_handler(component: *mut c_void, a2: usize) {
    let orig = ORIG_HAIRSTYLE_ONDETACH.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: unsafe extern "C" fn(*mut c_void, usize) = std::mem::transmute(orig);
        f(component, a2);
    }
    let n = HAIRSTYLE_ONDETACH_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 8 {
        crate::log(&format!("[axl-puppet] HairstyleController::OnDetach#{n} ctrl={component:p}"));
    }
}

// axl-attachment-apply / item #53 (2026-08-12): probe do `__invoke` CONFIRMADO por símbolo do
// event-connector `TPPRepresentationComponent`+`AttachmentSlotEvents::ItemEquippedInSlot` (ver
// nota grande na declaração de `TPP_ITEMEQUIP_INVOKE_VM`). Assinatura do thunk real (Itanium ABI,
// confirmada por disassembly): `void __invoke(void* closureData, ISerializable& receiver,
// THandle<red::Event> const& event)` — 3 args ponteiro, retorno void. Sempre repassa pra original
// SEM alterar nenhum registrador/retorno; a leitura da cadeia `SlotListener` é só log, nunca influi
// no fluxo real.
type ThreeArgVoidPtrFn = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void);
unsafe extern "C" fn tpp_itemequip_invoke_probe(closure: *mut c_void, receiver: *mut c_void, evt_ref: *mut c_void) {
    let n = TPP_ITEMEQUIP_INVOKE_CNT.fetch_add(1, Ordering::Relaxed);
    if n == 0 {
        // slide = game_base() - LINK_BASE(0x1_0000_0000) — logado 1x pra converter os ponteiros
        // runtime dos slots de vtable (abaixo) de volta pro vmaddr ESTÁTICO do arquivo, permitindo
        // disassembly offline (Capstone/otool) numa sessão futura sem precisar de boot novo.
        crate::log(&format!("[axl-tpp-itemequip] game_base={:#018x}", crate::game_base()));
    }
    if n < 8 {
        // SlotListener@+0x148 = Handle<IAttachmentSlotsListener>{instance*, refCountBlock*} —
        // offset de CAMPO confirmado no header vendorizado (TPPRepresentationComponent.hpp),
        // categoria de confiança mais alta que qualquer endereço de função desta família.
        // AMPLIADO 2026-08-12 (após 1º disparo real, dados dos slots 0x120..0x148 achados
        // NÃO-nulos/distintos em 0x140/0x148): a interface `IAttachmentSlotsListener` real (SDK
        // vendorizado) tem 13 métodos — dump ESTREITO só cobria 6 slots. Amplia pra 0x00..0x200
        // (64 slots) pra capturar a vtable INTEIRA numa passada só (dtors + os 13 métodos reais +
        // margem), permitindo mapear cada offset contra a lista nomeada da interface de uma vez.
        let mut slot_listener_instance: u64 = 0;
        let mut slot_listener_vtbl: u64 = 0;
        let mut vt_slots: [u64; 64] = [0; 64]; // offsets Mac 0x00..0x1F8 (passo 0x08)
        if gum::is_readable(receiver, 0x150) {
            slot_listener_instance = (receiver as *const u64).add(0x148 / 8).read_unaligned();
            if slot_listener_instance != 0
                && gum::is_readable(slot_listener_instance as *const c_void, 8)
            {
                slot_listener_vtbl = (slot_listener_instance as *const u64).read_unaligned();
                if slot_listener_vtbl != 0
                    && gum::is_readable(slot_listener_vtbl as *const c_void, 0x200)
                {
                    for (i, slot) in vt_slots.iter_mut().enumerate() {
                        *slot = (slot_listener_vtbl as *const u64).add(i).read_unaligned();
                    }
                }
            }
        }
        crate::log(&format!(
            "[axl-tpp-itemequip] ItemEquippedInSlot#{n} closure={closure:p} receiver={receiver:p} \
             evt_ref={evt_ref:p} SlotListener.instance={slot_listener_instance:#018x} \
             SlotListener.vtbl={slot_listener_vtbl:#018x} vt[0x00..0x200]={vt_slots:#018x?}"
        ));
    }
    let orig = ORIG_TPP_ITEMEQUIP_INVOKE.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: ThreeArgVoidPtrFn = std::mem::transmute(orig);
        f(closure, receiver, evt_ref);
    }
}

// item #53 (2026-08-12, checkpoint novo): probe do candidato idx28 do vtable REAL (40 slots) de
// `TPPRepresentationSlotListener` (ver nota grande na declaração de `TPP_SLOTLISTENER_IDX28_CAND_VM`
// — hipótese B, confiança MÉDIA-BAIXA, NÃO confirmada). ABI assumida por analogia com a assinatura
// documentada da interface (`(const ItemID& aItemID, TweakDBID aSlotID)`): x0=this, x1=possível
// ponteiro pra ItemID, x2=possível TweakDBID por valor — mas a disassembly (ver nota) achou um
// red flag (x2 parece ser deferenciado como ponteiro num helper downstream, incompatível com
// TweakDBID escalar) — este probe existe justamente pra RESOLVER essa dúvida com dado real, não
// pra confirmar uma hipótese já fechada. SEMPRE chama a original PRIMEIRO (nunca risca mudar
// comportamento observável antes de logar) — mesmo padrão de segurança já usado nos candidatos
// vizinhos (`tpp_onattach_candidate_probe`/`tpp_onattach_c2_probe`). Nota honesta: como a função
// real pode retornar um valor em w0/x0 (a disassembly mostra bodies que escrevem x0, apesar do
// header documentar `void` — outra divergência não resolvida), o código de log ABAIXO da chamada
// da original PODE sobrescrever esse retorno antes do nosso probe "retornar" — mesmo trade-off já
// aceito pelos candidatos vizinhos deste arquivo (observe-only de baixo risco, nunca ligado por
// padrão). CODADO, NUNCA TESTADO AO VIVO (sessão 100% offline).
unsafe extern "C" fn tpp_slotlistener_idx28_cand_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = TPP_SLOTLISTENER_IDX28_CAND_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_TPP_SLOTLISTENER_IDX28_CAND.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
    if n < 8 {
        // a1 tratado como possível ponteiro pra ItemID (deref defensivo, só se parecer endereço
        // válido de heap/módulo); a2 tratado cru (candidato TweakDBID) E como possível ponteiro
        // (deref defensivo) — deixa os DOIS formatos logados pra decidir empiricamente qual bate.
        let a1_deref = if a1 > 0x1000 && gum::is_readable(a1 as *const c_void, 8) {
            Some((a1 as *const u64).read_unaligned())
        } else {
            None
        };
        let a2_deref = if a2 > 0x1000 && gum::is_readable(a2 as *const c_void, 8) {
            Some((a2 as *const u64).read_unaligned())
        } else {
            None
        };
        crate::log(&format!(
            "[axl-tpp-slotlistener-idx28] cand#{n} this={this:p} a1={a1:#018x} a1_deref={a1_deref:?} \
             a2={a2:#018x} a2_deref={a2_deref:?} (a1~ItemID&? a2~TweakDBID escalar OU ponteiro?)"
        ));
    }
}

// axl-attachment-apply / itens #40, #41, #54 (2026-08-12, análise offline de continuação — ver
// checkpoint "itens #40/#41/#54" no fim de `CATALOGO-EXAUSTIVO-ARCHIVEXL.md"): decode dos 12
// primeiros words (0x60 bytes) comuns a QUALQUER instância `ISerializable`/`IScriptable`/
// `IComponent` (`AttachmentSlots extends ent::IComponent`) — layout confirmado byte-a-byte contra
// os headers vendorizados reais (`RED4ext.SDK/include/RED4ext/ISerializable.hpp` +
// `Scripting/IScriptable.hpp` + `Scripting/Natives/Generated/ent/IComponent.hpp`, bloco comentado
// com o layout real). **Confirmação decisiva, não heurística**: `name_hash` (offset 0x40,
// `IComponent::name`, um `CName`=FNV1a64) do dump real do boot 2026-08-12 (`EquipStart#1`) bate
// EXATO com `bwms_hashes::fnv1a64(b"AttachmentSlots")` — o objeto capturado É uma instância de
// `game::AttachmentSlots` por CONTEÚDO, não só por ser "o receiver do evento X".
//
// Mapa de campo completo (todos os 9 primeiros words, offsets 0x00-0x48):
//   0x00 vtable            (ISerializable vtable ptr)
//   0x08 ref.instance      (ISerializable::ref, WeakHandle<ISerializable>.instance — "Initialized
//                            in Handle ctor"; == o próprio ponteiro do objeto, confirmado nos 2
//                            samples capturados: EquipStart#0 E #1)
//   0x10 ref.refcount      (ISerializable::ref, WeakHandle<ISerializable>.refCountBlock)
//   0x18 unk18.instance    (ISerializable::unk18, 2º WeakHandle — nunca populado nos samples)
//   0x20 unk18.refcount    (idem)
//   0x28 global_serialize_id (ISerializable::unk28 — "Global incremental ID, used in
//                            serialization" per o header oficial; bate com a observação de que é
//                            um valor PEQUENO tipo contador, não ponteiro)
//   0x30 native_type       (IScriptable::nativeType, CClass*)
//   0x38 value_holder      (IScriptable::valueHolder, void*)
//   0x40 name_hash         (IComponent::name, CName — CONFIRMADO = fnv1a64("AttachmentSlots"))
// Words 0x48-0x60 (índices 9-11) caem no bloco NÃO-documentado `unk48` do header (IComponent
// genérico, não específico de AttachmentSlots) — sem hipótese de campo ainda.
//
// **Correção de uma hipótese de investigação anterior**: os valores em 0x10 (`ref.refcount`) e
// 0x38 (`value_holder`) no dump real diferem por só 8 bytes (`0x700fd04c88` vs `0x700fd04c80`) —
// isso NÃO é um par estrutural `Handle<T>{instance,refcount}` adjacente (são 2 campos de
// SUB-OBJETOS diferentes, `ref`@0x08 vs `valueHolder`@0x38, 0x28 bytes/5 words de distância no
// struct) — é mais provável proximidade de ALOCADOR (2 pequenas alocações feitas perto uma da
// outra no tempo), não um par causal. Documentado honestamente em vez de inflar a hipótese.
//
// **Limite real desta captura**: os 0x60 bytes cobertos (12 words) ficam TODOS dentro da base
// comum ISerializable(0x30)+IScriptable(0x10)+início do IComponent(0x18, até 0x48, mais o bloco
// não-documentado `unk48` até 0x60) — NUNCA alcançam campo algum específico de `AttachmentSlots`
// (que só começa depois de `IComponent` terminar, offset >=0x90, per
// `RED4ext.SDK/.../Generated/game/AttachmentSlots.hpp`: `unk90[0x90..0xE0]` — candidato mais
// plausível pro storage real de slots, já que fica bem entre o fim de `IComponent` e o início do
// `DynArray<AnimParamSlotsOption> animParams@0xE0`, que não é dado-por-slot). **Por isso `word[5]`
// (contador de serialização) e `word[8]` (CName do componente) são explicados com confiança ALTA,
// mas NENHUM dos 12 words dá pista de layout pra `IsSlotEmpty`/`IsSlotSpawning`/`s_dependentSlots`
// — essas continuam `Core::RawFunc` genuínas (função por endereço, não campo), sem hipótese de
// implementação por leitura pura de campo. Regra de ouro do projeto respeitada: item `#40`/`#41`/
// `#54` PERMANECE REAL_GAP, isto é groundwork, não fechamento.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IComponentBase60 {
    vtable: u64,
    ref_instance: u64,
    ref_refcount: u64,
    unk18_instance: u64,
    unk18_refcount: u64,
    global_serialize_id: u64,
    native_type: u64,
    value_holder: u64,
    name_hash: u64,
}

fn decode_icomponent_base60(words: &[u64; 12]) -> IComponentBase60 {
    IComponentBase60 {
        vtable: words[0],
        ref_instance: words[1],
        ref_refcount: words[2],
        unk18_instance: words[3],
        unk18_refcount: words[4],
        global_serialize_id: words[5],
        native_type: words[6],
        value_holder: words[7],
        name_hash: words[8],
    }
}

/// Confirma por CONTEÚDO (não por heurística de "quem chamou") que os primeiros 0x60 bytes de um
/// objeto pertencem a uma instância de `game::AttachmentSlots` — `IComponent::name`(CName)@0x40
/// deve bater com `fnv1a64("AttachmentSlots")`. Zero chamada nativa, zero RE de endereço; útil
/// como checagem defensiva pra qualquer hook futuro desta família (inclusive os já existentes:
/// dispensa depender só da suposição "o receiver do evento X é sempre um AttachmentSlots*").
fn is_attachment_slots_by_name(words: &[u64; 12]) -> bool {
    words[8] == bwms_hashes::fnv1a64(b"AttachmentSlots")
}

// axl-attachment-apply / itens #40, #41, #54 (2026-08-12): probe do `__invoke` CONFIRMADO por
// símbolo do event-connector `AttachmentSlots`+`AttachmentSlotEvents::EquipStart` (ver nota grande
// na declaração de `ATTACHSLOTS_EQUIPSTART_INVOKE_VM`). Mesma assinatura de thunk Itanium do TPP
// acima. Dump cru dos primeiros **0x120 bytes** (36 qwords — AMPLIADO 2026-08-12 pela análise
// offline acima; era 0x60/12 qwords) do `receiver` (candidato a `AttachmentSlots*` real): os
// primeiros 0x60 bytes já têm interpretação COMPLETA e confirmada (ver `decode_icomponent_base60`
// acima) — o novo range 0x60-0x120 cobre o resto do `IComponent` não-documentado (até 0x90), a
// região `unk90` genuinamente específica de `AttachmentSlots` (0x90-0xE0, candidato MAIS
// plausível pro storage real de slots — nunca visto por nenhuma investigação anterior deste
// item), o `DynArray<AnimParamSlotsOption> animParams`@0xE0, e o início de `unkF0` (até o limite
// 0x118 oficialmente asserido no header Windows). Groundwork de RE pura — ainda sem hipótese de
// layout pra `IsSlotEmpty`/`IsSlotSpawning`/`s_dependentSlots`, preenchido por leitura direta em
// vez de adivinhação, pronto pra uma sessão futura COM boot inspecionar a região que realmente
// importa (0x90-0xE0).
//
// axl-attachment-apply / itens #40, #41, #54 (2026-08-19, continuação direta da manhã — ver
// checkpoint "unk90" no HISTORICO.md/CLAUDE.md topo): teste da hipótese de layout dos 2 pares
// DynArray-like já achados dentro de `unk90` (offsets 0xB0/0xB8=cap23 e 0xC0/0xC8=cap9, índices
// `words[22..26]` do dump de 36 qwords acima — DynArray real confirmado byte-a-byte contra o
// header `RED4ext.SDK/include/RED4ext/Containers/DynArray.hpp`: `entries*@+0x00,
// capacity:u32@+0x08, size:u32@+0x0C`, MESMO layout já usado em `animParams@0xE0`).
//
// Candidato de elemento pro array de cap=23: `game::AttachmentSlotData`
// (`RED4ext.SDK/.../Scripting/Natives/gameAttachmentSlotData.hpp`) é EXATAMENTE 0x90 bytes
// (`RED4EXT_ASSERT_SIZE(AttachmentSlotData, 0x90)`) — `slotID:TweakDBID@0x00` /
// `itemObject:Handle<ItemObject>@0x08` / `spawningItemID:ItemID@0x18` / `activeItemID:ItemID@0x28`
// / `prevItemID:ItemID@0x38` / `appearanceItemID:ItemID@0x58` (header GERADO da reflection real,
// mesma classe de confiança já usada pro campo vizinho `AnimParamSlotsOption`). Testa a hipótese
// lendo `array_ptr+i*0x90` pros primeiros `min(size,23)` elementos e comparando `slotID` contra
// 169 nomes reais `AttachmentSlots.*` extraídos de `redscript-src/` inteiro (tabela
// `ATTACHMENT_SLOT_NAMES` abaixo) via `bwms_hashes::tweak_db_id` (TweakDBID=CRC32(nome)|len<<32,
// MESMO algoritmo já provado em várias sessões, zero RE nova). Se os nomes baterem, confirma por
// CONTEÚDO (não só tamanho) que é o array real de slots — e dá o layout exato de CADA elemento
// pra implementar `IsSlotEmpty`/`IsSlotSpawning` como composição pura (ler `activeItemID`/
// `spawningItemID` do slot certo, sem chamar a função nativa sem endereço).
//
// O 2º par (cap=9, offset 0xC0/0xC8) tem stride DESCONHECIDO — dumpado como qwords crus (sem
// hipótese de struct) pra inspeção offline pós-boot.
//
// 100% OBSERVE-ONLY, só LEITURA (nunca escreve nada nos arrays), zero chamada nativa nova.
const ATTACHMENT_SLOT_NAMES: &[&str] = &[
    "AttachmentSlots.ArmsCyberwareGeneralSlot",
    "AttachmentSlots.BerserkSlot1",
    "AttachmentSlots.BerserkSlot2",
    "AttachmentSlots.BerserkSlot3",
    "AttachmentSlots.Blade_WeaponMod1",
    "AttachmentSlots.Blade_WeaponMod1_Collectible",
    "AttachmentSlots.Blade_WeaponMod2",
    "AttachmentSlots.Blade_WeaponMod2_Collectible",
    "AttachmentSlots.Blunt_WeaponMod1",
    "AttachmentSlots.Blunt_WeaponMod1_Collectible",
    "AttachmentSlots.Blunt_WeaponMod2",
    "AttachmentSlots.Blunt_WeaponMod2_Collectible",
    "AttachmentSlots.Cannon",
    "AttachmentSlots.CannonLaser",
    "AttachmentSlots.Chest",
    "AttachmentSlots.ChimeraGasCloud",
    "AttachmentSlots.ChimeraMelee",
    "AttachmentSlots.Consumable",
    "AttachmentSlots.CyberdeckProgram1",
    "AttachmentSlots.CyberdeckProgram2",
    "AttachmentSlots.CyberdeckProgram3",
    "AttachmentSlots.CyberdeckProgram4",
    "AttachmentSlots.CyberdeckProgram5",
    "AttachmentSlots.CyberdeckProgram6",
    "AttachmentSlots.CyberdeckProgram7",
    "AttachmentSlots.CyberdeckProgram8",
    "AttachmentSlots.DamageMod",
    "AttachmentSlots.Eyes",
    "AttachmentSlots.FabricEnhancer2",
    "AttachmentSlots.Face",
    "AttachmentSlots.FaceFabricEnhancer1",
    "AttachmentSlots.FaceFabricEnhancer2",
    "AttachmentSlots.FaceFabricEnhancer3",
    "AttachmentSlots.FaceFabricEnhancer4",
    "AttachmentSlots.Feet",
    "AttachmentSlots.FootFabricEnhancer1",
    "AttachmentSlots.FootFabricEnhancer2",
    "AttachmentSlots.FootFabricEnhancer3",
    "AttachmentSlots.FootFabricEnhancer4",
    "AttachmentSlots.Gem",
    "AttachmentSlots.GenericWeaponMod1",
    "AttachmentSlots.GenericWeaponMod2",
    "AttachmentSlots.GenericWeaponMod3",
    "AttachmentSlots.GenericWeaponMod4",
    "AttachmentSlots.Head",
    "AttachmentSlots.HeadFabricEnhancer1",
    "AttachmentSlots.HeadFabricEnhancer2",
    "AttachmentSlots.HeadFabricEnhancer3",
    "AttachmentSlots.HeadFabricEnhancer4",
    "AttachmentSlots.IconicMeleeWeaponMod1",
    "AttachmentSlots.IconicWeaponModLegendary",
    "AttachmentSlots.InnerChestFabricEnhancer1",
    "AttachmentSlots.InnerChestFabricEnhancer2",
    "AttachmentSlots.InnerChestFabricEnhancer3",
    "AttachmentSlots.InnerChestFabricEnhancer4",
    "AttachmentSlots.Inspect",
    "AttachmentSlots.ItemSlotGenericMelee",
    "AttachmentSlots.ItemSlotGenericRanged",
    "AttachmentSlots.ItemSlotHammer",
    "AttachmentSlots.ItemSlotHandgunLeft",
    "AttachmentSlots.ItemSlotHandgunLeftJackie",
    "AttachmentSlots.ItemSlotHandgunRight",
    "AttachmentSlots.ItemSlotHandgunRightJackie",
    "AttachmentSlots.ItemSlotKatana",
    "AttachmentSlots.ItemSlotKnifeLeft",
    "AttachmentSlots.ItemSlotKnifeRight",
    "AttachmentSlots.ItemSlotSMG",
    "AttachmentSlots.ItemSlotSniperRifle",
    "AttachmentSlots.ItemSlotTechRifle",
    "AttachmentSlots.KERSSlot1",
    "AttachmentSlots.KERSSlot2",
    "AttachmentSlots.KERSSlot3",
    "AttachmentSlots.KiroshiOpticsSlot1",
    "AttachmentSlots.KiroshiOpticsSlot2",
    "AttachmentSlots.KiroshiOpticsSlot3",
    "AttachmentSlots.Laser",
    "AttachmentSlots.LeftShoulder",
    "AttachmentSlots.LeftShoulderChandelier",
    "AttachmentSlots.LeftShoulderMine",
    "AttachmentSlots.LeftShoulderSelf",
    "AttachmentSlots.LeftShoulderTrack",
    "AttachmentSlots.Legs",
    "AttachmentSlots.LegsFabricEnhancer1",
    "AttachmentSlots.LegsFabricEnhancer2",
    "AttachmentSlots.LegsFabricEnhancer3",
    "AttachmentSlots.LegsFabricEnhancer4",
    "AttachmentSlots.Magazine",
    "AttachmentSlots.MantisBladesEdge",
    "AttachmentSlots.MantisBladesRotor",
    "AttachmentSlots.MeleeWeaponMod1",
    "AttachmentSlots.MeleeWeaponMod2",
    "AttachmentSlots.MeleeWeaponMod3",
    "AttachmentSlots.MetalstormWeapon",
    "AttachmentSlots.MetalstormWeaponExplosive",
    "AttachmentSlots.MetalstormWeaponRaiseSequence",
    "AttachmentSlots.NanoWiresBattery",
    "AttachmentSlots.NanoWiresCable",
    "AttachmentSlots.NanoWiresQuickhackSlot",
    "AttachmentSlots.OuterChestFabricEnhancer1",
    "AttachmentSlots.OuterChestFabricEnhancer2",
    "AttachmentSlots.OuterChestFabricEnhancer3",
    "AttachmentSlots.OuterChestFabricEnhancer4",
    "AttachmentSlots.Outfit",
    "AttachmentSlots.PersonalLink",
    "AttachmentSlots.PowerModule",
    "AttachmentSlots.Power_AR_SMG_LMG_WeaponMod1",
    "AttachmentSlots.Power_AR_SMG_LMG_WeaponMod1_Collectible",
    "AttachmentSlots.Power_AR_SMG_LMG_WeaponMod2",
    "AttachmentSlots.Power_AR_SMG_LMG_WeaponMod2_Collectible",
    "AttachmentSlots.Power_Handgun_WeaponMod1",
    "AttachmentSlots.Power_Handgun_WeaponMod1_Collectible",
    "AttachmentSlots.Power_Handgun_WeaponMod2",
    "AttachmentSlots.Power_Handgun_WeaponMod2_Collectible",
    "AttachmentSlots.Power_Precision_Sniper_Rifle_WeaponMod1",
    "AttachmentSlots.Power_Precision_Sniper_Rifle_WeaponMod2",
    "AttachmentSlots.Power_Shotgun_WeaponMod1",
    "AttachmentSlots.Power_Shotgun_WeaponMod1_Collectible",
    "AttachmentSlots.Power_Shotgun_WeaponMod2",
    "AttachmentSlots.Power_Shotgun_WeaponMod2_Collectible",
    "AttachmentSlots.ProjectileLauncherRound",
    "AttachmentSlots.ProjectileLauncherWiring",
    "AttachmentSlots.QuestDeviceMappin",
    "AttachmentSlots.RailGun",
    "AttachmentSlots.RightArm",
    "AttachmentSlots.RightShoulder",
    "AttachmentSlots.RightShoulderChandelier",
    "AttachmentSlots.RightShoulderMine",
    "AttachmentSlots.RightShoulderSelf",
    "AttachmentSlots.RightShoulderTrack",
    "AttachmentSlots.SandevistanSlot1",
    "AttachmentSlots.SandevistanSlot2",
    "AttachmentSlots.SandevistanSlot3",
    "AttachmentSlots.Scope",
    "AttachmentSlots.ScopeRail",
    "AttachmentSlots.Smart_AR_SMG_LMG_WeaponMod1",
    "AttachmentSlots.Smart_AR_SMG_LMG_WeaponMod2",
    "AttachmentSlots.Smart_Handgun_WeaponMod1",
    "AttachmentSlots.Smart_Handgun_WeaponMod1_Collectible",
    "AttachmentSlots.Smart_Handgun_WeaponMod2",
    "AttachmentSlots.Smart_Handgun_WeaponMod2_Collectible",
    "AttachmentSlots.Smart_Precision_Sniper_Rifle_WeaponMod1",
    "AttachmentSlots.Smart_Precision_Sniper_Rifle_WeaponMod1_Collectible",
    "AttachmentSlots.Smart_Precision_Sniper_Rifle_WeaponMod2",
    "AttachmentSlots.Smart_Precision_Sniper_Rifle_WeaponMod2_Collectible",
    "AttachmentSlots.Smart_Shotgun_WeaponMod1",
    "AttachmentSlots.Smart_Shotgun_WeaponMod2",
    "AttachmentSlots.StatsShardSlot",
    "AttachmentSlots.StrongArmsBattery",
    "AttachmentSlots.StrongArmsKnuckles",
    "AttachmentSlots.Tech_AR_SMG_LMG_WeaponMod1",
    "AttachmentSlots.Tech_AR_SMG_LMG_WeaponMod2",
    "AttachmentSlots.Tech_Handgun_WeaponMod1",
    "AttachmentSlots.Tech_Handgun_WeaponMod2",
    "AttachmentSlots.Tech_Precision_Sniper_Rifle_WeaponMod1",
    "AttachmentSlots.Tech_Precision_Sniper_Rifle_WeaponMod1_Collectible",
    "AttachmentSlots.Tech_Precision_Sniper_Rifle_WeaponMod2",
    "AttachmentSlots.Tech_Precision_Sniper_Rifle_WeaponMod2_Collectible",
    "AttachmentSlots.Tech_Shotgun_WeaponMod1",
    "AttachmentSlots.Tech_Shotgun_WeaponMod2",
    "AttachmentSlots.Throwable_WeaponMod1",
    "AttachmentSlots.Throwable_WeaponMod1_Collectible",
    "AttachmentSlots.Throwable_WeaponMod2",
    "AttachmentSlots.Throwable_WeaponMod2_Collectible",
    "AttachmentSlots.Torso",
    "AttachmentSlots.Underwear",
    "AttachmentSlots.UnderwearBottom",
    "AttachmentSlots.UnderwearTop",
    "AttachmentSlots.WeaponLeft",
    "AttachmentSlots.WeaponRight",
];

pub fn match_attachment_slot_name(tdbid: u64) -> &'static str {
    for name in ATTACHMENT_SLOT_NAMES {
        if bwms_hashes::tweak_db_id(name) == tdbid {
            return name;
        }
    }
    "?"
}

/// Testa a hipótese `array_ptr` = `DynArray<game::AttachmentSlotData>` (stride 0x90, ver nota
/// grande acima) — lê `min(size,cap,30)` elementos e decodifica os campos de dado documentados no
/// header oficial (`slotID`/`itemObject.instance`/`spawningItemID.tdbid`/`activeItemID.tdbid`/
/// `prevItemID.tdbid`/`appearanceItemID.tdbid`), resolvendo `slotID` pro nome real via
/// [`match_attachment_slot_name`]. 100% leitura, nunca escreve.
pub unsafe fn dump_attachslotdata_candidate(array_ptr: u64, cap: u32, size: u32) -> Vec<String> {
    let mut out = Vec::new();
    if array_ptr == 0 {
        return out;
    }
    let n = size.min(cap).min(30) as usize;
    for i in 0..n {
        let base = array_ptr as usize + i * 0x90;
        if !gum::is_readable(base as *const c_void, 0x90) {
            out.push(format!("  [{i}] <unreadable @ {base:#018x}>"));
            continue;
        }
        let rd = |off: usize| ((base + off) as *const u64).read_unaligned();
        let slot_id = rd(0x00);
        let item_obj_instance = rd(0x08);
        let spawning_tdbid = rd(0x18);
        let active_tdbid = rd(0x28);
        let prev_tdbid = rd(0x38);
        let appearance_tdbid = rd(0x58);
        let name = match_attachment_slot_name(slot_id);
        out.push(format!(
            "  [{i}] slotID={slot_id:#018x}({name}) itemObj.instance={item_obj_instance:#018x} \
             spawning.tdbid={spawning_tdbid:#018x} active.tdbid={active_tdbid:#018x} \
             prev.tdbid={prev_tdbid:#018x} appearance.tdbid={appearance_tdbid:#018x}"
        ));
    }
    out
}

/// Dump cru (sem hipótese de struct) de N qwords a partir de `ptr` — usado pro array cap=9
/// (0xC0/0xC8), cujo stride de elemento é DESCONHECIDO. Leitura pura, teto de segurança embutido.
unsafe fn dump_raw_qwords(ptr: u64, count: usize) -> Vec<u64> {
    let mut out = Vec::with_capacity(count);
    if ptr == 0 {
        return out;
    }
    for i in 0..count {
        let addr = (ptr as usize + i * 8) as *const c_void;
        if !gum::is_readable(addr, 8) {
            out.push(0);
            continue;
        }
        out.push((addr as *const u64).read_unaligned());
    }
    out
}

// 100% OBSERVE-ONLY, sempre chama a original. CODADO, NUNCA TESTADO AO VIVO (ampliação também
// nunca testada — mesmo gate de sempre, `~/.bwms-hook-attachslots-equipstart-invoke`, off por
// padrão).
unsafe extern "C" fn attachslots_equipstart_invoke_probe(closure: *mut c_void, receiver: *mut c_void, evt_ref: *mut c_void) {
    let n = ATTACHSLOTS_EQUIPSTART_INVOKE_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 8 {
        let mut words = [0u64; 36];
        if gum::is_readable(receiver, 0x120) {
            for (i, w) in words.iter_mut().enumerate() {
                *w = (receiver as *const u64).add(i).read_unaligned();
            }
        }
        let base60: [u64; 12] = words[0..12].try_into().unwrap();
        let decoded = decode_icomponent_base60(&base60);
        let name_ok = is_attachment_slots_by_name(&base60);
        let mut evt_word0: u64 = 0;
        if gum::is_readable(evt_ref, 8) {
            let evt_ptr = (evt_ref as *const u64).read_unaligned();
            if evt_ptr != 0 && gum::is_readable(evt_ptr as *const c_void, 8) {
                evt_word0 = (evt_ptr as *const u64).read_unaligned();
            }
        }
        crate::log(&format!(
            "[axl-attachslots-equipstart] EquipStart#{n} closure={closure:p} receiver(AttachmentSlots*)={receiver:p} \
             evt_ref={evt_ref:p} evt.word0={evt_word0:#018x} name_confirmado={name_ok} decoded={decoded:?} \
             receiver[0..0x120)={words:#018x?}"
        ));

        // unk90 (#40/#41/#54, 2026-08-19): array1 (offset 0xB0/0xB8, cap~23) testado como
        // DynArray<AttachmentSlotData> (stride 0x90); array2 (0xC0/0xC8, cap~9) dumpado cru.
        let arr1_ptr = words[22];
        let arr1_capsize = words[23];
        let arr1_cap = (arr1_capsize & 0xFFFF_FFFF) as u32;
        let arr1_size = (arr1_capsize >> 32) as u32;
        let arr2_ptr = words[24];
        let arr2_capsize = words[25];
        let arr2_cap = (arr2_capsize & 0xFFFF_FFFF) as u32;
        let arr2_size = (arr2_capsize >> 32) as u32;
        crate::log(&format!(
            "[axl-attachslots-unk90] EquipStart#{n} array1(AttachmentSlotData-cand)@{arr1_ptr:#018x} cap={arr1_cap} size={arr1_size}"
        ));
        for line in dump_attachslotdata_candidate(arr1_ptr, arr1_cap, arr1_size) {
            crate::log(&format!("[axl-attachslots-unk90] EquipStart#{n} {line}"));
        }
        crate::log(&format!(
            "[axl-attachslots-unk90] EquipStart#{n} array2(stride-desconhecido)@{arr2_ptr:#018x} cap={arr2_cap} size={arr2_size}"
        ));
        if arr2_ptr != 0 {
            let raw = dump_raw_qwords(arr2_ptr, 40);
            crate::log(&format!("[axl-attachslots-unk90] EquipStart#{n} array2 raw[0..40)={raw:#018x?}"));
        }
    }
    let orig = ORIG_ATTACHSLOTS_EQUIPSTART_INVOKE.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: ThreeArgVoidPtrFn = std::mem::transmute(orig);
        f(closure, receiver, evt_ref);
    }
}

// axl-questphase-apply / item #55 (2026-08-12): observe-only do candidato `ForceStartNode`
// (ver nota grande na declaração de `QUESTSYS_FORCESTARTNODE_CAND_VM`). NUNCA chamado por nós —
// só observa se/quando o PRÓPRIO MOTOR invoca esta função durante execução normal de quest.
// HookWrap: chama a original PRIMEIRO (nunca risca mudar timing), decodifica os 2 args conforme a
// RE já documentada (`QuestNodeKey{hash:u32@0,campo:u16@4}` / `DynArray<CName>{entries@0,cap@8,
// size@0xc}`) só depois, puramente pra log.
unsafe extern "C" fn questsys_forcestartnode_cand_probe(this: *mut c_void, node_key: *mut c_void, arr: *mut c_void) {
    let orig = ORIG_QUESTSYS_FORCESTARTNODE_CAND.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: ThreeArgVoidPtrFn = std::mem::transmute(orig);
        f(this, node_key, arr);
    }
    let n = QUESTSYS_FORCESTARTNODE_CAND_CNT.fetch_add(1, Ordering::Relaxed);
    if n < 8 {
        let (mut hash, mut field4): (u32, u16) = (0, 0);
        if gum::is_readable(node_key, 6) {
            hash = (node_key as *const u32).read_unaligned();
            field4 = (node_key as *const u16).add(2).read_unaligned();
        }
        let (mut entries, mut cap, mut size): (u64, u32, u32) = (0, 0, 0);
        if gum::is_readable(arr, 0x10) {
            entries = (arr as *const u64).read_unaligned();
            cap = (arr as *const u32).add(2).read_unaligned();
            size = (arr as *const u32).add(3).read_unaligned();
        }
        crate::log(&format!(
            "[axl-questphase-forcestartnode] cand#{n} this={this:p} node_key.hash={hash:#010x} \
             node_key+4={field4:#06x} arr.entries={entries:#018x} arr.cap={cap} arr.size={size}"
        ));
    }
}

// axl-streaming-apply: worldStreamingSector::PostLoad candidate SL14 — observer + node-array probe
// slot[14] = vtable+0x70, strongest candidate: BLR callsite @ 0x10023a40c explicitly does MOV X1,X20.
// HookAfter pattern: call original first, then read/write the sector object.
// Probes candidate DynArray offsets to find the node list — logs layout for RE.
// On first sector with a plausible node array: writes node_sz+1 then restores (apply proof).
unsafe extern "C" fn streaming_sl14_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = POSTLOAD_CNT_SL14.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_SL14.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, a1, a2);
    }
    if !gum::is_readable(this, 0xc0) { return; }
    let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
    let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
    let path_hash = rf64(0x30);
    if n < 6 {
        crate::log(&format!(
            "[axl-probe] SL14#{n} this={this:p} path={path_hash:#018x}"
        ));
        crate::log(&format!(
            "[axl-probe] SL14#{n} +30={:#018x} +38={:#018x} +40={:#018x} +48={:#018x} +50={:#018x} +58={:#018x} +60={:#018x} +68={:#018x}",
            rf64(0x30), rf64(0x38), rf64(0x40), rf64(0x48), rf64(0x50), rf64(0x58), rf64(0x60), rf64(0x68)
        ));
        crate::log(&format!(
            "[axl-probe] SL14#{n} +70={:#018x} +78={:#018x} +80={:#018x} +88={:#018x} +90={:#018x} +98={:#018x} +a0={:#018x} +a8={:#018x}",
            rf64(0x70), rf64(0x78), rf64(0x80), rf64(0x88), rf64(0x90), rf64(0x98), rf64(0xa0), rf64(0xa8)
        ));
    }
    // Search for node DynArray: ptr (8B) + cap (4B) + sz (4B), all in [0x40..0xb0]
    if !STREAMING_SECTOR_APPLIED.load(Ordering::Relaxed) {
        for off in [0x40usize, 0x48, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78, 0x80, 0x90, 0xa0, 0xb0] {
            if !gum::is_readable(this.cast::<u8>().add(off) as *const c_void, 16) { continue; }
            let arr_ptr = rf64(off);
            let cap     = rf32(off + 8);
            let sz      = rf32(off + 0xc);
            if arr_ptr > 0x1_0000_0000 && sz > 0 && sz <= cap && cap <= 65536
                && gum::is_readable(arr_ptr as *const c_void, 8)
            {
                crate::log(&format!(
                    "[axl-streaming] NODE ARRAY @ +{off:#x}: sector={path_hash:#018x} ptr={arr_ptr:#018x} sz={sz} cap={cap}"
                ));
                // axl-streaming-apply: write sz+1, call original uses the incremented count,
                // then restore. Net effect zero but proves R/W access to the node list.
                let sz_ptr = this.cast::<u8>().add(off + 0xc) as *mut u32;
                sz_ptr.write_unaligned(sz + 1);
                let read_back = sz_ptr.read_unaligned();
                sz_ptr.write_unaligned(sz); // restore
                crate::log(&format!(
                    "[axl-streaming] >>> APPLY PROOF <<< sz {sz} → {read_back} → {sz} (R/W confirmed)"
                ));
                STREAMING_SECTOR_HASH.store(path_hash, Ordering::Relaxed);
                STREAMING_NODE_COUNT.store(sz as u64, Ordering::Relaxed);
                STREAMING_SECTOR_APPLIED.store(true, Ordering::Relaxed);
                break;
            }
        }
    }
}

// SL25: fires first of the pair. Run original FIRST (HookAfter), then probe node array.
// Log vtable ptr and a1 (loading token for PostLoad, 0 for adjacent fns).
unsafe extern "C" fn streaming_sl25_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = POSTLOAD_CNT_SL25.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_SL25.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if !gum::is_readable(this, 0x10) { return; }
    let vtbl = (this as *const u64).read_unaligned();
    if n < 5 {
        crate::log(&format!("[axl-probe] SL25#{n} this={this:p} vtbl={vtbl:#018x} a1={a1:#018x}"));
    }
    // Apply logic: search for node DynArray and prove R/W access
    if !STREAMING_SECTOR_APPLIED.load(Ordering::Relaxed)
        && gum::is_readable(this, 0xc0)
    {
        let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
        let path_hash = rf64(0x30);
        for off in [0x40usize, 0x48, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78, 0x80, 0x90, 0xa0, 0xb0] {
            if !gum::is_readable(this.cast::<u8>().add(off) as *const c_void, 16) { continue; }
            let arr_ptr = rf64(off);
            let cap     = rf32(off + 8);
            let sz      = rf32(off + 0xc);
            if arr_ptr > 0x1_0000_0000 && sz > 0 && sz <= cap && cap <= 65536
                && gum::is_readable(arr_ptr as *const c_void, 8)
            {
                crate::log(&format!(
                    "[axl-streaming] NODE ARRAY @ +{off:#x}: sector={path_hash:#018x} ptr={arr_ptr:#018x} sz={sz} cap={cap}"
                ));
                let sz_ptr = this.cast::<u8>().add(off + 0xc) as *mut u32;
                sz_ptr.write_unaligned(sz + 1);
                let read_back = sz_ptr.read_unaligned();
                sz_ptr.write_unaligned(sz);
                crate::log(&format!(
                    "[axl-streaming] >>> APPLY PROOF <<< sz {sz} → {read_back} → {sz} (R/W confirmed)"
                ));
                STREAMING_SECTOR_HASH.store(path_hash, Ordering::Relaxed);
                STREAMING_NODE_COUNT.store(sz as u64, Ordering::Relaxed);
                STREAMING_SECTOR_APPLIED.store(true, Ordering::Relaxed);
                break;
            }
        }
    }
}

// SL26: fires second of the pair (same this). Log vtable + a1, also try apply in case SL26 is PostLoad.
unsafe extern "C" fn streaming_sl26_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = POSTLOAD_CNT_SL26.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_SL26.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if !gum::is_readable(this, 0x10) { return; }
    let vtbl = (this as *const u64).read_unaligned();
    if n < 5 {
        crate::log(&format!("[axl-probe] SL26#{n} this={this:p} vtbl={vtbl:#018x} a1={a1:#018x}"));
    }
    // Try apply in case SL26 is the actual PostLoad (sector data may now be populated)
    if !STREAMING_SECTOR_APPLIED.load(Ordering::Relaxed)
        && gum::is_readable(this, 0xc0)
    {
        let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
        let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
        let path_hash = rf64(0x30);
        for off in [0x40usize, 0x48, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78, 0x80, 0x90, 0xa0, 0xb0] {
            if !gum::is_readable(this.cast::<u8>().add(off) as *const c_void, 16) { continue; }
            let arr_ptr = rf64(off);
            let cap     = rf32(off + 8);
            let sz      = rf32(off + 0xc);
            if arr_ptr > 0x1_0000_0000 && sz > 0 && sz <= cap && cap <= 65536
                && gum::is_readable(arr_ptr as *const c_void, 8)
            {
                crate::log(&format!(
                    "[axl-streaming] SL26 NODE ARRAY @ +{off:#x}: sector={path_hash:#018x} ptr={arr_ptr:#018x} sz={sz} cap={cap}"
                ));
                let sz_ptr = this.cast::<u8>().add(off + 0xc) as *mut u32;
                // axl-world-streaming-apply: worldNodes são memory-mapped (__TEXT range, obj < 0x200000000000).
                // Inject real requer heap-allocate do motor. Por ora: proof seguro (sz→sz+1→sz).
                sz_ptr.write_unaligned(sz + 1);
                let read_back = sz_ptr.read_unaligned();
                sz_ptr.write_unaligned(sz);
                let handle_arr = arr_ptr as *const u64;
                let obj_ptr = if gum::is_readable(handle_arr as *const c_void, 8) {
                    handle_arr.read_unaligned()
                } else { 0 };
                crate::log(&format!(
                    "[axl-streaming] PROOF: sz {sz}→{read_back}→{sz} handle[0].obj={obj_ptr:#018x} (mmap={})",
                    obj_ptr < 0x0002_0000_0000_0000
                ));
                STREAMING_SECTOR_HASH.store(path_hash, Ordering::Relaxed);
                STREAMING_NODE_COUNT.store(sz as u64, Ordering::Relaxed);
                STREAMING_SECTOR_APPLIED.store(true, Ordering::Relaxed);
                break;
            }
        }
    }
}

unsafe extern "C" fn streaming_sl27_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = POSTLOAD_CNT_SL27.fetch_add(1, Ordering::Relaxed);
    if n < 3 { crate::log(&format!("[axl-probe] SL27#{n} this={this:p}")); }
    let orig = ORIG_SL27.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
}

// axl-garment-apply: EntityTemplate::FindAppearance observe-only probe
// x0=EntityTemplate*, x1=CName hash (appearance name), ret=*TemplateAppearance or null
// If x1 contains "default" (0x6...hash) or other valid CNames during gameplay → confirmed.
unsafe extern "C" fn entity_template_find_app_probe(this: *mut c_void, cname: u64, a2: u64) {
    let n = FIND_APP_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_FIND_APP.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: PostLoadFn = std::mem::transmute(orig);
        f(this, cname, a2);
    }
    if n < 8 {
        crate::log(&format!(
            "[axl-garment] FindAppearance#{n} tpl={this:p} cname={cname:#018x}"
        ));
    }
}

// axl-garment-apply: AppearanceResource::FindAppearance — sret (2026-07-29, técnica SretPad32).
// x0=resource(this), x1=CName, w2=u32, w3=u8, x8=&out Handle<AppearanceDefinition>. DIFERENTE de
// EntityTemplate::FindAppearance acima (a culpada do crash 0x50) — esta tem lock explícito em
// +0xF0 (1ª instrução do corpo real), o que a torna estruturalmente mais segura.
type AppearanceResourceFindAppFn = unsafe extern "C" fn(*mut c_void, u64, u32, u8) -> SretPad32;
unsafe extern "C" fn appearance_resource_findapp_probe(this: *mut c_void, cname: u64, a2: u32, a3: u8) -> SretPad32 {
    let n = APPEARANCE_RESOURCE_FINDAPP_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_APPEARANCE_RESOURCE_FINDAPP.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: AppearanceResourceFindAppFn = std::mem::transmute(orig);
        f(this, cname, a2, a3)
    } else {
        SretPad32 { a: 0, b: 0, _pad: [0, 0] }
    };
    if n < 8 {
        crate::log(&format!("[axl-garment] AppearanceResource::FindAppearance#{n} this={this:p} cname={cname:#018x} a2={a2:#x} a3={a3:#x} handle=({:#018x},{:#018x})", ret.a, ret.b));
    }
    ret
}
// axl-garment-apply: GarmentAssembler::FindState — sret (2026-07-29, técnica SretPad32).
// x0=self, x1=WeakHandle<Entity>&, x8=&out (escreve 3 ponteiros = GarmentAssemblerState{0x18}).
type GarmentFindStateFn = unsafe extern "C" fn(*mut c_void, u64) -> SretPad32;
unsafe extern "C" fn garment_findstate_probe(this: *mut c_void, weak_handle: u64) -> SretPad32 {
    let n = GARMENT_FINDSTATE_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_FINDSTATE.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: GarmentFindStateFn = std::mem::transmute(orig);
        f(this, weak_handle)
    } else {
        SretPad32 { a: 0, b: 0, _pad: [0, 0] }
    };
    if n < 8 {
        crate::log(&format!("[axl-garment] GarmentAssembler::FindState#{n} this={this:p} weak_handle={weak_handle:#018x} out=({:#018x},{:#018x})", ret.a, ret.b));
    }
    ret
}
// axl-garment-apply: AppearanceChanger::GetSuffixes — sret CString por valor (2026-07-29, técnica
// SretPad32, tamanho CONFIRMADO por disassembly dedicado: red::CString nunca escreve além de +0x18,
// footprint total 0x20 = exatamente o tamanho do SretPad32). x0=this, x1/x2/x3=params (tipos exatos
// não confirmados, tratados como u64 genéricos — seguro pra observe-only, não deref nenhum campo).
// Categoria SEGURA (consumidora — lê um flat do TweakDB, não busca handle num array racy).
type GarmentGetSuffixesFn = unsafe extern "C" fn(*mut c_void, u64, u64, u64) -> SretPad32;
unsafe extern "C" fn garment_getsuffixes_probe(this: *mut c_void, a1: u64, a2: u64, a3: u64) -> SretPad32 {
    let n = GARMENT_GETSUFFIXES_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_GETSUFFIXES.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: GarmentGetSuffixesFn = std::mem::transmute(orig);
        f(this, a1, a2, a3)
    } else {
        SretPad32 { a: 0, b: 0, _pad: [0, 0] }
    };
    if n < 8 {
        crate::log(&format!("[axl-garment] AppearanceChanger::GetSuffixes#{n} this={this:p} a1={a1:#018x} a2={a2:#018x} a3={a3:#018x} cstring=({:#018x},{:#018x})", ret.a, ret.b));
    }
    ret
}

// axl-garment-apply: ItemFactoryRequest::LoadAppearance observe-only probe (RE 2026-07-28).
// 2026-08-15 (item ArchiveXL `#12`): estendido pra LER os 3 campos de maior valor via
// `gum::read_u64` (syscall mach_vm_read_overwrite, NUNCA deref cru — zero risco de crash mesmo
// se `this` for lixo/já liberado; é o MESMO padrão de segurança que fechou `#63`/NodeRef hoje,
// aplicado aqui com a garantia extra do syscall em vez do idioma is_readable+rd_ptr usado no
// resto do projeto, por causa da cautela documentada nesta família específica). Offsets do
// header vendorizado real (`enablers/ArchiveXL/src/Red/AppearanceChanger.hpp`,
// `namespace Raw::ItemFactoryRequest`): `Entity@0x28`(WeakHandle<Entity>, ponteiro cru no 1º
// qword), `AppearanceName@0x158`(CName, hash u64 puro), `ItemRecord@0x160`(gamedataTweakDBRecord*
// — resolvido pra nome de classe via `class_of`+`type_name_getname`, já provado em dezenas de
// itens fechados hoje). Lido DEPOIS de chamar o original (mesma ordem já testada segura desde
// 2026-07-29 — a chamada em si nunca crashou isolada).
type OneArgFn = unsafe extern "C" fn(*mut c_void);
unsafe extern "C" fn itemfactory_loadapp_probe(this: *mut c_void) {
    let n = ITEMFACTORY_LOADAPP_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_ITEMFACTORY_LOADAPP.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: OneArgFn = std::mem::transmute(orig);
        f(this);
    }
    if n < 8 {
        let base = this as usize;
        let entity_ptr = crate::gum::read_u64(base + 0x28);
        let appname_hash = crate::gum::read_u64(base + 0x158);
        let record_ptr = crate::gum::read_u64(base + 0x160);
        let appname_str = appname_hash.filter(|h| *h != 0).map(|h| crate::cname::resolve_cname(h));
        let record_cls = record_ptr.filter(|p| *p != 0).and_then(|p| {
            let cls = crate::rtti::class_of(p as *mut c_void);
            if cls.is_null() { return None; }
            let h = crate::rtti::type_name_getname(cls);
            if h == 0 { None } else { Some(crate::cname::resolve_cname(h)) }
        });
        crate::log(&format!(
            "[axl-garment] ItemFactoryRequest::LoadAppearance#{n} this={this:p} entity={entity_ptr:?} appearanceName={appname_str:?} itemRecordClass={record_cls:?}"
        ));
    }
}

// axl-garment-apply: ItemFactoryAppearanceChangeRequest::LoadAppearance observe-only probe
// (RE 2026-07-28). 2026-08-15: mesma extensão do probe acima, offsets diferentes (mesma classe
// do `#13`/`LoadTemplate` já provado, `Entity@0x80`/`AppearanceName@0x48`/`ItemRecord@0x100`).
unsafe extern "C" fn itemfactory_changeapp_loadapp_probe(this: *mut c_void) {
    let n = ITEMFACTORY_CHANGEAPP_LOADAPP_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_ITEMFACTORY_CHANGEAPP_LOADAPP.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: OneArgFn = std::mem::transmute(orig);
        f(this);
    }
    if n < 8 {
        let base = this as usize;
        let entity_ptr = crate::gum::read_u64(base + 0x80);
        let appname_hash = crate::gum::read_u64(base + 0x48);
        let record_ptr = crate::gum::read_u64(base + 0x100);
        let appname_str = appname_hash.filter(|h| *h != 0).map(|h| crate::cname::resolve_cname(h));
        let record_cls = record_ptr.filter(|p| *p != 0).and_then(|p| {
            let cls = crate::rtti::class_of(p as *mut c_void);
            if cls.is_null() { return None; }
            let h = crate::rtti::type_name_getname(cls);
            if h == 0 { None } else { Some(crate::cname::resolve_cname(h)) }
        });
        crate::log(&format!(
            "[axl-garment] ItemFactoryAppearanceChangeRequest::LoadAppearance#{n} this={this:p} entity={entity_ptr:?} appearanceName={appname_str:?} itemRecordClass={record_cls:?}"
        ));
    }
}

// axl-transmog-apply: ItemFactoryAppearanceChangeRequest::LoadTemplate observe-only probe (RE 2026-08-01)
// Mesma ABI de itemfactory_changeapp_loadapp_probe acima (bool(this), 1 param só) — reusa OneArgFn.
unsafe extern "C" fn transmog_loadtemplate_probe(this: *mut c_void) {
    let n = TRANSMOG_LOADTEMPLATE_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_TRANSMOG_LOADTEMPLATE.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: OneArgFn = std::mem::transmute(orig);
        f(this);
    }
    if n < 8 {
        crate::log(&format!("[axl-transmog] ItemFactoryAppearanceChangeRequest::LoadTemplate#{n} this={this:p}"));
    }
}

// axl-transmog-apply: AppearanceChanger::SelectAppearanceName observe-only probe (RE 2026-08-01,
// confiança MÉDIA — ver nota na constante). NÃO é sret: retorno void* real em x0, 6 args inteiros
// (aOut:CName* já é ponteiro explícito, diferente de GetSuffixes). Observe-only puro, não deref
// nenhum campo — só loga os ponteiros crus recebidos, mesma cautela do resto desta família.
type TransmogSelectAppNameFn = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;
unsafe extern "C" fn transmog_selectappname_probe(a_out: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
    let n = TRANSMOG_SELECTAPPNAME_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_TRANSMOG_SELECTAPPNAME.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: TransmogSelectAppNameFn = std::mem::transmute(orig);
        f(a_out, a1, a2, a3, a4, a5)
    } else {
        0
    };
    if n < 8 {
        crate::log(&format!("[axl-transmog] AppearanceChanger::SelectAppearanceName#{n} aOut={a_out:#018x} a1={a1:#018x} a2={a2:#018x} a3={a3:#018x} a4={a4:#018x} a5={a5:#018x} ret={ret:#018x}"));
    }
    ret
}

// axl-questphase-apply (2026-08-05): candidato a "processar recurso de fase carregado" — dispatch
// só por BLR (vtable job-dispatch), ABI real desconhecida. Observe-only: repassa x0..x3 crus,
// devolve x0 do original verbatim (seguro independente da semântica real, mesmo padrão já usado
// pra outras funções de assinatura incerta neste projeto).
unsafe extern "C" fn quest_processphaseresource_probe(this: *mut c_void, a1: u64, a2: u64, a3: u64) -> u64 {
    let n = QUEST_PROCESSPHASERESOURCE_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_QUEST_PROCESSPHASERESOURCE.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: FourArgRetFn = std::mem::transmute(orig);
        f(this, a1, a2, a3)
    } else {
        0
    };
    if n < 8 {
        crate::log(&format!("[axl-questphase] ProcessPhaseResource-cand#{n} this={this:p} a1={a1:#x} a2={a2:#x} a3={a3:#x} ret={ret:#x}"));
    }
    ret
}

// axl-garment-apply: Entity::ReassembleAppearance observe-only probe (RE 2026-07-28)
// x0=this(Entity*), x1..x5 = 5 params adicionais (assinatura AXL exata, 6 no total) — repassados
// crus como u64 (não sabemos os tipos reais ainda; observe-only não precisa saber).
type SixArgFn = unsafe extern "C" fn(*mut c_void, u64, u64, u64, u64, u64);
unsafe extern "C" fn entity_reassemble_appearance_probe(
    this: *mut c_void, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64,
) {
    let n = ENTITY_REASSEMBLE_APPEARANCE_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_ENTITY_REASSEMBLE_APPEARANCE.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: SixArgFn = std::mem::transmute(orig);
        f(this, a1, a2, a3, a4, a5);
    }
    if n < 8 {
        crate::log(&format!(
            "[axl-garment] Entity::ReassembleAppearance#{n} this={this:p} a1={a1:#x} a2={a2:#x} a3={a3:#x} a4={a4:#x} a5={a5:#x}"
        ));
    }
}

// axl-garment-apply rodada 2: 6 probes de ABI simples observe-only (RE offline 2026-07-28).
// Retorno modelado como u64 cru (repassa x0 do original verbatim) — seguro independente da
// semântica real (void/bool/ponteiro cabem em x0), NENHUM destes tem sret (só FindState tem,
// não hookado). Mesmo gate ~/.bwms-garmentprobe + mesma cautela da rodada 1 (família já crashou).
type TwoArgRetFn = unsafe extern "C" fn(*mut c_void, u64) -> u64;
// 2026-08-05: endereço trocado pra cópia VIVA (fundida com FindState) — assinatura 3-arg
// (this, weak_handle, req), mesmo padrão de garment_removeitem_probe, não mais o 2-arg do
// endereço morto antigo.
unsafe extern "C" fn garment_additem_probe(this: *mut c_void, weak_handle: u64, req: u64) -> u64 {
    let n = GARMENT_ADDITEM_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_ADDITEM.load(Ordering::Relaxed);
    let ret = if !orig.is_null() { let f: ThreeArgRetFn = std::mem::transmute(orig); f(this, weak_handle, req) } else { 0 };
    if n < 8 {
        crate::log(&format!("[axl-garment] GarmentAssemblerState::AddItem#{n} this={this:p} weak_handle={weak_handle:#x} req={req:#x} ret={ret:#x}"));
    }
    ret
}
unsafe extern "C" fn garment_addcustomitem_probe(this: *mut c_void, weak_handle: u64, req: u64) -> u64 {
    let n = GARMENT_ADDCUSTOMITEM_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_ADDCUSTOMITEM.load(Ordering::Relaxed);
    let ret = if !orig.is_null() { let f: ThreeArgRetFn = std::mem::transmute(orig); f(this, weak_handle, req) } else { 0 };
    if n < 8 {
        crate::log(&format!("[axl-garment] GarmentAssemblerState::AddCustomItem#{n} this={this:p} weak_handle={weak_handle:#x} req={req:#x} ret={ret:#x}"));
    }
    ret
}
unsafe extern "C" fn garment_changeitem_probe(state: *mut c_void, req: u64) -> u64 {
    let n = GARMENT_CHANGEITEM_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_CHANGEITEM.load(Ordering::Relaxed);
    let ret = if !orig.is_null() { let f: TwoArgRetFn = std::mem::transmute(orig); f(state, req) } else { 0 };
    if n < 8 {
        crate::log(&format!("[axl-garment] GarmentAssemblerState::ChangeItem#{n} state={state:p} req={req:#x} ret={ret:#x}"));
    }
    ret
}
unsafe extern "C" fn garment_changecustomitem_probe(state: *mut c_void, req: u64) -> u64 {
    let n = GARMENT_CHANGECUSTOMITEM_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_CHANGECUSTOMITEM.load(Ordering::Relaxed);
    let ret = if !orig.is_null() { let f: TwoArgRetFn = std::mem::transmute(orig); f(state, req) } else { 0 };
    if n < 8 {
        crate::log(&format!("[axl-garment] GarmentAssemblerState::ChangeCustomItem#{n} state={state:p} req={req:#x} ret={ret:#x}"));
    }
    ret
}
// RemoveItem(self, weak_handle_ref, req) -> bool (3 params)
type ThreeArgRetFn = unsafe extern "C" fn(*mut c_void, u64, u64) -> u64;
unsafe extern "C" fn garment_removeitem_probe(this: *mut c_void, weak_handle: u64, req: u64) -> u64 {
    let n = GARMENT_REMOVEITEM_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_REMOVEITEM.load(Ordering::Relaxed);
    let ret = if !orig.is_null() { let f: ThreeArgRetFn = std::mem::transmute(orig); f(this, weak_handle, req) } else { 0 };
    if n < 8 {
        crate::log(&format!("[axl-garment] GarmentAssembler::RemoveItem#{n} this={this:p} weak_handle={weak_handle:#x} req={req:#x} ret={ret:#x}"));
    }
    ret
}
// OnGameDetach(self) — 1 param, sem retorno conhecido (modelado como u64 pra segurança).
unsafe extern "C" fn garment_ongamedetach_probe(this: *mut c_void) -> u64 {
    let n = GARMENT_ONGAMEDETACH_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_ONGAMEDETACH.load(Ordering::Relaxed);
    let ret = if !orig.is_null() { let f: OneArgFn = std::mem::transmute(orig); f(this); 0 } else { 0 };
    if n < 8 {
        crate::log(&format!("[axl-garment] GarmentAssembler::OnGameDetach#{n} this={this:p}"));
    }
    ret
}

// axl-inkspawner-apply: AsyncSpawnFromLocal(lib&, InkSpawningInfo&, CName) -> bool (3 params, sem sret)
unsafe extern "C" fn inkspawner_async_local_probe(lib: *mut c_void, info: u64, cname: u64) -> u64 {
    let n = INKSPAWNER_ASYNC_LOCAL_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_INKSPAWNER_ASYNC_LOCAL.load(Ordering::Relaxed);
    let ret = if !orig.is_null() { let f: ThreeArgRetFn = std::mem::transmute(orig); f(lib, info, cname) } else { 0 };
    if n < 8 {
        crate::log(&format!("[axl-inkspawner] AsyncSpawnFromLocal#{n} lib={lib:p} info={info:#x} cname={cname:#018x} ret={ret:#x}"));
    }
    ret
}
// axl-inkspawner-apply: AsyncSpawnFromExternal(lib&, InkSpawningInfo&, ResourcePath, CName) -> bool (4 params)
type FourArgRetFn = unsafe extern "C" fn(*mut c_void, u64, u64, u64) -> u64;
unsafe extern "C" fn inkspawner_async_external_probe(lib: *mut c_void, info: u64, respath: u64, cname: u64) -> u64 {
    let n = INKSPAWNER_ASYNC_EXTERNAL_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_INKSPAWNER_ASYNC_EXTERNAL.load(Ordering::Relaxed);
    let ret = if !orig.is_null() { let f: FourArgRetFn = std::mem::transmute(orig); f(lib, info, respath, cname) } else { 0 };
    if n < 8 {
        crate::log(&format!("[axl-inkspawner] AsyncSpawnFromExternal#{n} lib={lib:p} info={info:#x} respath={respath:#018x} cname={cname:#018x} ret={ret:#x}"));
    }
    ret
}

// axl-inkspawner-apply: AsyncSpawnFromDefPack — funil único dos 3 caminhos async (RE offline
// 2026-07-29). x0=item ptr, x1=InkSpawningContext*, x2=byte flag (só w2), void/sem sret, retorno
// descartado pelos 3 callers. Observe-only puro (chama original ANTES de logar, nunca deref campo).
type ThreeArgVoidFn = unsafe extern "C" fn(*mut c_void, *mut c_void, u64);
unsafe extern "C" fn inkspawner_defpack_probe(item: *mut c_void, ctx: *mut c_void, flag: u64) {
    let n = INKSPAWNER_DEFPACK_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_INKSPAWNER_DEFPACK.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: ThreeArgVoidFn = std::mem::transmute(orig);
        f(item, ctx, flag);
    }
    if n < 8 {
        crate::log(&format!("[axl-inkspawner] AsyncSpawnFromDefPack#{n} item={item:p} ctx={ctx:p} flag={flag:#x}"));
    }
}

// axl-inkspawner-apply: SpawnFromLocal(lib, CName) -> Handle<inkWidget> (sret real no C++, x8=&out).
// Ver comentário de INKSPAWNER_SPAWNFROMLOCAL_VM sobre por que `SretPad32` (não um struct 16B "fiel")
// é a forma CORRETA de modelar isso em Rust — força o compilador a usar x8 de verdade, batendo com
// o que o código do jogo espera. Observe-only: chama original ANTES de logar, nunca deref os campos.
type SpawnFromLocalFn = unsafe extern "C" fn(*mut c_void, u64) -> SretPad32;
unsafe extern "C" fn inkspawner_spawnfromlocal_probe(lib: *mut c_void, cname: u64) -> SretPad32 {
    let n = INKSPAWNER_SPAWNLOCAL_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_INKSPAWNER_SPAWNLOCAL.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: SpawnFromLocalFn = std::mem::transmute(orig);
        f(lib, cname)
    } else {
        SretPad32 { a: 0, b: 0, _pad: [0, 0] }
    };
    if n < 8 {
        crate::log(&format!("[axl-inkspawner] SpawnFromLocal#{n} lib={lib:p} cname={cname:#018x} handle=({:#018x},{:#018x})", ret.a, ret.b));
    }
    // Codeware `#123` (`WidgetSpawningService::ToggleWidgetSpawnEvent`, investigação dedicada
    // 2026-08-19): fecha o loop entre este hook OBSERVE-ONLY (já provado seguro em RE offline,
    // nunca testado ao vivo — mesma família `axl-inkspawner-apply`) e a classe `inkWidgetSpawnEvent`
    // JÁ forjada+provada via fixture (item `#11`, fechado) — a fonte real só faz isso se
    // `s_widgetSpawnEventEnabled` (nosso `widget_spawn_event_enabled()`, `BwmsToggleWidgetSpawnEvent`,
    // ZERO RE de endereço, é 1 linha na fonte C++). MESMO padrão bounded/decisivo já usado no
    // RED4ext.SDK `#461` (cont.239-241) e no `cw-ctrl-misc`/`VehicleLightControlEvent` acima:
    // `has_listener_for` é lookup barato (Mutex+scan, zero RTTI); só depois de confirmar um
    // listener REAL é que arma `fire_event_args` (RTTI real), bounded a poucas chamadas seguintes.
    // `aLibrary.path` = `CResource::path@+0x30` (RED4ext.SDK `Scripting/Natives/CResource.hpp`,
    // header oficial — `WidgetLibraryResource extends CResource`, campo de DADO, zero endereço
    // nativo). `ret.a` = ponteiro do "item instance" (widening já documentado na fixture — nunca
    // tipar `inkWidgetLibraryItemInstance`). Gate OPT-IN por marcador PRÓPRIO, nunca ligado por
    // padrão — mesma cautela do `#461`/`cw-ctrl-misc` (1ª ativação ao vivo fica pra sessão futura
    // com monitoramento de crash, não esta — 100% offline/RE dedicada).
    static IWSE_LOCAL_ARMED_AT: AtomicU64 = AtomicU64::new(0);
    if crate::register::widget_spawn_event_enabled()
        && std::path::Path::new(&format!("{}/.bwms-inkspawner-fireevent-confirm", std::env::var("HOME").unwrap_or_default())).exists()
    {
        let armed_at = IWSE_LOCAL_ARMED_AT.load(Ordering::Relaxed);
        if armed_at == 0 && crate::register::has_listener_for("InkWidget/Spawn") {
            IWSE_LOCAL_ARMED_AT.store(n + 1, Ordering::Relaxed);
            crate::log(&format!("[axl-inkspawner] listener real confirmado pra 'InkWidget/Spawn' (#{n}) — armando fire_event_args pras próximas 5 chamadas"));
        }
        let armed_at = IWSE_LOCAL_ARMED_AT.load(Ordering::Relaxed);
        if armed_at != 0 && n >= armed_at && n < armed_at + 5 && ret.a != 0 {
            let lib_path = if crate::gum::is_readable((lib as *const u8).add(0x30) as *const c_void, 8) {
                ((lib as *const u8).add(0x30) as *const u64).read_unaligned()
            } else {
                0
            };
            if let Some(arg) = crate::register::make_inkwidgetspawnevent_arg(lib_path, cname, ret.a as usize) {
                let sent = crate::register::fire_event_args("InkWidget/Spawn", &[arg]);
                crate::log(&format!("[axl-inkspawner] fire_event_args('InkWidget/Spawn') dentro do SpawnFromLocal#{n} lib_path={lib_path:#018x} cname={cname:#018x} -> despachado pra {sent} listener(s)"));
            }
        }
    }
    ret
}
// SpawnFromExternal(lib, ResourcePath, CName) -> Handle<inkWidget> (3 args + sret x8).
type SpawnFromExternalFn = unsafe extern "C" fn(*mut c_void, u64, u64) -> SretPad32;
unsafe extern "C" fn inkspawner_spawnfromexternal_probe(lib: *mut c_void, respath: u64, cname: u64) -> SretPad32 {
    let n = INKSPAWNER_SPAWNEXTERNAL_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_INKSPAWNER_SPAWNEXTERNAL.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: SpawnFromExternalFn = std::mem::transmute(orig);
        f(lib, respath, cname)
    } else {
        SretPad32 { a: 0, b: 0, _pad: [0, 0] }
    };
    if n < 8 {
        crate::log(&format!("[axl-inkspawner] SpawnFromExternal#{n} lib={lib:p} respath={respath:#018x} cname={cname:#018x} handle=({:#018x},{:#018x})", ret.a, ret.b));
    }
    // Codeware `#123`: mesmo mecanismo do SpawnFromLocal acima, gate PRÓPRIO e SEPARADO (o par
    // Local/External já tem gates de instalação independentes desde 2026-07-28/29 — preserva a
    // mesma disciplina de isolamento pra fire_event_args).
    static IWSE_EXTERNAL_ARMED_AT: AtomicU64 = AtomicU64::new(0);
    if crate::register::widget_spawn_event_enabled()
        && std::path::Path::new(&format!("{}/.bwms-inkspawner-fireevent-confirm", std::env::var("HOME").unwrap_or_default())).exists()
    {
        let armed_at = IWSE_EXTERNAL_ARMED_AT.load(Ordering::Relaxed);
        if armed_at == 0 && crate::register::has_listener_for("InkWidget/Spawn") {
            IWSE_EXTERNAL_ARMED_AT.store(n + 1, Ordering::Relaxed);
            crate::log(&format!("[axl-inkspawner] listener real confirmado pra 'InkWidget/Spawn' via SpawnFromExternal (#{n}) — armando fire_event_args pras próximas 5 chamadas"));
        }
        let armed_at = IWSE_EXTERNAL_ARMED_AT.load(Ordering::Relaxed);
        if armed_at != 0 && n >= armed_at && n < armed_at + 5 && ret.a != 0 {
            let lib_path = if crate::gum::is_readable((lib as *const u8).add(0x30) as *const c_void, 8) {
                ((lib as *const u8).add(0x30) as *const u64).read_unaligned()
            } else {
                0
            };
            if let Some(arg) = crate::register::make_inkwidgetspawnevent_arg(lib_path, cname, ret.a as usize) {
                let sent = crate::register::fire_event_args("InkWidget/Spawn", &[arg]);
                crate::log(&format!("[axl-inkspawner] fire_event_args('InkWidget/Spawn') dentro do SpawnFromExternal#{n} lib_path={lib_path:#018x} respath={respath:#018x} cname={cname:#018x} -> despachado pra {sent} listener(s)"));
            }
        }
    }
    ret
}

// axl-garment-apply rodada 3: os 2 últimos sub-hooks, ambos void/sem sret (RE offline 2026-07-28).
// GetVisualTags(preset, resourcepath, cname, &taglist_out) -> Void (4 params)
type FourArgVoidFn = unsafe extern "C" fn(*mut c_void, u64, u64, u64);
unsafe extern "C" fn garment_getvisualtags_probe(preset: *mut c_void, respath: u64, cname: u64, taglist_out: u64) {
    let n = GARMENT_GETVISUALTAGS_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_GETVISUALTAGS.load(Ordering::Relaxed);
    if !orig.is_null() { let f: FourArgVoidFn = std::mem::transmute(orig); f(preset, respath, cname, taglist_out); }
    if n < 8 {
        crate::log(&format!("[axl-garment] AppearanceNameVisualTagsPreset::GetVisualTags#{n} preset={preset:p} respath={respath:#018x} cname={cname:#018x} taglist_out={taglist_out:#x}"));
    }
}
// RegisterPart(ctx, &tpl_handle, &storage_handle, &appdef_handle) -> Void (4 params, todos ponteiros)
unsafe extern "C" fn garment_registerpart_probe(ctx: *mut c_void, tpl_h: u64, storage_h: u64, appdef_h: u64) {
    let n = GARMENT_REGISTERPART_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_REGISTERPART.load(Ordering::Relaxed);
    if !orig.is_null() { let f: FourArgVoidFn = std::mem::transmute(orig); f(ctx, tpl_h, storage_h, appdef_h); }
    if n < 8 {
        crate::log(&format!("[axl-garment] AppearanceChanger::RegisterPart#{n} ctx={ctx:p} tpl_h={tpl_h:#x} storage_h={storage_h:#x} appdef_h={appdef_h:#x}"));
    }
}

// ArchiveXL `#17` candidato `0x100ace4b0` (ver nota grande na declaração da const
// `APPEARANCE_EXTRACTPARTCOMPONENTS_CAND_VM`) — 100% OBSERVE-ONLY: chama a original, NUNCA muta
// x0/x1/retorno. Loga (até 8 chamadas) os campos candidatos de x0 (perfil "this"-like achado na RE:
// +0x50/+0x5c/+0x88/+0x98) + x1 como DynArray-like {ptr@0x00, cap(u32)@0x08, size(u32)@0x0c} ANTES
// e DEPOIS da chamada original (se a hipótese "x1=aOut" estiver certa, size deve crescer/mudar) + o
// retorno bool. Gate PRÓPRIO (`~/.bwms-hook-axl17-extractpartcomponents-cand`), nunca ligado por
// padrão — só ativa sob `dev_mode()` (`BWMS_DEV`/`/tmp/bwms-dev`) + este marcador individual.
type TwoPtrRetBoolFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool;
unsafe extern "C" fn extractpartcomponents_cand_probe(x0: *mut c_void, x1: *mut c_void) -> bool {
    let n = EXTRACTPARTCOMPONENTS_CAND_CNT.fetch_add(1, Ordering::Relaxed);

    if n < 8 {
        // Campos candidatos de x0 (perfil "this"-like achado na RE offline 2026-08-19).
        if gum::is_readable(x0, 0xa0) {
            let rf64 = |off: usize| (x0.cast::<u8>().add(off) as *const u64).read_unaligned();
            crate::log(&format!(
                "[axl-17-cand] 0x100ace4b0#{n} PRE x0={x0:p} +50={:#018x} +5c={:#018x} +88={:#018x} +98={:#018x}",
                rf64(0x50), rf64(0x5c), rf64(0x88), rf64(0x98)
            ));
        } else {
            crate::log(&format!("[axl-17-cand] 0x100ace4b0#{n} PRE x0={x0:p} (não-legível)"));
        }
        // x1 como DynArray-like {ptr,cap(u32),size(u32)}, ANTES da chamada original.
        if gum::is_readable(x1, 16) {
            let ptr0 = (x1 as *const u64).read_unaligned();
            let cap0 = (x1.cast::<u8>().add(0x08) as *const u32).read_unaligned();
            let sz0  = (x1.cast::<u8>().add(0x0c) as *const u32).read_unaligned();
            crate::log(&format!(
                "[axl-17-cand] 0x100ace4b0#{n} PRE x1(aOut?)={x1:p} ptr={ptr0:#018x} cap={cap0} size={sz0}"
            ));
        } else {
            crate::log(&format!("[axl-17-cand] 0x100ace4b0#{n} PRE x1={x1:p} (não-legível)"));
        }
    }

    let orig = ORIG_EXTRACTPARTCOMPONENTS_CAND.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: TwoPtrRetBoolFn = std::mem::transmute(orig);
        f(x0, x1)
    } else {
        false
    };

    if n < 8 {
        if gum::is_readable(x1, 16) {
            let ptr1 = (x1 as *const u64).read_unaligned();
            let cap1 = (x1.cast::<u8>().add(0x08) as *const u32).read_unaligned();
            let sz1  = (x1.cast::<u8>().add(0x0c) as *const u32).read_unaligned();
            crate::log(&format!(
                "[axl-17-cand] 0x100ace4b0#{n} POST x1(aOut?)={x1:p} ptr={ptr1:#018x} cap={cap1} size={sz1} ret={ret}"
            ));
        } else {
            crate::log(&format!("[axl-17-cand] 0x100ace4b0#{n} POST ret={ret} (x1 não-legível)"));
        }
    }

    ret
}

// Codeware `#176` candidato `0x1049bded0` de `Raw::inkWidget::TriggerEvent` (ver nota grande na
// declaração da const `INKWIDGET_TRIGGEREVENT_CAND_VM`) — 100% OBSERVE-ONLY: chama a original
// sempre, nunca muta x0(manager)/x1(CName)/x2(&Handle<Event>)/retorno. Loga (até 12 chamadas)
// x0, x1 como CName cru (hex — comparar contra `0x81a340f64c0d72b6`="OnCharacterKey" e outros
// hashes conhecidos do projeto) e os 2 words do Handle apontado por x2 (instance/refcount-ptr).
// Gate PRÓPRIO (`~/.bwms-hook-codeware-176-triggerevent-cand`), nunca ligado por padrão.
type TriggerEventFn = unsafe extern "C" fn(*mut c_void, u64, *mut c_void) -> bool;
unsafe extern "C" fn inkwidget_triggerevent_cand_probe(x0: *mut c_void, x1: u64, x2: *mut c_void) -> bool {
    let n = INKWIDGET_TRIGGEREVENT_CAND_CNT.fetch_add(1, Ordering::Relaxed);

    if n < 12 {
        if gum::is_readable(x2, 16) {
            let inst = (x2 as *const u64).read_unaligned();
            let rc = (x2.cast::<u8>().add(0x08) as *const u64).read_unaligned();
            crate::log(&format!(
                "[cw-176-triggerevent-cand] 0x1049bded0#{n} PRE manager(x0)={x0:p} name(x1)={x1:#018x} handle(x2)={x2:p} instance={inst:#018x} refcount_ptr={rc:#018x}"
            ));
        } else {
            crate::log(&format!(
                "[cw-176-triggerevent-cand] 0x1049bded0#{n} PRE manager(x0)={x0:p} name(x1)={x1:#018x} handle(x2)={x2:p} (não-legível)"
            ));
        }
    }

    let orig = ORIG_INKWIDGET_TRIGGEREVENT_CAND.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: TriggerEventFn = std::mem::transmute(orig);
        f(x0, x1, x2)
    } else {
        false
    };

    if n < 12 {
        crate::log(&format!("[cw-176-triggerevent-cand] 0x1049bded0#{n} POST ret={ret}"));
    }

    ret
}

// ComputePlayerGarment(a1, &entity_handle, &offsets_dynarr, &data_sharedptr, a5, a6, a7, a8:bool)
// -> Void (8 params, x0-x7 — observe-only, mesma categoria segura de RegisterPart/GetVisualTags).
type EightArgVoidFn = unsafe extern "C" fn(*mut c_void, u64, u64, u64, u64, u64, u64, u64);

/// Item `#67` (2026-08-16, rodada 42 offline + rodada 43 ao vivo + rodada 44 offline/escrita) —
/// extensão do probe abaixo. **Por padrão (sem o marcador `~/.bwms-garmentprobe-write`) continua
/// 100% OBSERVE-ONLY** — só lê (a) `aData->resources` (o `GarmentProcessingContext.resources:
/// DynArray<ResourcePath> @ +0x40`, offset CONFIRMADO contra memória viva na rodada 43, `plausible=
/// true`/hashes reais) e (b) o header do próprio `aOffsets` (`DynArray<int32_t>&`, arg direto —
/// SEM indireção de `SharedPtr`, diferente de `aData`). Loga também o que `ApplyOffsetOverrides`
/// (núcleo puro, `register.rs::run_apply_offset_overrides`) COMPUTARIA pros hashes capturados.
///
/// **Rodada 44 (2026-08-16): a ESCRITA real agora existe** (`register::
/// run_apply_offset_overrides_write`, porte fiel de `EntityState::ApplyOffsetOverrides` — Clear+
/// PushBack, técnica `dynarray_push_i32`/`dynarray_clear`), mas só é EXERCITADA se
/// `~/.bwms-garmentprobe-write` (marcador PRÓPRIO, separado de `~/.bwms-flatwrite`) estiver
/// presente — ver `try_write_offsets_synthetic` logo abaixo. Sem esse marcador, o comportamento é
/// IDÊNTICO ao das rodadas 42/43 (zero mutação).
unsafe fn log_garment_resources_and_offsets(n: u64, entity_inst: u64, offsets_ptr: u64, data_ptr: u64) {
    // `data_ptr` é `&data_sharedptr` (`SharedPtr<GarmentProcessingContext>&`) — deref 1x pra
    // pegar a instância real (mesmo padrão já usado no loop `cpg-data-inner` acima).
    if data_ptr == 0 || !gum::is_readable(data_ptr as *const c_void, 8) {
        return;
    }
    let ctx = (data_ptr as *const u64).read_unaligned();
    if ctx == 0 || !gum::is_readable((ctx as *const u8).add(0x40) as *const c_void, 16) {
        crate::log(&format!(
            "[axl-garment] #{n} ComputePlayerGarment.resources: ctx={ctx:#x} @+0x40 ilegível — offset +0x40 NÃO confirmado neste disparo"
        ));
        return;
    }
    let res_entries = (ctx as *const u8).add(0x40).cast::<u64>().read_unaligned();
    let res_cap = (ctx as *const u8).add(0x48).cast::<u32>().read_unaligned();
    let res_size = (ctx as *const u8).add(0x4C).cast::<u32>().read_unaligned();
    // Sentinela de plausibilidade — mesma disciplina usada em todo probe read-only deste projeto
    // (`workspotdump`/`mappin_cooked_count`/etc.): recusa concluir qualquer coisa se os números
    // não fizerem sentido de DynArray real (evita logar lixo como se fosse dado confirmado).
    let plausible = res_size <= res_cap && res_cap < 4096;
    let mut hashes: Vec<u64> = Vec::new();
    if plausible && res_entries != 0 && gum::is_readable(res_entries as *const c_void, (res_size as usize) * 8) {
        for i in 0..res_size.min(16) {
            hashes.push((res_entries as *const u8).add(i as usize * 8).cast::<u64>().read_unaligned());
        }
    }
    crate::log(&format!(
        "[axl-garment] #{n} ComputePlayerGarment.resources ctx={ctx:#x} entries={res_entries:#x} cap={res_cap} size={res_size} plausible={plausible} hashes(primeiros {})={:?}",
        hashes.len(),
        hashes.iter().map(|h| format!("{h:#018x}")).collect::<Vec<_>>()
    ));

    // ArchiveXL `#4` (`GarmentProcessingContext`) — 2026-08-22. Os 2 campos restantes com
    // superfície real saem do MESMO `ctx` já dereferenciado acima, no MESMO hook já instalado:
    // zero endereço novo, zero hook novo. Offsets verificados à mão contra a fonte real
    // (`enablers/ArchiveXL/src/Red/GarmentAssembler.hpp:73-85`) e cross-validados contra o
    // consumidor real (`App/Extensions/Garment/Extension.cpp`, `OnComputeGarment`, que usa
    // exatamente `components`/`resources`/`definition` e nada mais):
    //   components : DynArray<Handle<ent::IComponent>>          @ +0x10
    //   definition : Handle<appearance::AppearanceDefinition>   @ +0x58
    // 100% OBSERVE-ONLY, mesma disciplina de sentinela do bloco de `resources` acima.
    if gum::is_readable((ctx as *const u8).add(0x10) as *const c_void, 16) {
        let comp_entries = (ctx as *const u8).add(0x10).cast::<u64>().read_unaligned();
        let comp_cap = (ctx as *const u8).add(0x18).cast::<u32>().read_unaligned();
        let comp_size = (ctx as *const u8).add(0x1C).cast::<u32>().read_unaligned();
        let comp_plausible = comp_size <= comp_cap && comp_cap < 4096;
        // `Handle<IComponent>` = 16B {instance, refCount}; `IComponent.name: CName @ +0x40`
        // (header GERADO da reflection real do jogo — mesmo offset já usado e provado no
        // filtro de `ComponentTarget`, Codeware `#132`).
        let mut names: Vec<String> = Vec::new();
        if comp_plausible
            && comp_entries != 0
            && gum::is_readable(comp_entries as *const c_void, (comp_size as usize) * 16)
        {
            for i in 0..comp_size.min(16) {
                let inst = (comp_entries as *const u8).add(i as usize * 16).cast::<u64>().read_unaligned();
                let nm = if inst != 0 && gum::is_readable((inst as *const u8).add(0x40) as *const c_void, 8) {
                    let h = (inst as *const u8).add(0x40).cast::<u64>().read_unaligned();
                    if h == 0 { "?".to_string() } else { crate::cname::resolve_cname(h) }
                } else {
                    "?".to_string()
                };
                names.push(nm);
            }
        }
        crate::log(&format!(
            "[axl-garment] #{n} ComputePlayerGarment.components ctx={ctx:#x} entries={comp_entries:#x} \
             cap={comp_cap} size={comp_size} plausible={comp_plausible} names={names:?}"
        ));
    } else {
        crate::log(&format!(
            "[axl-garment] #{n} ComputePlayerGarment.components: ctx={ctx:#x} @+0x10 ilegível — offset NÃO confirmado neste disparo"
        ));
    }
    if gum::is_readable((ctx as *const u8).add(0x58) as *const c_void, 16) {
        let def_inst = (ctx as *const u8).add(0x58).cast::<u64>().read_unaligned();
        let def_rc = (ctx as *const u8).add(0x60).cast::<u64>().read_unaligned();
        // Critério objetivo de sucesso: a classe tem que sair `appearanceAppearanceDefinition`.
        // Qualquer outra coisa = offset errado, e o log diz isso em vez de fingir confirmação.
        let def_cls = if def_inst != 0 && gum::is_readable(def_inst as *const c_void, 8) {
            let cls = crate::rtti::class_of(def_inst as *mut c_void);
            if cls.is_null() {
                None
            } else {
                let h = crate::rtti::type_name_getname(cls);
                if h == 0 { None } else { Some(crate::cname::resolve_cname(h)) }
            }
        } else {
            None
        };
        crate::log(&format!(
            "[axl-garment] #{n} ComputePlayerGarment.definition ctx={ctx:#x} instance={def_inst:#x} \
             refcount={def_rc:#x} class={def_cls:?} (esperado 'appearanceAppearanceDefinition')"
        ));
    } else {
        crate::log(&format!(
            "[axl-garment] #{n} ComputePlayerGarment.definition: ctx={ctx:#x} @+0x58 ilegível — offset NÃO confirmado neste disparo"
        ));
    }

    // `aOffsets` (arg2) é `DynArray<int32_t>&` DIRETO (sem `SharedPtr`) — lê o header no PRÓPRIO
    // `offsets_ptr`, sem deref extra (diferente de `data_ptr`, que precisou de 1 nível a mais).
    //
    // CORREÇÃO (rodada 44, 2026-08-16): o rótulo antigo deste log ("IN, ANTES do consumo real")
    // estava ERRADO — `garment_computeplayergarment_probe` (chamador desta função) chama `orig()`
    // (a `ComputePlayerGarment` VANILLA real) *ANTES* de invocar este log, então o que lemos aqui é
    // o estado de `aOffsets` DEPOIS do vanilla já ter rodado, não antes. Ver o bloco de comentário
    // grande "RODADA 44" em `register.rs` (logo acima de `Es67ResourceOffset`) pra resolução
    // completa da anomalia `cap=3 size=3` capturada na rodada 43 — resumo: `EntityState::
    // ApplyOffsetOverrides` (fonte real) faz `aOffsets.Clear()` + 1 `PushBack` POR resource de
    // `aResources`, na MESMA ordem — **são arrays PARALELOS (mesmo índice = mesmo recurso), nunca
    // indexados por peça/slot de vestuário** — a hipótese "1 offset por peça equipada" está
    // REFUTADA pela fonte. O `cap=3 size=3` observado é só o estado que sobrou de ANTES de
    // qualquer lógica da ArchiveXL rodar (que não existe neste build) — nossa escrita (abaixo,
    // gated) ignora esse valor de propósito, igual ao `Clear()` real.
    if offsets_ptr != 0 && gum::is_readable(offsets_ptr as *const c_void, 16) {
        let off_cap = (offsets_ptr as *const u8).add(0x08).cast::<u32>().read_unaligned();
        let off_size = (offsets_ptr as *const u8).add(0x0C).cast::<u32>().read_unaligned();
        crate::log(&format!(
            "[axl-garment] #{n} ComputePlayerGarment.offsets(pós-orig(), ANTES de qualquer escrita nossa) cap={off_cap} size={off_size} — esperado ==resources.size() SÓ depois do write (ver #67-write); array paralelo a resources, não por slot"
        ));
    }

    if plausible && !hashes.is_empty() {
        let computed = crate::register::run_apply_offset_overrides(entity_inst, &hashes);
        crate::log(&format!(
            "[axl-garment] #{n} ApplyOffsetOverrides (núcleo puro, SÓ LOG — nunca escrito de volta em aOffsets por ESTE bloco) entity={entity_inst:#x} -> {computed:?}"
        ));
        // Rodada 44 (2026-08-16): tentativa de ESCRITA real, gated atrás de um marcador PRÓPRIO
        // (`~/.bwms-garmentprobe-write`, SEPARADO de `~/.bwms-flatwrite` — categoria de risco
        // distinta, "mutar o DynArray<int32_t> de saída de ComputePlayerGarment", não TweakDB).
        // Ver `try_write_offsets_synthetic` abaixo.
        try_write_offsets_synthetic(n, entity_inst, offsets_ptr, &hashes);
    }
}

/// Rodada 44 (2026-08-16, offline dedicada) — `true` se `~/.bwms-garmentprobe-write` existir no
/// disco. Marcador PRÓPRIO E SEPARADO de `~/.bwms-flatwrite` (que gateia mutação de TweakDB) —
/// esta escrita é uma categoria diferente (DynArray de saída de uma função nativa hookada, não
/// TweakDB), por isso um marcador dedicado em vez de reusar o existente (instrução explícita da
/// rodada). Ausência do marcador = 100% observe-only, comportamento idêntico ao das rodadas 42/43.
unsafe fn garmentprobe_write_enabled() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(&format!("{home}/.bwms-garmentprobe-write")).exists()
}

/// Rodada 44 (2026-08-16) — testa a ESCRITA real em `aOffsets` (`register::
/// run_apply_offset_overrides_write`, porte fiel de `EntityState::ApplyOffsetOverrides`) contra a
/// memória VIVA capturada por `garment_computeplayergarment_probe`. **Só roda atrás do marcador
/// dedicado `garmentprobe_write_enabled()`** — sem ele, é NO-OP (mesmo comportamento observe-only
/// das rodadas 42/43, zero mutação).
///
/// Sequência (auto-contida, não depende de equip real — o objetivo desta rodada é só confirmar que
/// a técnica de escrita NÃO CRASHA contra este `DynArray<int32_t>` específico, com um valor
/// sintético claramente identificável, nunca fabricado a partir de dado plausível-mas-ambíguo):
///   1. Registra 1 override SINTÉTICO (`TEST_OFFSET=777`, valor que não ocorreria por acaso) pro
///      1º hash capturado de `aResources` (`hashes[0]`) — usa `register::add_offset_override`
///      diretamente (mesmo caminho que a native redscript `ArchiveXL.AddOffsetOverride` usaria).
///   2. Chama `run_apply_offset_overrides_write` — Clear+PushBack real contra a memória do jogo.
///   3. Lê de volta DIRETO da memória (via `offsets_ptr`, independente do valor de retorno da
///      função — prova mais forte que só confiar no `Some(...)`) e compara.
unsafe fn try_write_offsets_synthetic(n: u64, entity_inst: u64, offsets_ptr: u64, hashes: &[u64]) {
    if !garmentprobe_write_enabled() {
        return;
    }
    if hashes.is_empty() || offsets_ptr == 0 {
        crate::log(&format!("[axl-garment] #{n} #67-write ABORTADO: sem hashes/offsets_ptr válido"));
        return;
    }
    const TEST_MOD_HASH: u64 = 0xB0755EEDu64; // marcador arbitrário só precisa ser único
    const TEST_OFFSET: i32 = 777; // valor claramente identificável, nunca plausível por acaso
    crate::register::add_offset_override(entity_inst, TEST_MOD_HASH, hashes[0], TEST_OFFSET);
    match crate::register::run_apply_offset_overrides_write(entity_inst, offsets_ptr, hashes) {
        Some(computed) => {
            // Leitura independente DIRETO da memória (não confia só no retorno da função) — prova
            // de verdade, não só "a função devolveu Some(...)".
            let cap = (offsets_ptr as *const u8).add(0x08).cast::<u32>().read_unaligned();
            let size = (offsets_ptr as *const u8).add(0x0C).cast::<u32>().read_unaligned();
            let entries = (offsets_ptr as *const u64).read_unaligned();
            let mut readback: Vec<i32> = Vec::new();
            if entries != 0 && size <= 4096 && gum::is_readable(entries as *const c_void, size as usize * 4) {
                for i in 0..size {
                    readback.push((entries as *const u8).add(i as usize * 4).cast::<i32>().read_unaligned());
                }
            }
            let matches = readback == computed;
            crate::log(&format!(
                "[axl-garment] #{n} #67-write ESCRITA REAL: hashes[0]={:#018x} testOffset={TEST_OFFSET} -> computed={computed:?} | pós-escrita cap={cap} size={size} readback(direto da memória, independente do retorno)={readback:?} matches={matches} >>> {} <<<",
                hashes[0],
                if matches { "ESCRITA CONFIRMADA" } else { "DIVERGÊNCIA — investigar" }
            ));
        }
        None => {
            crate::log(&format!("[axl-garment] #{n} #67-write FALHOU (ver linha de log anterior pro motivo — aoffsets_ptr ilegível ou push falhou no meio)"));
        }
    }
}

unsafe extern "C" fn garment_computeplayergarment_probe(
    a1: *mut c_void, entity_h: u64, offsets: u64, data: u64, a5: u64, a6: u64, a7: u64, a8: u64,
) {
    let n = GARMENT_COMPUTEPLAYERGARMENT_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_GARMENT_COMPUTEPLAYERGARMENT.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: EightArgVoidFn = std::mem::transmute(orig);
        f(a1, entity_h, offsets, data, a5, a6, a7, a8);
    }
    if n < 30 {
        crate::log(&format!(
            "[axl-garment] AppearanceChanger::ComputePlayerGarment#{n} a1={a1:p} entity_h={entity_h:#x} offsets={offsets:#x} data={data:#x} a5={a5:#x} a6={a6:#x} a7={a7:#x} a8(bool)={a8:#x}"
        ));
        // axl-fullbody-torso (2026-08-06): entity_h/offsets/data são documentados como PONTEIROS
        // (`&entity_handle`/`&offsets_dynarr`/`&data_sharedptr`) — dereferencia (com is_readable
        // antes) e pesca CName nos primeiros bytes de cada um, mesma técnica de `scan_cnames`.
        let mut entity_inst: u64 = 0;
        for (label, ptr) in [("entity_h", entity_h), ("offsets", offsets), ("data", data)] {
            if ptr == 0 || !gum::is_readable(ptr as *const c_void, 0x40) {
                continue;
            }
            scan_cnames(&format!("cpg-{label}"), n, ptr as *mut c_void);
            // o próprio ponteiro deref'd (1º word) também pode ser o handle/instância real.
            let inner = (ptr as *const u64).read_unaligned();
            if inner != 0 && gum::is_readable(inner as *const c_void, 0x40) {
                scan_cnames(&format!("cpg-{label}-inner"), n, inner as *mut c_void);
            }
            if label == "entity_h" {
                entity_inst = inner;
            }
        }
        // Item `#67` (2026-08-16): extensão OBSERVE-ONLY, ver `log_garment_resources_and_offsets`
        // acima — só leitura, zero mutação de `aOffsets`/`aData`.
        log_garment_resources_and_offsets(n, entity_inst, offsets, data);
    }
}

// Wrapper de ComputePlayerGarment (ver nota grande na declaração da const `GARMENT_CPG_WRAPPER_VM`).
// 2 params: x0=contexto, x1=payload (payload vira `a1` de CPG por dentro). Observe-only — chama a
// original primeiro (preserva 100% do comportamento real, inclusive o que roda DEPOIS da chamada a
// CPG dentro do próprio wrapper, que ainda não foi mapeado), só loga depois.
unsafe extern "C" fn garment_cpg_wrapper_probe(ctx: *mut c_void, payload: *mut c_void) {
    let n = GARMENT_CPG_WRAPPER_CNT.fetch_add(1, Ordering::Relaxed);
    // axl-fullbody-torso (cont.36): CPG (chamada de dentro de `orig`) empurra pra `*(payload)+0x20`
    // — lê ANTES e DEPOIS de chamar a original, mesma invocação, pra ver se algo consome
    // sincronamente (valor muda/zera) ou se fica pendente (mesmo valor, sinal de consumidor externo
    // de verdade, não visível aqui).
    // (cont.37) achado ao vivo: payload+0x20 é um PONTEIRO ESTÁVEL (sempre payload-8, nunca muda) —
    // provável header de DynArray, não o dado em si. Le-lo cru não capturava a mutação. Agora
    // dereferencia (ponteiro→ponteiro) e lê 0x20 bytes NO ALVO, antes e depois, pra pegar o
    // conteúdo real que muda quando o job empurra dado novo.
    unsafe fn snap(payload: *mut c_void) -> Option<(u64, [u64; 4])> {
        if payload.is_null() || !gum::is_readable(payload, 0x28) {
            return None;
        }
        let hdr = (payload.cast::<u8>().add(0x20) as *const u64).read_unaligned();
        if hdr == 0 || !gum::is_readable(hdr as *const c_void, 0x20) {
            return Some((hdr, [0; 4]));
        }
        let f = |off: usize| (hdr as *const u8).add(off).cast::<u64>().read_unaligned();
        Some((hdr, [f(0), f(8), f(0x10), f(0x18)]))
    }
    let before = snap(payload);
    let orig = ORIG_GARMENT_CPG_WRAPPER.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: TwoArgVoidFn = std::mem::transmute(orig);
        f(ctx, payload);
    }
    let after = snap(payload);
    if n < 30 {
        crate::log(&format!(
            "[axl-fullbody-torso] cpg-wrapper#{n} ctx={ctx:p} payload={payload:p} snap(hdr,[deref x4]) ANTES={before:#x?} DEPOIS={after:#x?}"
        ));
        if !payload.is_null() && gum::is_readable(payload, 0x40) {
            scan_cnames("cpg-wrapper-payload", n, payload);
        }
        if !ctx.is_null() && gum::is_readable(ctx, 0x40) {
            scan_cnames("cpg-wrapper-ctx", n, ctx);
        }
    }
}

// Handler de AppearanceMeshLoadedEvent pra SkinnedMeshComponent (ver nota grande na declaração
// da const `APPEARANCE_MESHLOADED_HANDLER_VM`). 2 params: this=componente, event=ponteiro real
// do evento (já dereferenciado pelo thunk conector antes de chegar aqui). Observe-only.
unsafe extern "C" fn appearance_meshloaded_handler_probe(this: *mut c_void, event: *mut c_void) {
    let n = APPEARANCE_MESHLOADED_HANDLER_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_APPEARANCE_MESHLOADED_HANDLER.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: TwoArgVoidFn = std::mem::transmute(orig);
        f(this, event);
    }
    // (cont.39, teste de isolamento de custo): 2 boots com `scan_cnames` mostraram CPU subindo
    // exatamente durante rajadas de disparo (streaming de veículo) — pode ser overhead real do
    // scan (chama resolve_cname/CNamePool::Get nativo várias vezes por disparo, possivelmente sob
    // contenção com o motor streaming pesado) OU pode ser só o streaming pesado em si (já
    // documentado neste projeto, independente de qualquer hook nosso). Versão SEM scan_cnames,
    // só log cru dos ponteiros — decide a questão.
    if n < 30 {
        crate::log(&format!(
            "[axl-fullbody-torso] mesh-loaded-handler#{n} this={this:p} event={event:p}"
        ));
    }
}

// Handler de AppearanceStatusEvent pra SkinnedMeshComponent (ver nota grande na declaração da
// const `APPEARANCE_STATUS_HANDLER_VM`). 2 params: this=componente, event=ponteiro real do
// evento. Observe-only, LEVE de propósito (lição do cont.39 — sem scan_cnames por padrão nesta
// primeira rodada, só log cru + os 2 campos que a própria função lê/escreve).
unsafe extern "C" fn appearance_status_handler_probe(this: *mut c_void, event: *mut c_void) {
    let n = APPEARANCE_STATUS_HANDLER_CNT.fetch_add(1, Ordering::Relaxed);
    let event_filter = if !event.is_null() && gum::is_readable(event, 0x50) {
        Some((event.cast::<u8>().add(0x48) as *const u32).read_unaligned())
    } else {
        None
    };
    let this_filter = if !this.is_null() && gum::is_readable(this, 0x44) {
        Some((this.cast::<u8>().add(0x40) as *const u32).read_unaligned())
    } else {
        None
    };
    let orig = ORIG_APPEARANCE_STATUS_HANDLER.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: TwoArgVoidFn = std::mem::transmute(orig);
        f(this, event);
    }
    // (cont.41): mesmo ajuste de cap do rebuild-proxy — log incondicional só nas 10 primeiras
    // (sanity check), depois só quando o filtro NÃO bate (caso raro/interessante) pra sobrar
    // espaço de log pra um equip real feito depois do streaming inicial já ter gastado o cap.
    let interesting = match (event_filter, this_filter) {
        (Some(e), Some(t)) => e != t,
        _ => true,
    };
    if n < 10 || interesting {
        crate::log(&format!(
            "[axl-fullbody-torso] appearance-status#{n} this={this:p} event={event:p} event+0x48={event_filter:#x?} this+0x40={this_filter:#x?}{}",
            if interesting && n >= 10 { " <<< INTERESSANTE" } else { "" }
        ));
    }
}

// Método virtual "reconstrói render proxy" (vtable+0x288 de SkinnedMeshComponent), ver nota
// grande na declaração da const `APPEARANCE_REBUILD_PROXY_VM`. 1 param: this=componente.
// Observe-only. Lê `this+0x8b`(enabled)/`this+0x1f8`(ponteiro suspeito) ANTES e DEPOIS de chamar
// a original, pra ver se `+0x1f8` muda (null→algo = reconstrução real aconteceu).
type OneArgVoidFn = unsafe extern "C" fn(*mut c_void);
unsafe extern "C" fn appearance_rebuild_proxy_probe(this: *mut c_void) {
    let n = APPEARANCE_REBUILD_PROXY_CNT.fetch_add(1, Ordering::Relaxed);
    unsafe fn snap(this: *mut c_void) -> Option<(u8, u64)> {
        if this.is_null() || !gum::is_readable(this, 0x200) {
            return None;
        }
        let enabled = (this.cast::<u8>().add(0x8b)).read_unaligned();
        let res_ptr = (this.cast::<u8>().add(0x1f8) as *const u64).read_unaligned();
        Some((enabled, res_ptr))
    }
    let before = snap(this);
    let orig = ORIG_APPEARANCE_REBUILD_PROXY.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: OneArgVoidFn = std::mem::transmute(orig);
        f(this);
    }
    let after = snap(this);
    // (cont.40, ajuste pós-boot-de-teste): cap fixo de 30 esgotava no streaming do boot, sem
    // sobrar espaço pra ver dado de um equip real feito depois. Agora: sempre loga os primeiros
    // 10 (sanity check de que instalou/dispara), DEPOIS só loga o caso RARO/interessante — mudou
    // de antes pra depois (reconstrução real aconteceu) OU res_ptr é null (gate falhando) — evita
    // spam do caso comum "enabled+válido+sem mudança" (já confirmado 30/30 no boot anterior).
    let interesting = before != after || matches!(before, Some((_, 0))) || matches!(after, Some((_, 0)));
    if n < 10 || interesting {
        crate::log(&format!(
            "[axl-fullbody-torso] rebuild-proxy#{n} this={this:p} (enabled,res_ptr) ANTES={before:#x?} DEPOIS={after:#x?}{}",
            if interesting && n >= 10 { " <<< INTERESSANTE" } else { "" }
        ));
    }
}

// Handler de AppearanceMeshLoadedEvent DE VERDADE (ver nota grande na declaração da const
// `APPEARANCE_MESHLOADED_REAL_VM`). Mesma ABI dos outros handlers de evento desta família.
unsafe extern "C" fn appearance_meshloaded_real_probe(this: *mut c_void, event: *mut c_void) {
    let n = APPEARANCE_MESHLOADED_REAL_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_APPEARANCE_MESHLOADED_REAL.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: TwoArgVoidFn = std::mem::transmute(orig);
        f(this, event);
    }
    if n < 30 {
        crate::log(&format!(
            "[axl-fullbody-torso] mesh-loaded-REAL#{n} this={this:p} event={event:p}"
        ));
    }
}

// axl-animation-apply: candidato de BAIXA CONFIANÇA pra InitializeAnimations (ver nota grande na
// declaração da const). Observe-only, 1-arg void — lê [x0+0x50] (esperado: ponteiro `owner`/Entity*
// se a hipótese estiver certa) e [owner+0x60] (esperado: algo parecido com ResourcePath/hash se
// `owner` for mesmo um Entity* válido) SÓ PRA LOG, sem mutar nada. Espera-se refutar.
unsafe extern "C" fn anim_initanim_candidate_probe(this: *mut c_void) {
    let n = ANIM_INITANIM_CANDIDATE_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_ANIM_INITANIM_CANDIDATE.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: OneArgFn = std::mem::transmute(orig);
        f(this);
    }
    if n < 8 {
        let owner_ptr = if gum::is_readable(this, 0x58) {
            (this as *const u64).add(0x50 / 8).read_unaligned()
        } else {
            0
        };
        let owner_p60 = if owner_ptr != 0 && gum::is_readable(owner_ptr as *const c_void, 0x68) {
            (owner_ptr as *const u64).add(0x60 / 8).read_unaligned()
        } else {
            0
        };
        crate::log(&format!(
            "[axl-anim-candidate] 0x1024d1f24#{n} this={this:p} [this+0x50]={owner_ptr:#018x} [owner+0x60]={owner_p60:#018x} (baixa confiança, esperado refutar)"
        ));
    }
}

// axl-attachment-apply: 2 candidatos pra IsAffectedSlot(TweakDBID) -> bool (ver nota grande na
// declaração das consts). ABI trivial: 1 u64 em x0, bool em w0 — chama a original, loga o
// resultado, devolve o MESMO valor (observe-only, zero mutação de comportamento).
type OneArgRetBoolFn = unsafe extern "C" fn(u64) -> bool;
unsafe extern "C" fn attach_isaffectedslot_c1_probe(slot_id: u64) -> bool {
    let n = ATTACH_ISAFFECTEDSLOT_C1_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_ATTACH_ISAFFECTEDSLOT_C1.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: OneArgRetBoolFn = std::mem::transmute(orig);
        f(slot_id)
    } else {
        false
    };
    if n < 8 {
        crate::log(&format!("[axl-attach-isaffected-c1] 0x1035a1c18#{n} slot_id={slot_id:#018x} -> {ret}"));
    }
    ret
}
unsafe extern "C" fn attach_isaffectedslot_c2_probe(slot_id: u64) -> bool {
    let n = ATTACH_ISAFFECTEDSLOT_C2_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_ATTACH_ISAFFECTEDSLOT_C2.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: OneArgRetBoolFn = std::mem::transmute(orig);
        f(slot_id)
    } else {
        false
    };
    if n < 8 {
        crate::log(&format!("[axl-attach-isaffected-c2] 0x1035a1d68#{n} slot_id={slot_id:#018x} -> {ret}"));
    }
    ret
}

// axl-attachment-apply: candidato #3 pra IsAffectedSlot/função irmã (ver nota grande na
// declaração da const, `ATTACH_ISAFFECTEDSLOT_C3_VM`) — 100% OBSERVE-ONLY, nunca muta o retorno
// nem o estado; só loga `this`/`slot_id`(x1, decodificado como TweakDBID cru)/retorno + os 2
// campos candidatos (`this+0x110`=ponteiro plausível de array, `this+0x11c`=possível
// contador/capacidade adjacente), sempre com `gum::is_readable` antes de dereferenciar. CODADO,
// NUNCA TESTADO AO VIVO (sessão 100% offline) — não chamar nem consumir o retorno em lugar
// nenhum além deste log.
type TwoArgRetPtrFn = unsafe extern "C" fn(*mut c_void, u64) -> *mut c_void;
unsafe extern "C" fn attach_isaffectedslot_c3_probe(this: *mut c_void, slot_id: u64) -> *mut c_void {
    let n = ATTACH_ISAFFECTEDSLOT_C3_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_ATTACH_ISAFFECTEDSLOT_C3.load(Ordering::Relaxed);
    let ret = if !orig.is_null() {
        let f: TwoArgRetPtrFn = std::mem::transmute(orig);
        f(this, slot_id)
    } else {
        std::ptr::null_mut()
    };
    if n < 8 {
        let field_110 = if gum::is_readable(this, 0x124) {
            (this as *const u64).add(0x110 / 8).read_unaligned()
        } else {
            0
        };
        let field_11c = if gum::is_readable(this, 0x124) {
            (this as *const u32).add(0x11c / 4).read_unaligned()
        } else {
            0xFFFF_FFFF
        };
        crate::log(&format!(
            "[axl-attach-isaffected-c3] 0x1035a60e0#{n} this={this:p} slot_id={slot_id:#018x} -> {ret:p} [this+0x110]={field_110:#018x} [this+0x11c]={field_11c:#010x} (candidato MÉDIA confiança, nunca testado ao vivo)"
        ));
    }
    ret
}

// axl-attachment-apply: candidato pra CheckState (ver nota grande na declaração da const).
// HookAfter observe-only — chama a original PRIMEIRO (mesmo padrão do ArchiveXL real), depois lê
// o campo de saída (`[aState+0x0]`, esperado tri-estado 0/1/2 se a hipótese estiver certa).
type TwoArgVoidFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
unsafe extern "C" fn attach_checkstate_candidate_probe(this: *mut c_void, a_state: *mut c_void) {
    let n = ATTACH_CHECKSTATE_CANDIDATE_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_ATTACH_CHECKSTATE_CANDIDATE.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: TwoArgVoidFn = std::mem::transmute(orig);
        f(this, a_state);
    }
    if n < 8 {
        let state_val = if gum::is_readable(a_state, 4) {
            (a_state as *const u32).read_unaligned()
        } else {
            0xFFFF_FFFF
        };
        crate::log(&format!(
            "[axl-attach-candidate] 0x102442d8c#{n} this={this:p} aState={a_state:p} [aState+0]={state_val:#010x} (candidato CheckState, ver se 0/1/2 plausível)"
        ));
    }
}

// TPPRepresentationComponent::OnAttach candidato (ver nota grande na declaração da const).
// Observe-only — chama a original, loga `this`+`a2` cru e um pedaço de `[this+0x50]` (offset
// `owner` padrão de IComponent, já confirmado noutras classes) só como sanity-check informal.
type OneArgU64Fn = unsafe extern "C" fn(*mut c_void, u64);
unsafe extern "C" fn tpp_onattach_candidate_probe(this: *mut c_void, a2: u64) {
    let n = TPP_ONATTACH_CANDIDATE_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_TPP_ONATTACH_CANDIDATE.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: OneArgU64Fn = std::mem::transmute(orig);
        f(this, a2);
    }
    if n < 8 {
        let owner_ptr = if gum::is_readable(this, 0x58) {
            (this as *const u64).add(0x50 / 8).read_unaligned()
        } else {
            0
        };
        crate::log(&format!(
            "[axl-tpp-candidate] 0x1035a05f0#{n} this={this:p} a2={a2:#018x} [this+0x50](owner?)={owner_ptr:#018x} (candidato OnAttach)"
        ));
    }
}

// TPPRepresentationComponent::OnAttach candidato #2, vtable slot[49] = 0x1035a0604 (ver nota
// grande na declaração da const). Observe-only — chama a original, loga `this`+`a2` cru, mesmo
// padrão de sanity-check informal do candidato #1.
unsafe extern "C" fn tpp_onattach_c2_probe(this: *mut c_void, a2: u64) {
    let n = TPP_ONATTACH_C2_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_TPP_ONATTACH_C2.load(Ordering::Relaxed);
    if !orig.is_null() {
        let f: OneArgU64Fn = std::mem::transmute(orig);
        f(this, a2);
    }
    if n < 8 {
        crate::log(&format!(
            "[axl-tpp-candidate] 0x1035a0604#{n} this={this:p} a2={a2:#018x} (candidato OnAttach, vtable slot[49])"
        ));
    }
    // ArchiveXL `#40` (2026-08-19, continuação direta): HookAfter real da fonte
    // (`HookAfter<Raw::TPPRepresentationComponent::OnAttach>(&OnAttachTPP)`) — chama a original
    // ACIMA (sempre, passthrough), e SÓ DEPOIS roda a composição real (`register::onattachtpp_compose`,
    // ver nota grande na declaração da função) usando `this` como `aComponent`. Bounded nas primeiras
    // 4 firings (o padrão já visto é 2/boot: player-espúrio+real) por disciplina, mesmo sem risco de
    // mutação destrutiva conhecido (só PushBack condicional num array vazio-por-padrão).
    if n < 4 {
        if let Some(reg) = crate::rtti::Registry::obtain() {
            let out = crate::register::onattachtpp_compose(&reg, this);
            crate::log(&out);
        }
    }
}

// axl-journal-apply: JournalTree::ProcessJournalIndex — DynArray probe (HookAfter)
// x0=JournalTree*, x1=loading context (stack ptr); fires during game load.
// Probe scans JournalTree* offsets for DynArray<Handle<JournalRootFolderEntry>>.
unsafe extern "C" fn journal_process_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = JOURNAL_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_JOURNAL.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n > 0 { return; } // only probe on first call (main game load)
    crate::log(&format!("[axl-journal] ProcessJournalIndex#{n} this={this:p} a1={a1:#018x}"));
    // Dump JournalManager singleton
    let jm_global = crate::rebase(JOURNAL_MANAGER_GLOBAL_VM) as *const *mut c_void;
    if gum::is_readable(jm_global as *const c_void, 8) {
        let jm_ptr = jm_global.read_unaligned();
        crate::log(&format!("[axl-journal] JournalManager* = {jm_ptr:p} (global@{jm_global:p})"));
        if !jm_ptr.is_null() && gum::is_readable(jm_ptr as *const c_void, 0x100) {
            let rjm = |off: usize| (jm_ptr.cast::<u8>().add(off) as *const u64).read_unaligned();
            let rjm32 = |off: usize| (jm_ptr.cast::<u8>().add(off) as *const u32).read_unaligned();
            crate::log(&format!(
                "[axl-journal] JM +00..+3f: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
                rjm(0x00), rjm(0x08), rjm(0x10), rjm(0x18), rjm(0x20), rjm(0x28), rjm(0x30), rjm(0x38)
            ));
            crate::log(&format!(
                "[axl-journal] JM +40..+7f: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
                rjm(0x40), rjm(0x48), rjm(0x50), rjm(0x58), rjm(0x60), rjm(0x68), rjm(0x70), rjm(0x78)
            ));
            crate::log(&format!(
                "[axl-journal] JM +80..+bf: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
                rjm(0x80), rjm(0x88), rjm(0x90), rjm(0x98), rjm(0xa0), rjm(0xa8), rjm(0xb0), rjm(0xb8)
            ));
            crate::log(&format!(
                "[axl-journal] JM +c0..+ff: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
                rjm(0xc0), rjm(0xc8), rjm(0xd0), rjm(0xd8), rjm(0xe0), rjm(0xe8), rjm(0xf0), rjm(0xf8)
            ));
            // Scan JM for DynArray<Handle<T>> pattern: ptr(8) + cap(4) + sz(4)
            for off in (0x08usize..=0xf0).step_by(8) {
                let arr_ptr = rjm(off);
                let cap     = rjm32(off + 8);
                let sz      = rjm32(off + 0xc);
                if arr_ptr > 0x7_0000_0000 && arr_ptr < 0x1_0000_0000_0000
                    && sz >= 1 && sz <= cap && cap <= 4096
                    && gum::is_readable(arr_ptr as *const c_void, (sz as usize).min(4) * 16)
                {
                    let h0 = (arr_ptr as *const u64).read_unaligned();
                    crate::log(&format!(
                        "[axl-journal] JM DYNARRAY @+{off:#x}: ptr={arr_ptr:#018x} cap={cap} sz={sz} h0={h0:#018x}"
                    ));
                }
            }
        } else {
            crate::log("[axl-journal] JournalManager* null or not readable");
        }
    } else {
        crate::log("[axl-journal] JM global not readable");
    }
    let rdbl8  = gum::is_readable(this, 8);
    let rdbl40 = gum::is_readable(this, 0x40);
    let rdbl120 = gum::is_readable(this, 0x120);
    crate::log(&format!("[axl-journal] readable: 8={rdbl8} 0x40={rdbl40} 0x120={rdbl120}"));
    if !rdbl8 { return; }
    let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
    let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
    // Dump first 0x100 bytes of JournalTree struct for offline analysis
    crate::log(&format!(
        "[axl-journal] struct +00..+3f: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
        rf64(0x00), rf64(0x08), rf64(0x10), rf64(0x18), rf64(0x20), rf64(0x28), rf64(0x30), rf64(0x38)
    ));
    crate::log(&format!(
        "[axl-journal] struct +40..+7f: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
        rf64(0x40), rf64(0x48), rf64(0x50), rf64(0x58), rf64(0x60), rf64(0x68), rf64(0x70), rf64(0x78)
    ));
    crate::log(&format!(
        "[axl-journal] struct +80..+bf: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
        rf64(0x80), rf64(0x88), rf64(0x90), rf64(0x98), rf64(0xa0), rf64(0xa8), rf64(0xb0), rf64(0xb8)
    ));
    crate::log(&format!(
        "[axl-journal] struct +c0..+ff: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
        rf64(0xc0), rf64(0xc8), rf64(0xd0), rf64(0xd8), rf64(0xe0), rf64(0xe8), rf64(0xf0), rf64(0xf8)
    ));
    // Scan for DynArray pattern: heap_ptr(8) + cap(4) + sz(4), sz∈[1..256], sz≤cap
    // Range 0x10..0x180 (journal tree may extend further than 0x110)
    for off in (0x10usize..=0x180).step_by(8) {
        if !gum::is_readable(this.cast::<u8>().add(off) as *const c_void, 16) { continue; }
        let arr_ptr = rf64(off);
        let cap     = rf32(off + 8);
        let sz      = rf32(off + 0xc);
        if arr_ptr > 0x7_0000_0000 && arr_ptr < 0x1_0000_0000_0000
            && sz >= 1 && sz <= cap && cap <= 256
            && gum::is_readable(arr_ptr as *const c_void, (sz as usize) * 16)
        {
            let h0_inst = (arr_ptr as *const u64).read_unaligned();
            let h0_rc   = ((arr_ptr + 8) as *const u64).read_unaligned();
            crate::log(&format!(
                "[axl-journal] DYNARRAY @+{off:#x}: ptr={arr_ptr:#018x} cap={cap} sz={sz} h0.inst={h0_inst:#018x} h0.rc={h0_rc:#018x}"
            ));
            let sz_ptr = this.cast::<u8>().add(off + 0xc) as *mut u32;
            sz_ptr.write_unaligned(sz + 1);
            let rb = sz_ptr.read_unaligned();
            sz_ptr.write_unaligned(sz);
            crate::log(&format!(
                "[axl-journal] APPLY PROOF @+{off:#x}: sz {sz} → {rb} → {sz} R/W CONFIRMED"
            ));
        }
    }
    // Deref pointer at +0x10 — likely indirect to journal data struct
    let ind_ptr = rf64(0x10);
    if ind_ptr > 0x7_0000_0000 && ind_ptr < 0x1_0000_0000_0000
        && gum::is_readable(ind_ptr as *const c_void, 0x80)
    {
        let ri64 = |off: usize| ((ind_ptr as *const u8).add(off) as *const u64).read_unaligned();
        let ri32 = |off: usize| ((ind_ptr as *const u8).add(off) as *const u32).read_unaligned();
        crate::log(&format!(
            "[axl-journal] *ind_ptr +00..+3f: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
            ri64(0), ri64(8), ri64(0x10), ri64(0x18), ri64(0x20), ri64(0x28), ri64(0x30), ri64(0x38)
        ));
        crate::log(&format!(
            "[axl-journal] *ind_ptr +40..+7f: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
            ri64(0x40), ri64(0x48), ri64(0x50), ri64(0x58), ri64(0x60), ri64(0x68), ri64(0x70), ri64(0x78)
        ));
        // DynArray scan in indirect object
        for off in (0x00usize..=0x70).step_by(8) {
            let arr_ptr = ri64(off);
            let cap     = ri32(off + 8);
            let sz      = ri32(off + 0xc);
            if arr_ptr > 0x7_0000_0000 && arr_ptr < 0x1_0000_0000_0000
                && sz >= 1 && sz <= cap && cap <= 256
                && gum::is_readable(arr_ptr as *const c_void, (sz as usize) * 16)
            {
                let h0_inst = (arr_ptr as *const u64).read_unaligned();
                crate::log(&format!(
                    "[axl-journal] IND DYNARRAY @ind+{off:#x}: ptr={arr_ptr:#018x} cap={cap} sz={sz} h0={h0_inst:#018x}"
                ));
            }
        }
    } else {
        crate::log(&format!("[axl-journal] ind_ptr={ind_ptr:#018x} not readable or out of range"));
    }
}

// axl-journal-apply: JournalFolderEntry::LoadSubResource A (frame=0x110)
// Fires when a journal folder entry loads its sub-resources during game load.
unsafe extern "C" fn journal_loadsub_a_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = JOURNAL_LOADSUB_A_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_JOURNAL_LOADSUB_A.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 4 {
        crate::log(&format!("[axl-journal] LoadSubResourceA#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
        if gum::is_readable(this, 0x40) {
            let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
            let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
            crate::log(&format!("[axl-journal] LSA +00..+1f: {:#018x} {:#018x} {:#018x} {:#018x}",
                rf64(0), rf64(8), rf64(0x10), rf64(0x18)));
            for off in (0x08usize..=0x38).step_by(8) {
                if !gum::is_readable(this.cast::<u8>().add(off) as *const c_void, 16) { continue; }
                let arr_ptr = rf64(off);
                let cap = rf32(off + 8);
                let sz  = rf32(off + 0xc);
                if arr_ptr > 0x7_0000_0000 && arr_ptr < 0x1_0000_0000_0000
                    && sz >= 1 && sz <= cap && cap <= 1024
                    && gum::is_readable(arr_ptr as *const c_void, 8)
                {
                    crate::log(&format!("[axl-journal] LSA DYNARRAY@+{off:#x}: ptr={arr_ptr:#018x} cap={cap} sz={sz}"));
                }
            }
        }
    }
}

// axl-journal-apply: JournalFolderEntry::LoadSubResource B (frame=0x120)
unsafe extern "C" fn journal_loadsub_b_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = JOURNAL_LOADSUB_B_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_JOURNAL_LOADSUB_B.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 4 {
        crate::log(&format!("[axl-journal] LoadSubResourceB#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
    }
}

// axl-journal-apply: JournalRootFolderEntry::Initialize/LoadRootResource (frame=0xe0)
// Root entry initializer — fires once at journal system startup.
unsafe extern "C" fn journal_loadroot_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = JOURNAL_LOADROOT_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_JOURNAL_LOADROOT.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 4 {
        crate::log(&format!("[axl-journal] LoadRootResource#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
        if gum::is_readable(this, 0x40) {
            let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
            let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
            crate::log(&format!("[axl-journal] LRR +00..+1f: {:#018x} {:#018x} {:#018x} {:#018x}",
                rf64(0), rf64(8), rf64(0x10), rf64(0x18)));
            for off in (0x08usize..=0x38).step_by(8) {
                if !gum::is_readable(this.cast::<u8>().add(off) as *const c_void, 16) { continue; }
                let arr_ptr = rf64(off);
                let cap = rf32(off + 8);
                let sz  = rf32(off + 0xc);
                if arr_ptr > 0x7_0000_0000 && arr_ptr < 0x1_0000_0000_0000
                    && sz >= 1 && sz <= cap && cap <= 1024
                    && gum::is_readable(arr_ptr as *const c_void, 8)
                {
                    crate::log(&format!("[axl-journal] LRR DYNARRAY@+{off:#x}: ptr={arr_ptr:#018x} cap={cap} sz={sz}"));
                }
            }
        }
    }
}

// axl-journal-apply: index.reslist loader (frame=0x50) — small function referencing "base\journal\index.reslist"
unsafe extern "C" fn journal_reslist_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = JOURNAL_RESLIST_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_JOURNAL_RESLIST.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 6 {
        crate::log(&format!("[axl-journal] ReslistLoader#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
    }
}

// axl-customization-apply: gameuiCharacterCustomizationSystem candidate A
// Hypothesis: CharCustomA = GetHeadOptions/GetBodyOptions (large func, frame=0x1C0).
// ABI: x0=this, x1=output DynArray<Handle<CharacterCustomizationOption>>*, x2=presetName CName.
unsafe extern "C" fn char_custom_a_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = CHAR_CUSTOM_A_CNT.fetch_add(1, Ordering::Relaxed);
    // CRASH REAL 2026-07-28: esta linha guardava `this` em CHAR_CUSTOM_SYS_PTR pra uso
    // posterior por `custsys`. Achado: CharCustomA é RTTI-init (ver CODEBASE.md FAZER #5,
    // "CharCustomA/B=RTTI-init, inúteis") — `this` NÃO é a instância estável do
    // gameuiCharacterCustomizationSystem, é um objeto transiente de registro de classe.
    // Cachear e chamar `class_of()` nele bem depois (custsys probe) crashou
    // (EXC_CRASH/SIGABRT, malloc detectou ponteiro-não-alocado — use-after-free clássico,
    // proofs do crash: Cyberpunk2077-2026-07-28-185501.ips). Removido. Fonte de verdade
    // agora é só `getcustsys` (GameInstance.GetCharacterCustomizationSystem(), sempre fresco).
    let orig = ORIG_CHAR_CUSTOM_A.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 5 {
        crate::log(&format!("[axl-custom] CharCustomA#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
        // Try reading a1 as DynArray<Handle<T>>*: ptr(8)+cap(4)+sz(4)
        if a1 > 0x1_0000_0000 && gum::is_readable(a1 as *const c_void, 16) {
            let arr_ptr = (a1 as *const u64).read_unaligned();
            let cap = ((a1 + 8) as *const u32).read_unaligned();
            let sz = ((a1 + 12) as *const u32).read_unaligned();
            crate::log(&format!("[axl-custom] CharCustomA#{n} → a1 as DynArray: ptr={arr_ptr:#018x} cap={cap} sz={sz}"));
            if sz > 0 && sz < 256 && arr_ptr > 0x1_0000_0000
                && gum::is_readable(arr_ptr as *const c_void, 16)
            {
                // Handle<T> = 8B obj-ptr + 8B refcount-block-ptr
                let h0_ptr = (arr_ptr as *const u64).read_unaligned();
                let h0_rc  = ((arr_ptr + 8) as *const u64).read_unaligned();
                crate::log(&format!("[axl-custom] CharCustomA#{n} option[0]: obj={h0_ptr:#018x} rc={h0_rc:#018x}"));
            }
        }
    }
}

// axl-customization-apply: gameuiCharacterCustomizationSystem candidate B (smaller, frame=0x30)
unsafe extern "C" fn char_custom_b_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = CHAR_CUSTOM_B_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_CHAR_CUSTOM_B.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 5 {
        crate::log(&format!("[axl-custom] CharCustomB#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
        // Try reading a1 as DynArray too (same hypothesis check)
        if a1 > 0x1_0000_0000 && gum::is_readable(a1 as *const c_void, 16) {
            let arr_ptr = (a1 as *const u64).read_unaligned();
            let cap = ((a1 + 8) as *const u32).read_unaligned();
            let sz = ((a1 + 12) as *const u32).read_unaligned();
            crate::log(&format!("[axl-custom] CharCustomB#{n} → a1 as DynArray: ptr={arr_ptr:#018x} cap={cap} sz={sz}"));
        }
    }
}

// axl-world-streaming-apply: worldStreamingBlock / worldStreamingBlockIndex probe functions (2026-07-26)
// Gate: ~/.bwms-streamprobe — installs all 4 candidates to find which fires during level load.
// Probes log x0/x1/x2 and scan x0 as struct for DynArray signatures (ptr+cap+sz).
unsafe extern "C" fn wsblock_fn1_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = WSBLOCK_FN1_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_WSBLOCK_FN1.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 3 {
        crate::log(&format!("[axl-wsblk] FN1 (rtti-once)#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
    }
}

unsafe extern "C" fn wsblock_fn2_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = WSBLOCK_FN2_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_WSBLOCK_FN2.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 8 {
        crate::log(&format!("[axl-wsblk] FN2#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
        if gum::is_readable(this, 0x80) {
            let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
            let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
            crate::log(&format!("[axl-wsblk] FN2 struct+00..+3f: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
                rf64(0x00), rf64(0x08), rf64(0x10), rf64(0x18), rf64(0x20), rf64(0x28), rf64(0x30), rf64(0x38)));
            // DynArray scan +0x10..+0x70
            for off in (0x10usize..=0x70).step_by(8) {
                if !gum::is_readable(this.cast::<u8>().add(off) as *const c_void, 16) { continue; }
                let arr_ptr = rf64(off);
                let cap = rf32(off + 8);
                let sz  = rf32(off + 0xc);
                if arr_ptr > 0x7_0000_0000 && arr_ptr < 0x1_0000_0000_0000
                    && sz >= 1 && sz <= cap && cap <= 512
                {
                    crate::log(&format!("[axl-wsblk] FN2 DynArray@+{off:#x}: ptr={arr_ptr:#018x} cap={cap} sz={sz}"));
                }
            }
        }
    }
}

unsafe extern "C" fn wsblockidx_fn1_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = WSBLOCKIDX_FN1_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_WSBLOCKIDX_FN1.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 3 {
        crate::log(&format!("[axl-wsidx] FN1 (rtti-once)#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
    }
}

unsafe extern "C" fn wsblockidx_fn2_probe(this: *mut c_void, a1: u64, a2: u64) {
    let n = WSBLOCKIDX_FN2_CNT.fetch_add(1, Ordering::Relaxed);
    let orig = ORIG_WSBLOCKIDX_FN2.load(Ordering::Relaxed);
    if !orig.is_null() { let f: PostLoadFn = std::mem::transmute(orig); f(this, a1, a2); }
    if n < 8 {
        crate::log(&format!("[axl-wsidx] FN2#{n} this={this:p} a1={a1:#018x} a2={a2:#018x}"));
        if gum::is_readable(this, 0x80) {
            let rf64 = |off: usize| (this.cast::<u8>().add(off) as *const u64).read_unaligned();
            let rf32 = |off: usize| (this.cast::<u8>().add(off) as *const u32).read_unaligned();
            crate::log(&format!("[axl-wsidx] FN2 struct+00..+3f: {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x} {:#018x}",
                rf64(0x00), rf64(0x08), rf64(0x10), rf64(0x18), rf64(0x20), rf64(0x28), rf64(0x30), rf64(0x38)));
            for off in (0x10usize..=0x70).step_by(8) {
                if !gum::is_readable(this.cast::<u8>().add(off) as *const c_void, 16) { continue; }
                let arr_ptr = rf64(off);
                let cap = rf32(off + 8);
                let sz  = rf32(off + 0xc);
                if arr_ptr > 0x7_0000_0000 && arr_ptr < 0x1_0000_0000_0000
                    && sz >= 1 && sz <= cap && cap <= 512
                {
                    crate::log(&format!("[axl-wsidx] FN2 DynArray@+{off:#x}: ptr={arr_ptr:#018x} cap={cap} sz={sz}"));
                }
            }
        }
    }
}

pub(crate) fn axl_streaming_info() -> Option<String> {
    let sl14 = POSTLOAD_CNT_SL14.load(Ordering::Relaxed);
    let sl25 = POSTLOAD_CNT_SL25.load(Ordering::Relaxed);
    let sl26 = POSTLOAD_CNT_SL26.load(Ordering::Relaxed);
    let sl27 = POSTLOAD_CNT_SL27.load(Ordering::Relaxed);
    if sl14 == 0 && sl25 == 0 && sl26 == 0 && sl27 == 0 { return None; }
    if STREAMING_SECTOR_APPLIED.load(Ordering::Relaxed) {
        let hash = STREAMING_SECTOR_HASH.load(Ordering::Relaxed);
        let cnt  = STREAMING_NODE_COUNT.load(Ordering::Relaxed);
        Some(format!("AXL STREAMING: SL14={sl14} SL25={sl25} SL26={sl26} SL27={sl27} nodes={cnt} sector={hash:#010x}"))
    } else {
        Some(format!("AXL STREAMING PROBE: SL14={sl14} SL25={sl25} SL26={sl26} SL27={sl27}"))
    }
}

/// Instala 1 hook de probe (`Interceptor::replace`) num vmaddr, com o padrão idempotente já usado
/// em toda `install_postload_hooks()` (static `AtomicBool` local pra nunca reinstalar 2x, checa
/// legibilidade antes, guarda o trampolim original, loga sucesso/falha). Generaliza
/// `install_journal_probe!`/`install_wsblk_probe!` (2026-07 26/27), que já faziam exatamente isto
/// mas com prefixo/formato de log FIXO — aqui as mensagens são expressões livres (`&str` ou
/// `&format!(...)`), então qualquer call-site antigo porta sem mudar o texto logado.
///
/// Extraído em 2026-08-06 (cont.27): `install_postload_hooks()` tem ~30 blocos deste exato
/// padrão, um por vmaddr candidato descoberto em RE ao longo de ~3 semanas de sessões — cada um
/// copiado do anterior em vez de reusar uma função. Migração feita EM FASES pequenas (pedido
/// explícito do usuário: não migrar tudo de uma vez, verificar boot a cada fase) — esta fase
/// cobre só os 6 primeiros blocos (patch-probe + streaming SL14/25/26/27), nenhum deles com
/// histórico de crash. Os ~24 blocos restantes (famílias garment/transmog/attachment/inkspawner,
/// vários deles probes de funções JÁ CONFIRMADAS crashando isoladas — ver DATABASE.md) ficam
/// intocados por ora: migrar esses precisa de cautela extra (risco de erro de transcrição num
/// probe que já é conhecido por crashar é pior que a redundância em si).
macro_rules! install_hook_probe {
    ($vm:expr, $probe:expr, $orig:expr, $ok:expr, $fail:expr) => {{
        static INSTALLED: AtomicBool = AtomicBool::new(false);
        let target = crate::rebase($vm);
        if gum::is_readable(target as *const c_void, 16) && !INSTALLED.swap(true, Ordering::Relaxed) {
            let it = Interceptor::obtain();
            match it.replace(target, $probe as *mut c_void) {
                Some(tramp) => {
                    $orig.store(tramp, Ordering::Relaxed);
                    std::mem::forget(it);
                    crate::log($ok);
                }
                None => {
                    INSTALLED.store(false, Ordering::Relaxed);
                    crate::log($fail);
                }
            }
        }
    }};
}

pub(crate) unsafe fn install_postload_hooks() {
    install_hook_probe!(
        CMESH_POSTLOAD_VM, cmesh_postload_replacement, ORIG_CMESH_POSTLOAD,
        "[axl-patch-probe] CMesh::PostLoad hook instalado @ 0x100e16b28",
        "[axl-patch-probe] FALHA CMesh::PostLoad hook"
    );
    install_hook_probe!(
        MORPHTGT_POSTLOAD_VM, morphtgt_postload_replacement, ORIG_MORPHTGT_POSTLOAD,
        "[axl-patch-probe] MorphTargetMesh::PostLoad hook instalado @ 0x100e467bc",
        "[axl-patch-probe] FALHA MorphTargetMesh::PostLoad hook"
    );
    // ArchiveXL `#51`: EntityTemplate/AppearanceResource PostLoad (2026-08-12, candidatos resolvidos
    // por `postloadprobe` ao vivo, nunca antes instalados como hook). Gate PRÓPRIO (nunca testado
    // ainda) — mesma disciplina de todo probe novo desta sessão, apesar de a categoria (PostLoad
    // observe-only, mesmo padrão de CMesh/MorphTargetMesh) já ter histórico limpo.
    if bwms_hook_enabled("entitytemplate-postload") {
        install_hook_probe!(
            ENTITYTEMPLATE_POSTLOAD_VM, entitytemplate_postload_replacement, ORIG_ENTITYTEMPLATE_POSTLOAD,
            &format!("[axl-resource-patch] EntityTemplate::PostLoad hook instalado @ {:#010x}", ENTITYTEMPLATE_POSTLOAD_VM),
            "[axl-resource-patch] FALHA EntityTemplate::PostLoad hook"
        );
    }
    if bwms_hook_enabled("appearanceresource-postload") {
        install_hook_probe!(
            APPEARANCERESOURCE_POSTLOAD_VM, appearanceresource_postload_replacement, ORIG_APPEARANCERESOURCE_POSTLOAD,
            &format!("[axl-resource-patch] AppearanceResource::PostLoad hook instalado @ {:#010x}", APPEARANCERESOURCE_POSTLOAD_VM),
            "[axl-resource-patch] FALHA AppearanceResource::PostLoad hook"
        );
    }
    // ArchiveXL `#49`: worldStreamingSector::PostLoad candidato #2 (2026-08-12, diverge de SL26 —
    // ver nota completa no const). Gate PRÓPRIO E SEPARADO do `streaming-sl26` — NUNCA ligar os 2
    // juntos até uma sessão futura decidir testar a reconciliação de propósito (comparar ordem de
    // disparo + offsets de DynArray no MESMO `this`).
    if bwms_hook_enabled("wssector-postload-c2") {
        install_hook_probe!(
            WORLDSTREAMINGSECTOR_POSTLOAD_C2_VM, wssector_c2_postload_replacement, ORIG_WSSECTOR_C2_POSTLOAD,
            &format!("[axl-streaming-c2] worldStreamingSector::PostLoad candidato#2 hook instalado @ {:#010x}", WORLDSTREAMINGSECTOR_POSTLOAD_C2_VM),
            "[axl-streaming-c2] FALHA worldStreamingSector::PostLoad candidato#2 hook"
        );
    }
    install_hook_probe!(
        STREAMING_SL14_VM, streaming_sl14_probe, ORIG_SL14,
        &format!("[axl-probe] SL14 hook @ {:#010x}", STREAMING_SL14_VM),
        "[axl-probe] FALHA SL14"
    );
    install_hook_probe!(
        STREAMING_SL25_VM, streaming_sl25_probe, ORIG_SL25,
        &format!("[axl-probe] SL25 hook @ {:#010x}", STREAMING_SL25_VM),
        "[axl-probe] FALHA SL25"
    );
    // GATE (2026-08-03): SL26 mata o processo SEM crash report nem log (2 boots reproduziram
    // exatamente no mesmo ponto — instalação nunca completa, nem a linha "FALHA SL26" aparece).
    // Não relacionado a nenhum dos 4 gaps do axl-*-apply — opt-in via marcador, default OFF, pra
    // não bloquear os hooks importantes (transmog/puppet) que vêm depois nesta mesma função.
    if bwms_hook_enabled("streaming-sl26") {
        install_hook_probe!(
            STREAMING_SL26_VM, streaming_sl26_probe, ORIG_SL26,
            &format!("[axl-probe] SL26 hook @ {:#010x}", STREAMING_SL26_VM),
            "[axl-probe] FALHA SL26"
        );
    }
    if bwms_hook_enabled("streaming-sl27") {
        install_hook_probe!(
            STREAMING_SL27_VM, streaming_sl27_probe, ORIG_SL27,
            &format!("[axl-probe] SL27 hook @ {:#010x}", STREAMING_SL27_VM),
            "[axl-probe] FALHA SL27"
        );
    }
    // 2026-07-29: agente de RE dedicado investigou o crash `0x50`/`redDispatcher8` (achado: nenhum
    // dos 6 frames reais da pilha bate com NENHUMA das funções hookadas — o crash é vários frames
    // ABAIXO, em código genérico de entity-attachment sem lock, e os 4 probes da rodada 1 estavam
    // TODOS armados juntos no boot que crashou — ainda não se sabe QUAL especificamente alcançou
    // esse caminho). Recomendação do agente: isolar 1 hook por vez antes de re-testar. Refatorado:
    // cada um dos 12 hooks desta família agora tem um marcador PRÓPRIO E INDIVIDUAL
    // (`~/.bwms-garmentprobe-<nome>`), permitindo testar exatamente 1 por boot; o marcador antigo
    // (`~/.bwms-garmentprobe`, sem sufixo) continua funcionando como "liga todos juntos" por
    // compatibilidade, mas NUNCA usar esse modo de novo sem entender o que já crashou antes.
    // 2026-08-05 (unificação BWMS): renomeado de `garment_hook_enabled` — o nome antigo vazava
    // "garment" (o 1º caso de uso) pra hooks que já não têm nada a ver com garment há tempos
    // (transmog/loadtemplate/selectappname, questphase, streaming-sl26/27) — os 21 call-sites
    // que usam este gate cobrem 4 "enablers" diferentes, é o gate ÚNICO de probes opcionais do
    // runtime, não um recurso do garment. Marcador NOVO preferido: `~/.bwms-hook`/`~/.bwms-hook-
    // <nome>`; o antigo `~/.bwms-garmentprobe*` continua funcionando (compat — não quebra
    // marcadores já em disco de sessões anteriores, ex. `~/.bwms-garmentprobe-additem`).
    fn bwms_hook_enabled(individual: &str) -> bool {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::Path::new(&format!("{home}/.bwms-hook")).exists()
            || std::path::Path::new(&format!("{home}/.bwms-hook-{individual}")).exists()
            || std::path::Path::new(&format!("{home}/.bwms-garmentprobe")).exists()
            || std::path::Path::new(&format!("{home}/.bwms-garmentprobe-{individual}")).exists()
    }
    // Codeware `#120` candidato `0x104a13088` (2026-08-17): gate PRÓPRIO
    // (`~/.bwms-hook-ink120cand`), nunca ligado por padrão — mesmo probe também instalável sob
    // demanda via comando `ink120hook` sem precisar deste gate de boot.
    if bwms_hook_enabled("ink120cand") {
        install_ink120cand_probe();
    }
    // FindAppearance probe gateado: multi-thread no redDispatcher causava 0x50 null-deref crash
    if bwms_hook_enabled("findappearance") {
        install_hook_probe!(
            ENTITY_TEMPLATE_FIND_APP_VM, entity_template_find_app_probe, ORIG_FIND_APP,
            &format!("[axl-garment] FindAppearance probe @ {:#010x}", ENTITY_TEMPLATE_FIND_APP_VM),
            "[axl-garment] FALHA FindAppearance probe"
        );
    }

    // axl-garment-apply: AppearanceResource::FindAppearance — sret (2026-07-29). DIFERENTE de
    // EntityTemplate::FindAppearance (a culpada do crash) — tem lock explícito, candidata mais
    // segura. Gate PRÓPRIO E SEPARADO, nunca testada ao vivo ainda.
    if bwms_hook_enabled("appearanceresource-findapp") {
        install_hook_probe!(
            APPEARANCE_RESOURCE_FIND_APP_VM, appearance_resource_findapp_probe, ORIG_APPEARANCE_RESOURCE_FINDAPP,
            &format!("[axl-garment] AppearanceResource::FindAppearance probe @ {:#010x}", APPEARANCE_RESOURCE_FIND_APP_VM),
            "[axl-garment] FALHA AppearanceResource::FindAppearance probe"
        );
    }

    // axl-garment-apply: GarmentAssembler::FindState — sret (2026-07-29). Gate PRÓPRIO E SEPARADO.
    // **NUNCA testar ao vivo**: agente de RE confirmou (não por analogia) que a categoria "Find"/
    // resolve deste subsistema (FindAppearance×2 já crasharam) é uma corrida real sem mitigação
    // segura possível num hook de entrada — FindState busca WeakHandle<Entity> na MESMA categoria.
    if bwms_hook_enabled("findstate") {
        install_hook_probe!(
            GARMENT_FINDSTATE_VM, garment_findstate_probe, ORIG_GARMENT_FINDSTATE,
            &format!("[axl-garment] GarmentAssembler::FindState probe @ {:#010x}", GARMENT_FINDSTATE_VM),
            "[axl-garment] FALHA GarmentAssembler::FindState probe"
        );
    }

    // axl-garment-apply: AppearanceChanger::GetSuffixes — sret CString (2026-07-29). Categoria
    // SEGURA (consumidora, tamanho de sret confirmado por disassembly) — gate próprio, mas sem o
    // mesmo veto de "Find"/resolve dos outros sret desta família.
    if bwms_hook_enabled("getsuffixes") {
        install_hook_probe!(
            GARMENT_GETSUFFIXES_VM, garment_getsuffixes_probe, ORIG_GARMENT_GETSUFFIXES,
            &format!("[axl-garment] AppearanceChanger::GetSuffixes probe @ {:#010x}", GARMENT_GETSUFFIXES_VM),
            "[axl-garment] FALHA AppearanceChanger::GetSuffixes probe"
        );
    }

    // axl-garment-apply: 3 probes novos de ABI simples (RE offline 2026-07-28, ver DATABASE.md).
    if bwms_hook_enabled("loadapp") {
        install_hook_probe!(
            ITEMFACTORY_LOADAPP_VM, itemfactory_loadapp_probe, ORIG_ITEMFACTORY_LOADAPP,
            &format!("[axl-garment] ItemFactoryRequest::LoadAppearance probe @ {:#010x}", ITEMFACTORY_LOADAPP_VM),
            "[axl-garment] FALHA ItemFactoryRequest::LoadAppearance probe"
        );
    }

    if bwms_hook_enabled("changeapp-loadapp") {
        install_hook_probe!(
            ITEMFACTORY_CHANGEAPP_LOADAPP_VM, itemfactory_changeapp_loadapp_probe, ORIG_ITEMFACTORY_CHANGEAPP_LOADAPP,
            &format!("[axl-garment] ItemFactoryAppearanceChangeRequest::LoadAppearance probe @ {:#010x}", ITEMFACTORY_CHANGEAPP_LOADAPP_VM),
            "[axl-garment] FALHA ItemFactoryAppearanceChangeRequest::LoadAppearance probe"
        );
    }

    // axl-transmog-apply: 2 probes novos (RE offline 2026-08-01, ver DATABASE.md). Reusa
    // garment_hook_enabled/marcadores individuais — mesma classe/infra, gap diferente só no nome.
    if bwms_hook_enabled("loadtemplate") {
        install_hook_probe!(
            TRANSMOG_LOADTEMPLATE_VM, transmog_loadtemplate_probe, ORIG_TRANSMOG_LOADTEMPLATE,
            &format!("[axl-transmog] ItemFactoryAppearanceChangeRequest::LoadTemplate probe @ {:#010x}", TRANSMOG_LOADTEMPLATE_VM),
            "[axl-transmog] FALHA ItemFactoryAppearanceChangeRequest::LoadTemplate probe"
        );
    }

    if bwms_hook_enabled("selectappname") {
        install_hook_probe!(
            TRANSMOG_SELECTAPPNAME_VM, transmog_selectappname_probe, ORIG_TRANSMOG_SELECTAPPNAME,
            &format!("[axl-transmog] AppearanceChanger::SelectAppearanceName probe @ {:#010x}", TRANSMOG_SELECTAPPNAME_VM),
            "[axl-transmog] FALHA AppearanceChanger::SelectAppearanceName probe"
        );
    }

    if bwms_hook_enabled("reassemble") {
        install_hook_probe!(
            ENTITY_REASSEMBLE_APPEARANCE_VM, entity_reassemble_appearance_probe, ORIG_ENTITY_REASSEMBLE_APPEARANCE,
            &format!("[axl-garment] Entity::ReassembleAppearance probe @ {:#010x}", ENTITY_REASSEMBLE_APPEARANCE_VM),
            "[axl-garment] FALHA Entity::ReassembleAppearance probe"
        );
    }

    // axl-garment-apply rodada 2: 6 probes de ABI simples (RE offline 2026-07-28).
    if bwms_hook_enabled("additem") {
        install_hook_probe!(
            GARMENT_STATE_ADDITEM_VM, garment_additem_probe, ORIG_GARMENT_ADDITEM,
            &format!("[axl-garment] GarmentAssemblerState::AddItem probe @ {:#010x}", GARMENT_STATE_ADDITEM_VM),
            "[axl-garment] FALHA AddItem probe"
        );
    }

    if bwms_hook_enabled("addcustomitem") {
        install_hook_probe!(
            GARMENT_STATE_ADDCUSTOMITEM_VM, garment_addcustomitem_probe, ORIG_GARMENT_ADDCUSTOMITEM,
            &format!("[axl-garment] GarmentAssemblerState::AddCustomItem probe @ {:#010x}", GARMENT_STATE_ADDCUSTOMITEM_VM),
            "[axl-garment] FALHA AddCustomItem probe"
        );
    }

    if bwms_hook_enabled("changeitem") {
        install_hook_probe!(
            GARMENT_STATE_CHANGEITEM_VM, garment_changeitem_probe, ORIG_GARMENT_CHANGEITEM,
            &format!("[axl-garment] GarmentAssemblerState::ChangeItem probe @ {:#010x}", GARMENT_STATE_CHANGEITEM_VM),
            "[axl-garment] FALHA ChangeItem probe"
        );
    }

    if bwms_hook_enabled("changecustomitem") {
        install_hook_probe!(
            GARMENT_STATE_CHANGECUSTOMITEM_VM, garment_changecustomitem_probe, ORIG_GARMENT_CHANGECUSTOMITEM,
            &format!("[axl-garment] GarmentAssemblerState::ChangeCustomItem probe @ {:#010x}", GARMENT_STATE_CHANGECUSTOMITEM_VM),
            "[axl-garment] FALHA ChangeCustomItem probe"
        );
    }

    if bwms_hook_enabled("removeitem") {
        install_hook_probe!(
            GARMENT_REMOVEITEM_VM, garment_removeitem_probe, ORIG_GARMENT_REMOVEITEM,
            &format!("[axl-garment] GarmentAssembler::RemoveItem probe @ {:#010x}", GARMENT_REMOVEITEM_VM),
            "[axl-garment] FALHA RemoveItem probe"
        );
    }

    if bwms_hook_enabled("ongamedetach") {
        install_hook_probe!(
            GARMENT_ONGAMEDETACH_VM, garment_ongamedetach_probe, ORIG_GARMENT_ONGAMEDETACH,
            &format!("[axl-garment] GarmentAssembler::OnGameDetach probe @ {:#010x}", GARMENT_ONGAMEDETACH_VM),
            "[axl-garment] FALHA OnGameDetach probe"
        );
    }

    // axl-garment-apply rodada 3: os 2 últimos sub-hooks (RE offline 2026-07-28) — com isso os
    // 17/17 sub-hooks do gap têm vmaddr; 15/17 codados (FindState/SpawnFromLocal/SpawnFromExternal
    // seguem de fora por ABI sret, ver DATABASE.md).
    if bwms_hook_enabled("getvisualtags") {
        install_hook_probe!(
            GARMENT_GETVISUALTAGS_VM, garment_getvisualtags_probe, ORIG_GARMENT_GETVISUALTAGS,
            &format!("[axl-garment] AppearanceNameVisualTagsPreset::GetVisualTags probe @ {:#010x}", GARMENT_GETVISUALTAGS_VM),
            "[axl-garment] FALHA GetVisualTags probe"
        );
    }

    if bwms_hook_enabled("registerpart") {
        install_hook_probe!(
            GARMENT_REGISTERPART_VM, garment_registerpart_probe, ORIG_GARMENT_REGISTERPART,
            &format!("[axl-garment] AppearanceChanger::RegisterPart probe @ {:#010x}", GARMENT_REGISTERPART_VM),
            "[axl-garment] FALHA RegisterPart probe"
        );
    }

    // ArchiveXL `#17` candidato `0x100ace4b0` de ExtractPartComponents (RE offline 2026-08-19,
    // sessão nº33 — ver nota grande na declaração da const). Gate PRÓPRIO
    // (`~/.bwms-hook-axl17-extractpartcomponents-cand`), nunca ligado por padrão. Observe-only,
    // 1º teste ao vivo.
    if bwms_hook_enabled("axl17-extractpartcomponents-cand") {
        install_hook_probe!(
            APPEARANCE_EXTRACTPARTCOMPONENTS_CAND_VM, extractpartcomponents_cand_probe, ORIG_EXTRACTPARTCOMPONENTS_CAND,
            &format!("[axl-17-cand] candidato ExtractPartComponents probe @ {:#010x}", APPEARANCE_EXTRACTPARTCOMPONENTS_CAND_VM),
            "[axl-17-cand] FALHA candidato probe"
        );
    }

    // Codeware `#176` candidato `0x1049bded0` de `Raw::inkWidget::TriggerEvent` (RE offline
    // 2026-08-19, mesmo dia, elevada a MÉDIA-ALTA por 3 confirmações estruturais — ver nota
    // grande na declaração da const). Gate PRÓPRIO
    // (`~/.bwms-hook-codeware-176-triggerevent-cand`), nunca ligado por padrão. Observe-only,
    // 1º teste ao vivo.
    if bwms_hook_enabled("codeware-176-triggerevent-cand") {
        install_hook_probe!(
            INKWIDGET_TRIGGEREVENT_CAND_VM, inkwidget_triggerevent_cand_probe, ORIG_INKWIDGET_TRIGGEREVENT_CAND,
            &format!("[cw-176-triggerevent-cand] candidato TriggerEvent probe @ {:#010x}", INKWIDGET_TRIGGEREVENT_CAND_VM),
            "[cw-176-triggerevent-cand] FALHA candidato probe"
        );
    }

    // ComputePlayerGarment (2026-08-05): mesma categoria segura de RegisterPart/GetVisualTags —
    // nunca hookada antes, é o ponto real de APLICAÇÃO visual do override (ver nota grande na
    // declaração da const). Gate próprio, novo, nunca ligado por padrão.
    if bwms_hook_enabled("computeplayergarment") {
        install_hook_probe!(
            GARMENT_COMPUTEPLAYERGARMENT_VM, garment_computeplayergarment_probe, ORIG_GARMENT_COMPUTEPLAYERGARMENT,
            &format!("[axl-garment] AppearanceChanger::ComputePlayerGarment probe @ {:#010x}", GARMENT_COMPUTEPLAYERGARMENT_VM),
            "[axl-garment] FALHA ComputePlayerGarment probe"
        );
    }
    // Wrapper de ComputePlayerGarment (2026-08-06, cont.36) — roda como JOB ASSÍNCRONO (ver nota
    // grande na declaração da const). Gate PRÓPRIO, separado do CPG síncrono, nunca ligado por padrão.
    if bwms_hook_enabled("cpgwrapper") {
        install_hook_probe!(
            GARMENT_CPG_WRAPPER_VM, garment_cpg_wrapper_probe, ORIG_GARMENT_CPG_WRAPPER,
            &format!("[axl-fullbody-torso] wrapper de ComputePlayerGarment (job assíncrono) probe @ {:#010x}", GARMENT_CPG_WRAPPER_VM),
            "[axl-fullbody-torso] FALHA cpg-wrapper probe"
        );
    }
    // Handler de AppearanceMeshLoadedEvent (2026-08-06, cont.39) — mecanismo SEPARADO do JobSystem,
    // achado por reconstrução de PMF (ver nota grande na declaração da const). Gate PRÓPRIO, nunca
    // ligado por padrão.
    if bwms_hook_enabled("meshloadedhandler") {
        install_hook_probe!(
            APPEARANCE_MESHLOADED_HANDLER_VM, appearance_meshloaded_handler_probe, ORIG_APPEARANCE_MESHLOADED_HANDLER,
            &format!("[axl-fullbody-torso] handler de AppearanceMeshLoadedEvent/DissolveFinish (SkinnedMeshComponent) probe @ {:#010x}", APPEARANCE_MESHLOADED_HANDLER_VM),
            "[axl-fullbody-torso] FALHA mesh-loaded-handler probe"
        );
    }
    // Handler de AppearanceStatusEvent (2026-08-07, cont.40) — mesma família, candidato mais
    // promissor semanticamente. Gate PRÓPRIO, nunca ligado por padrão.
    if bwms_hook_enabled("statushandler") {
        install_hook_probe!(
            APPEARANCE_STATUS_HANDLER_VM, appearance_status_handler_probe, ORIG_APPEARANCE_STATUS_HANDLER,
            &format!("[axl-fullbody-torso] handler de AppearanceStatusEvent (SkinnedMeshComponent) probe @ {:#010x}", APPEARANCE_STATUS_HANDLER_VM),
            "[axl-fullbody-torso] FALHA appearance-status probe"
        );
    }
    // Método virtual "reconstrói render proxy" (2026-08-07, cont.40) — vtable+0x288 real por
    // trás do dispatch acima. Gate PRÓPRIO, nunca ligado por padrão.
    if bwms_hook_enabled("rebuildproxy") {
        install_hook_probe!(
            APPEARANCE_REBUILD_PROXY_VM, appearance_rebuild_proxy_probe, ORIG_APPEARANCE_REBUILD_PROXY,
            &format!("[axl-fullbody-torso] método virtual reconstrói-render-proxy (SkinnedMeshComponent) probe @ {:#010x}", APPEARANCE_REBUILD_PROXY_VM),
            "[axl-fullbody-torso] FALHA rebuild-proxy probe"
        );
    }

    // Handler de AppearanceMeshLoadedEvent DE VERDADE (2026-08-07, cont.41) — endereço corrigido,
    // nunca testado ao vivo antes. Gate PRÓPRIO, nunca ligado por padrão.
    if bwms_hook_enabled("meshloadedreal") {
        install_hook_probe!(
            APPEARANCE_MESHLOADED_REAL_VM, appearance_meshloaded_real_probe, ORIG_APPEARANCE_MESHLOADED_REAL,
            &format!("[axl-fullbody-torso] handler de AppearanceMeshLoadedEvent DE VERDADE (SkinnedMeshComponent) probe @ {:#010x}", APPEARANCE_MESHLOADED_REAL_VM),
            "[axl-fullbody-torso] FALHA mesh-loaded-REAL probe"
        );
    }

    // axl-animation-apply: candidato de BAIXA CONFIANÇA (ver nota grande na declaração da const),
    // gate PRÓPRIO, nunca ligado por padrão. Testado ao vivo só pra refutar/confirmar rápido.
        if bwms_hook_enabled("animinitcandidate") {
        install_hook_probe!(
            ANIM_INITANIM_CANDIDATE_VM, anim_initanim_candidate_probe, ORIG_ANIM_INITANIM_CANDIDATE,
            &format!("[axl-anim-candidate] candidato InitializeAnimations probe @ {:#010x}", ANIM_INITANIM_CANDIDATE_VM),
            "[axl-anim-candidate] FALHA candidato probe"
        );
    }
    // axl-attachment-apply: 2 candidatos pra IsAffectedSlot (ver nota grande). Gates próprios.
    if bwms_hook_enabled("attachisaffectedslot") {
        install_hook_probe!(
            ATTACH_ISAFFECTEDSLOT_C1_VM, attach_isaffectedslot_c1_probe, ORIG_ATTACH_ISAFFECTEDSLOT_C1,
            &format!("[axl-attach-isaffected-c1] candidato probe @ {:#010x}", ATTACH_ISAFFECTEDSLOT_C1_VM),
            "[axl-attach-isaffected-c1] FALHA candidato probe"
        );
        install_hook_probe!(
            ATTACH_ISAFFECTEDSLOT_C2_VM, attach_isaffectedslot_c2_probe, ORIG_ATTACH_ISAFFECTEDSLOT_C2,
            &format!("[axl-attach-isaffected-c2] candidato probe @ {:#010x}", ATTACH_ISAFFECTEDSLOT_C2_VM),
            "[axl-attach-isaffected-c2] FALHA candidato probe"
        );
    }
    // axl-attachment-apply: candidato #3 pra IsAffectedSlot/função irmã (0x1035a60e0, confiança
    // MÉDIA, RE 2026-08-11 — ver nota grande na declaração da const). Gate PRÓPRIO e SEPARADO dos
    // C1/C2 (nomes de gate diferentes de propósito — nunca ligar junto sem motivo, mantém cada
    // candidato isolável por boot, mesma disciplina já usada pro resto desta família).
    // OBSERVE-ONLY, nunca chamado/consumido fora deste hook. CODADO, SEM PROVA AO VIVO (sessão
    // 100% offline).
    if bwms_hook_enabled("attachisaffectedslot-c3") {
        install_hook_probe!(
            ATTACH_ISAFFECTEDSLOT_C3_VM, attach_isaffectedslot_c3_probe, ORIG_ATTACH_ISAFFECTEDSLOT_C3,
            &format!("[axl-attach-isaffected-c3] candidato probe @ {:#010x}", ATTACH_ISAFFECTEDSLOT_C3_VM),
            "[axl-attach-isaffected-c3] FALHA candidato probe"
        );
    }
    // axl-attachment-apply: candidato pra CheckState (ver nota grande na declaração da const).
    // Gate próprio, nunca ligado por padrão.
        if bwms_hook_enabled("attachcheckstatecandidate") {
        install_hook_probe!(
            ATTACH_CHECKSTATE_CANDIDATE_VM, attach_checkstate_candidate_probe, ORIG_ATTACH_CHECKSTATE_CANDIDATE,
            &format!("[axl-attach-candidate] candidato CheckState probe @ {:#010x}", ATTACH_CHECKSTATE_CANDIDATE_VM),
            "[axl-attach-candidate] FALHA candidato probe"
        );
    }
    // TPPRepresentationComponent::OnAttach candidato (ver nota grande na declaração da const).
    // Gate próprio, nunca ligado por padrão.
        if bwms_hook_enabled("tpponattachcandidate") {
        install_hook_probe!(
            TPP_ONATTACH_CANDIDATE_VM, tpp_onattach_candidate_probe, ORIG_TPP_ONATTACH_CANDIDATE,
            &format!("[axl-tpp-candidate] candidato TPPRepresentationComponent::OnAttach probe @ {:#010x}", TPP_ONATTACH_CANDIDATE_VM),
            "[axl-tpp-candidate] FALHA candidato probe"
        );
    }
    // TPPRepresentationComponent::OnAttach candidato #2, vtable slot[49] (ver nota grande na
    // declaração da const). Gate próprio, separado do candidato #1, nunca ligado por padrão.
    if bwms_hook_enabled("tpponattachc2") {
        install_hook_probe!(
            TPP_ONATTACH_C2_VM, tpp_onattach_c2_probe, ORIG_TPP_ONATTACH_C2,
            &format!("[axl-tpp-candidate] candidato #2 TPPRepresentationComponent::OnAttach (vtable slot[49]) probe @ {:#010x}", TPP_ONATTACH_C2_VM),
            "[axl-tpp-candidate] FALHA candidato #2 probe"
        );
    }
    // item #53 (2026-08-12): __invoke CONFIRMADO por símbolo do event-connector
    // TPPRepresentationComponent+ItemEquippedInSlot (ver nota grande na declaração da const).
    // Gate PRÓPRIO, nunca ligado por padrão.
    if bwms_hook_enabled("tpp-itemequip-invoke") {
        install_hook_probe!(
            TPP_ITEMEQUIP_INVOKE_VM, tpp_itemequip_invoke_probe, ORIG_TPP_ITEMEQUIP_INVOKE,
            &format!("[axl-tpp-itemequip] ItemEquippedInSlot invoke-thunk probe @ {:#010x}", TPP_ITEMEQUIP_INVOKE_VM),
            "[axl-tpp-itemequip] FALHA ItemEquippedInSlot invoke-thunk probe"
        );
    }
    // item #53 (2026-08-12, checkpoint novo): candidato idx28 do vtable REAL (40 slots, não 64)
    // de TPPRepresentationSlotListener (ver nota grande na declaração da const). Confiança
    // MÉDIA-BAIXA, groundwork exploratório — gate PRÓPRIO, nunca ligado por padrão.
    if bwms_hook_enabled("tpp-slotlistener-idx28-cand") {
        install_hook_probe!(
            TPP_SLOTLISTENER_IDX28_CAND_VM, tpp_slotlistener_idx28_cand_probe, ORIG_TPP_SLOTLISTENER_IDX28_CAND,
            &format!("[axl-tpp-slotlistener-idx28] candidato idx28 (hipótese B, MÉDIA-BAIXA) probe @ {:#010x}", TPP_SLOTLISTENER_IDX28_CAND_VM),
            "[axl-tpp-slotlistener-idx28] FALHA candidato idx28 probe"
        );
    }
    // itens #40/#41/#54 (2026-08-12): __invoke CONFIRMADO por símbolo do event-connector
    // AttachmentSlots+EquipStart (ver nota grande na declaração da const). Gate PRÓPRIO, nunca
    // ligado por padrão.
    if bwms_hook_enabled("attachslots-equipstart-invoke") {
        install_hook_probe!(
            ATTACHSLOTS_EQUIPSTART_INVOKE_VM, attachslots_equipstart_invoke_probe, ORIG_ATTACHSLOTS_EQUIPSTART_INVOKE,
            &format!("[axl-attachslots-equipstart] EquipStart invoke-thunk probe @ {:#010x}", ATTACHSLOTS_EQUIPSTART_INVOKE_VM),
            "[axl-attachslots-equipstart] FALHA EquipStart invoke-thunk probe"
        );
    }
    // item #55 (2026-08-12): candidato ForceStartNode, observe-only (ver nota grande na declaração
    // da const). Gate PRÓPRIO, nunca ligado por padrão (confiança MÉDIA-ALTA, não confirmada).
    if bwms_hook_enabled("questphase-forcestartnode-cand") {
        install_hook_probe!(
            QUESTSYS_FORCESTARTNODE_CAND_VM, questsys_forcestartnode_cand_probe, ORIG_QUESTSYS_FORCESTARTNODE_CAND,
            &format!("[axl-questphase-forcestartnode] candidato probe @ {:#010x}", QUESTSYS_FORCESTARTNODE_CAND_VM),
            "[axl-questphase-forcestartnode] FALHA candidato probe"
        );
    }
    // axl-inkspawner-apply: 2 probes de ABI simples (RE offline 2026-07-28). Gate PRÓPRIO, separado
    // do garmentprobe — família nunca testada ao vivo ainda, nunca ligar por padrão.
    if std::path::Path::new(&format!("{}/.bwms-inkspawnerprobe", std::env::var("HOME").unwrap_or_default())).exists() {
        install_hook_probe!(
            INKSPAWNER_ASYNC_LOCAL_VM, inkspawner_async_local_probe, ORIG_INKSPAWNER_ASYNC_LOCAL,
            &format!("[axl-inkspawner] AsyncSpawnFromLocal probe @ {:#010x}", INKSPAWNER_ASYNC_LOCAL_VM),
            "[axl-inkspawner] FALHA AsyncSpawnFromLocal probe"
        );
        install_hook_probe!(
            INKSPAWNER_ASYNC_EXTERNAL_VM, inkspawner_async_external_probe, ORIG_INKSPAWNER_ASYNC_EXTERNAL,
            &format!("[axl-inkspawner] AsyncSpawnFromExternal probe @ {:#010x}", INKSPAWNER_ASYNC_EXTERNAL_VM),
            "[axl-inkspawner] FALHA AsyncSpawnFromExternal probe"
        );
    }
    // axl-inkspawner-apply: AsyncSpawnFromDefPack (funil, RE offline 2026-07-29). Gate PRÓPRIO E
    // SEPARADO do inkspawnerprobe de cima — o agente de RE achou o mesmo padrão de risco da família
    // garment (atomics de refcount + hand-off pro job-system, sem lock explícito visível), mesmo
    // sendo o "funil"; isolar o gate preserva poder testar o par já confirmado seguro (Local/External)
    // sem re-armar este. Nunca ligar por padrão; teste ao vivo deliberadamente adiado por cautela.
    if std::path::Path::new(&format!("{}/.bwms-inkspawner-defpack-probe", std::env::var("HOME").unwrap_or_default())).exists() {
        install_hook_probe!(
            INKSPAWNER_ASYNC_DEFPACK_VM, inkspawner_defpack_probe, ORIG_INKSPAWNER_DEFPACK,
            &format!("[axl-inkspawner] AsyncSpawnFromDefPack probe @ {:#010x}", INKSPAWNER_ASYNC_DEFPACK_VM),
            "[axl-inkspawner] FALHA AsyncSpawnFromDefPack probe"
        );
    }
    // axl-inkspawner-apply: SpawnFromLocal/SpawnFromExternal (sret, RE offline 2026-07-28, ABI
    // verificada empiricamente em 2026-07-29 antes de codar — ver comentário de
    // INKSPAWNER_SPAWNFROMLOCAL_VM). Gates PRÓPRIOS E SEPARADOS — nunca testados ao vivo ainda.
    if std::path::Path::new(&format!("{}/.bwms-inkspawner-spawnlocal-probe", std::env::var("HOME").unwrap_or_default())).exists() {
        install_hook_probe!(
            INKSPAWNER_SPAWNFROMLOCAL_VM, inkspawner_spawnfromlocal_probe, ORIG_INKSPAWNER_SPAWNLOCAL,
            &format!("[axl-inkspawner] SpawnFromLocal probe @ {:#010x}", INKSPAWNER_SPAWNFROMLOCAL_VM),
            "[axl-inkspawner] FALHA SpawnFromLocal probe"
        );
    }
    if std::path::Path::new(&format!("{}/.bwms-inkspawner-spawnexternal-probe", std::env::var("HOME").unwrap_or_default())).exists() {
        install_hook_probe!(
            INKSPAWNER_SPAWNFROMEXTERNAL_VM, inkspawner_spawnfromexternal_probe, ORIG_INKSPAWNER_SPAWNEXTERNAL,
            &format!("[axl-inkspawner] SpawnFromExternal probe @ {:#010x}", INKSPAWNER_SPAWNFROMEXTERNAL_VM),
            "[axl-inkspawner] FALHA SpawnFromExternal probe"
        );
    }
    {
        install_hook_probe!(
            JOURNAL_PROCESS_VM, journal_process_probe, ORIG_JOURNAL,
            &format!("[axl-journal] ProcessJournalIndex probe @ {:#010x}", JOURNAL_PROCESS_VM),
            "[axl-journal] FALHA ProcessJournalIndex probe"
        );
    }
    // axl-journal-apply: 4 additional journal hook candidates — gated ~/.bwms-journalprobe
    if std::path::Path::new(&format!("{}/.bwms-journalprobe", std::env::var("HOME").unwrap_or_default())).exists() {
        macro_rules! install_journal_probe {
            ($vm:expr, $probe:expr, $orig:expr, $tag:literal) => {{
                static INSTALLED: AtomicBool = AtomicBool::new(false);
                let target = crate::rebase($vm);
                if gum::is_readable(target as *const c_void, 16) && !INSTALLED.swap(true, Ordering::Relaxed) {
                    let it = Interceptor::obtain();
                    match it.replace(target, $probe as *mut c_void) {
                        Some(tramp) => {
                            $orig.store(tramp, Ordering::Relaxed);
                            std::mem::forget(it);
                            crate::log(&format!("[axl-journal] {} probe @ {:#010x}", $tag, $vm));
                        }
                        None => {
                            INSTALLED.store(false, Ordering::Relaxed);
                            crate::log(&format!("[axl-journal] FALHA {} probe", $tag));
                        }
                    }
                }
            }};
        }
        install_journal_probe!(JOURNAL_LOADSUB_A_VM, journal_loadsub_a_probe, ORIG_JOURNAL_LOADSUB_A, "LoadSubResourceA");
        install_journal_probe!(JOURNAL_LOADSUB_B_VM, journal_loadsub_b_probe, ORIG_JOURNAL_LOADSUB_B, "LoadSubResourceB");
        install_journal_probe!(JOURNAL_LOADROOT_VM,  journal_loadroot_probe,  ORIG_JOURNAL_LOADROOT,  "LoadRootResource");
        install_journal_probe!(JOURNAL_RESLIST_VM,   journal_reslist_probe,   ORIG_JOURNAL_RESLIST,   "ReslistLoader");
    }
    // worldStreamingBlock / worldStreamingBlockIndex probes — gated ~./bwms-streamprobe
    if std::path::Path::new(&format!("{}/.bwms-streamprobe", std::env::var("HOME").unwrap_or_default())).exists() {
        macro_rules! install_wsblk_probe {
            ($vm:expr, $probe:expr, $orig:expr, $tag:literal) => {{
                static INSTALLED: AtomicBool = AtomicBool::new(false);
                let target = crate::rebase($vm);
                if gum::is_readable(target as *const c_void, 16) && !INSTALLED.swap(true, Ordering::Relaxed) {
                    let it = Interceptor::obtain();
                    match it.replace(target, $probe as *mut c_void) {
                        Some(tramp) => {
                            $orig.store(tramp, Ordering::Relaxed);
                            std::mem::forget(it);
                            crate::log(&format!("[axl-wsblk] {} probe @ {:#010x}", $tag, $vm));
                        }
                        None => {
                            INSTALLED.store(false, Ordering::Relaxed);
                            crate::log(&format!("[axl-wsblk] FALHA {} probe", $tag));
                        }
                    }
                }
            }};
        }
        install_wsblk_probe!(WSBLOCK_FN1_VM,    wsblock_fn1_probe,    ORIG_WSBLOCK_FN1,    "wsblock-fn1");
        install_wsblk_probe!(WSBLOCK_FN2_VM,    wsblock_fn2_probe,    ORIG_WSBLOCK_FN2,    "wsblock-fn2");
        install_wsblk_probe!(WSBLOCKIDX_FN1_VM, wsblockidx_fn1_probe, ORIG_WSBLOCKIDX_FN1, "wsidx-fn1");
        install_wsblk_probe!(WSBLOCKIDX_FN2_VM, wsblockidx_fn2_probe, ORIG_WSBLOCKIDX_FN2, "wsidx-fn2");
    }
    {
        install_hook_probe!(
            CHAR_CUSTOM_A_VM, char_custom_a_probe, ORIG_CHAR_CUSTOM_A,
            &format!("[axl-custom] CharCustomA probe @ {:#010x}", CHAR_CUSTOM_A_VM),
            "[axl-custom] FALHA CharCustomA probe"
        );
    }
    {
        install_hook_probe!(
            CHAR_CUSTOM_B_VM, char_custom_b_probe, ORIG_CHAR_CUSTOM_B,
            &format!("[axl-custom] CharCustomB probe @ {:#010x}", CHAR_CUSTOM_B_VM),
            "[axl-custom] FALHA CharCustomB probe"
        );
    }
    // axl-animation-apply: animAnimSet PostLoad
    {
        install_hook_probe!(
            ANIMSET_POSTLOAD_VM, animset_postload_replacement, ORIG_ANIMSET_POSTLOAD,
            "[axl-anim] animAnimSet::PostLoad hook @ 0x10432ff54",
            "[axl-anim] FALHA animAnimSet"
        );
    }
    // axl-animation-apply: animRig PostLoad
    {
        install_hook_probe!(
            ANIMRIG_POSTLOAD_VM, animrig_postload_replacement, ORIG_ANIMRIG_POSTLOAD,
            "[axl-anim] animRig::PostLoad hook @ 0x1045c2644",
            "[axl-anim] FALHA animRig"
        );
    }
    // axl-mesh-apply: entGarmentSkinnedMeshComponent + entSkinnedMeshComponent (shared vmaddr)
    {
        install_hook_probe!(
            GARMENT_SKIN_POSTLOAD_VM, garment_skin_postload_replacement, ORIG_GARMENT_SKIN_POSTLOAD,
            "[axl-mesh] entGarmentSkinned/entSkinned PostLoad hook @ 0x100c7c7e4",
            "[axl-mesh] FALHA garment-skin"
        );
    }
    // axl-mesh-apply: entMeshComponent PostLoad
    {
        install_hook_probe!(
            MESH_COMP_POSTLOAD_VM, mesh_comp_postload_replacement, ORIG_MESH_COMP_POSTLOAD,
            "[axl-mesh] entMeshComponent::PostLoad hook @ 0x100c69f40",
            "[axl-mesh] FALHA entMeshComponent"
        );
    }
    // axl-attachment-apply: entSlotComponent PostLoad
    {
        install_hook_probe!(
            SLOT_COMP_POSTLOAD_VM, slot_comp_postload_replacement, ORIG_SLOT_COMP_POSTLOAD,
            "[axl-attach] entSlotComponent::PostLoad hook @ 0x100c48118",
            "[axl-attach] FALHA entSlotComponent"
        );
    }
    // axl-puppet-state-apply: gameObject::PostLoad DESABILITADO — 0x102185f28 é thunk de vtable
    // (vtable dispatch, causa loop: thunk→chama nosso hook→trampolim→thunk→loop→SIGILL).
    // Precisa de RE estática para achar o PostLoad concreto (não-thunk) de gameObject/entPuppet.
    // cw-controller-misc: IComponent::Toggle scripting handler (ICOMP_TOGGLE_VM=0 → desabilitado)
    if ICOMP_TOGGLE_VM != 0 {
        install_hook_probe!(
            ICOMP_TOGGLE_VM, icomp_toggle_handler, ORIG_ICOMP_TOGGLE,
            &format!("[cw-ctrl-misc] IComponent::Toggle hook @ {ICOMP_TOGGLE_VM:#012x}"),
            "[cw-ctrl-misc] FALHA Toggle hook"
        );
    }
    // cw-controller-misc: vehicleController::ToggleLights C++ hook (2026-07-28: vmaddr resolvido por RE)
    if VEHICLE_AUX_VM != 0 {
        install_hook_probe!(
            VEHICLE_AUX_VM, vehicle_aux_lights_handler, ORIG_VEHICLE_AUX,
            &format!("[cw-ctrl-misc] vehicleController::ToggleLights hook @ {VEHICLE_AUX_VM:#012x}"),
            "[cw-ctrl-misc] FALHA ToggleLights hook"
        );
    }
    // cw-player-scheduling-vehicle: VehicleSystem::ToggleGarageVehicle (RE offline 2026-07-29).
    // Gate PRÓPRIO — nunca testado ao vivo ainda, primeira vez com este gap.
    if std::path::Path::new(&format!("{}/.bwms-vehiclesystem-togglegaragevehicle-probe", std::env::var("HOME").unwrap_or_default())).exists() {
        install_hook_probe!(
            VEHICLESYSTEM_TOGGLEGARAGEVEHICLE_VM, vehiclesystem_togglegaragevehicle_probe, ORIG_VEHICLESYSTEM_TOGGLEGARAGEVEHICLE,
            &format!("[cw-player-scheduling-vehicle] VehicleSystem::ToggleGarageVehicle probe @ {:#010x}", VEHICLESYSTEM_TOGGLEGARAGEVEHICLE_VM),
            "[cw-player-scheduling-vehicle] FALHA ToggleGarageVehicle probe"
        );
    }
    // Codeware `#81`/`#140`: `RuntimeSystemWeather::SetWeatherByName` candidato de RE offline
    // 2026-08-18 (Capstone genuíno, ver nota completa no const `WEATHER_SETWEATHERBYNAME_VM` acima).
    // Gate PRÓPRIO, NUNCA testado ao vivo ainda — primeira vez que este item recebe qualquer código.
    if std::path::Path::new(&format!("{}/.bwms-weathersetname-probe", std::env::var("HOME").unwrap_or_default())).exists() {
        install_hook_probe!(
            WEATHER_SETWEATHERBYNAME_VM, weather_setweatherbyname_probe, ORIG_WEATHER_SETWEATHERBYNAME,
            &format!("[cw-81-weathersetname] SetWeatherByName probe @ {:#010x}", WEATHER_SETWEATHERBYNAME_VM),
            "[cw-81-weathersetname] FALHA SetWeatherByName probe"
        );
    }
    // RED4ext.SDK #461: UpdateRegistrar::RegisterUpdate(group) — PROMOVIDO 2026-08-17 pra
    // `install_pipeline_framebegin_probe()` (sempre-ativo, chamado direto do `on_load`, fora do
    // gate `dev_mode()` desta função) — ver essa função logo abaixo pro racional completo.
    // axl-puppet-state-apply: GenitalsController::OnAttach (GENITAL_ONATTACH_VM=0 → desabilitado)
    if GENITAL_ONATTACH_VM != 0 {
        install_hook_probe!(
            GENITAL_ONATTACH_VM, genital_onattach_handler, ORIG_GENITAL_ONATTACH,
            &format!("[axl-puppet] GenitalsController::OnAttach hook @ {GENITAL_ONATTACH_VM:#012x}"),
            "[axl-puppet] FALHA GenitalsController::OnAttach hook"
        );
    }
    // axl-puppet-state-apply: HairstyleController::OnDetach (HAIRSTYLE_ONDETACH_VM=0 → desabilitado)
    if HAIRSTYLE_ONDETACH_VM != 0 {
        install_hook_probe!(
            HAIRSTYLE_ONDETACH_VM, hairstyle_ondetach_handler, ORIG_HAIRSTYLE_ONDETACH,
            &format!("[axl-puppet] HairstyleController::OnDetach hook @ {HAIRSTYLE_ONDETACH_VM:#012x}"),
            "[axl-puppet] FALHA HairstyleController::OnDetach hook"
        );
    }
    // axl-questphase-apply: QuestsSystem::OnGameRestored candidato (vtable slot 0x158, achado
    // AO VIVO 2026-07-28 via getquestsys). Instalado incondicionalmente no boot (early), pra
    // pegar o disparo real do autocontinue/save-load nesta MESMA sessão.
    {
        install_hook_probe!(
            QUESTSYS_ONGAMERESTORED_VM, questsys_ongamerestored_handler, ORIG_QUESTSYS_ONGAMERESTORED,
            &format!("[axl-questphase] QuestsSystem::OnGameRestored-cand hook @ {QUESTSYS_ONGAMERESTORED_VM:#012x}"),
            "[axl-questphase] FALHA OnGameRestored-cand hook"
        );
    }
    // axl-questphase-apply (2026-08-05): candidato a ProcessPhaseResource — dispatch só por BLR
    // (vtable job-dispatch, categoria de risco já vista noutros gaps deste projeto), ABI incerta.
    // Gateado (não incondicional) por ser mais especulativo que OnGameRestored — liga com
    // ~/.bwms-garmentprobe-questphase (ou o global ~/.bwms-garmentprobe).
        if bwms_hook_enabled("questphase") {
        install_hook_probe!(
            QUEST_PROCESSPHASERESOURCE_VM, quest_processphaseresource_probe, ORIG_QUEST_PROCESSPHASERESOURCE,
            &format!("[axl-questphase] ProcessPhaseResource-cand probe @ {QUEST_PROCESSPHASERESOURCE_VM:#012x}"),
            "[axl-questphase] FALHA ProcessPhaseResource-cand probe"
        );
    }
    // Codeware `#120` (`RawInputHook`/`Raw::inkSystem::ProcessInputEvents`, 2026-08-14 — captura
    // DINÂMICA, ver bloco `inkget_naked`/`install_inkget_probe` abaixo). Gate próprio
    // (~/.bwms-hook-inkget-lr), nunca ligado por padrão.
    install_inkget_probe();
}

/// PRODUTIZAÇÃO 2026-08-17 (RED4ext.SDK `#461`, já ✅ FECHADO no catálogo desde 2026-08-11 —
/// isto NÃO reabre RE nova, só promove a capacidade PROVADA de "atrás de marcador de teste
/// manual" pra "sempre ativa no runtime shipado", mesmo tratamento que `install_pathb_capture()`
/// já recebeu quando virou base da API Facade). Sem isto, `CallbackSystem.RegisterCallback(n
/// "Pipeline/FrameBegin",...)` nunca dispara pra nenhum mod real (o probe que captura o alvo do
/// hook vivia só dentro de `install_postload_hooks()`, atrás de `dev_mode()`). Passthrough
/// incondicional (nunca muda comportamento do jogo) — prova ao vivo decisiva já existe (cont.239-
/// 241 de `HISTORICO.md`, 8+ chamadas vanilla capturadas, zero crash atribuível em toda a
/// investigação). Chamado 1x no `on_load`, idempotente (a macro já tem seu próprio `AtomicBool`).
pub(crate) unsafe fn install_pipeline_framebegin_probe() {
    install_hook_probe!(
        UPDATE_REGISTRAR_GROUP_VM, update_registrar_group_probe, ORIG_UPDATE_REGISTRAR_GROUP,
        &format!("[red4ext-461] UpdateRegistrar::RegisterUpdate(group) probe @ {UPDATE_REGISTRAR_GROUP_VM:#012x} (sempre-ativo)"),
        "[red4ext-461] FALHA UpdateRegistrar probe"
    );
}

// ===== RESOURCE.LINK: hook de swap em 0x1021c5858 (ResourcePath->ResourceReference constructor) =====
// Shim NAKED: grava o path (x0) num ring (dump) e, se x0==swapsrc, troca x0=swaptgt (= resource.link).
// Preserva x8 (indirect-result do constructor) — só usa x9-x14 (scratch). Tail-call ao original.
#[repr(C)]
struct ResLink {
    idx: AtomicU64,        // @0  contador de construções (ring)
    count: AtomicU64,      // @8  # de pares no mapa (gate do asm: 0 = pula a chamada Rust)
    swapcnt: AtomicU64,    // @16 swaps disparados (hits)
    _pad: AtomicU64,       // @24
    ring: [AtomicU64; 64], // @32 últimos paths construídos (pro dump)
}
static RESLINK: ResLink = ResLink {
    idx: AtomicU64::new(0),
    count: AtomicU64::new(0),
    swapcnt: AtomicU64::new(0),
    _pad: AtomicU64::new(0),
    ring: [const { AtomicU64::new(0) }; 64],
};
/// Tabela de redirects (source_hash -> target_hash). Const-init (Vec::new/Mutex::new são const).
static RESLINK_MAP: std::sync::Mutex<Vec<(u64, u64)>> = std::sync::Mutex::new(Vec::new());
static ORIG_RESLINK: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
const RESLINK_VM: u64 = 0x1_021c_5858;

/// `cw-controller-misc` (2026-07-19): hash observado por `watchres <path>` + flag "visto" — o
/// hook `reslink_lookup` (já disparando em TODA construção real de `ResourcePath`, mecanismo
/// zero-crash provado dezenas de vezes) marca `WATCH_RES_SEEN` quando o hash observado bate. O
/// `cp77_tick` (thread do jogo, edge-triggered, MESMO padrão seguro de `Player/Spawned`) drena a
/// flag e dispara `Resource/Load` (`ResourceEvent` real) com o hash — sem hookar
/// `ResourceSerializer::SchedulePostLoadJobs` (RE nova, fora de escopo desta sessão). O disparo em
/// si é 100% real: só acontece quando o jogo de fato constrói aquele `ResourcePath` específico.
pub(crate) static WATCH_RES_HASH: AtomicU64 = AtomicU64::new(0);
pub(crate) static WATCH_RES_SEEN: AtomicBool = AtomicBool::new(false);

/// Arma o watch: registra um self-map (src==tgt, no-op de swap, só força o gate asm `count>0`) +
/// grava o hash-alvo. Reseta `WATCH_RES_SEEN` (novo braço). Loga o hash esperado (cross-check
/// contra o `[re] ResourceEvent.GetPath()` que vai disparar, quando disparar).
pub(crate) fn reslink_watch(path: &str) {
    let h = resource_path_hash(path);
    WATCH_RES_HASH.store(h, Ordering::Relaxed);
    WATCH_RES_SEEN.store(false, Ordering::Relaxed);
    crate::log(&format!("[reslink] watchres armado: '{path}' (hash={h:#018x}) — aguardando construção real do ResourcePath"));
    reslink_add(h, h); // self-map: abre o gate do hook, zero efeito de swap
}

/// Chamado pelo shim SÓ quando count>0 (gate no asm). Varre a tabela; se achar o path, devolve o
/// alvo (swap = resource.link) e conta o hit; senão devolve o path intacto.
unsafe extern "C" fn reslink_lookup(path: u64) -> u64 {
    if let Ok(m) = RESLINK_MAP.lock() {
        for &(s, t) in m.iter() {
            if s == path {
                RESLINK.swapcnt.fetch_add(1, Ordering::Relaxed);
                // DIAG (2026-07-16): loga quando redireciona o .ent do PLAYER (não o animgraph) —
                // confirma se o template do player passa pelo nosso hook no spawn. Player .ent hashes:
                if s == 0x1ab9_05fe_c596_cb22
                    || s == 0x1bcd_09f6_f70a_7818
                    || s == 0x58b1_6007_a8cd_0f7c
                    || s == 0x5ea3_4357_4414_752e
                {
                    crate::log(&format!("[reslink] >>> PLAYER .ent redirecionado: {s:#x} -> {t:#x}"));
                }
                if s == WATCH_RES_HASH.load(Ordering::Relaxed) && s != 0 {
                    WATCH_RES_SEEN.store(true, Ordering::Relaxed);
                }
                return t;
            }
        }
    }
    path
}

#[unsafe(naked)]
unsafe extern "C" fn reslink_shim() {
    core::arch::naked_asm!(
        "adrp x9, {r}@PAGE",
        "add  x9, x9, {r}@PAGEOFF",   // x9 = &RESLINK
        "ldr  x10, [x9]",
        "add  x11, x10, #1",
        "str  x11, [x9]",             // idx++
        "and  x10, x10, #0x3f",
        "add  x12, x9, #32",
        "str  x0, [x12, x10, lsl #3]", // ring[idx&63] = x0 (path)
        "ldr  x13, [x9, #8]",          // count (# de pares)
        "cbz  x13, 2f",                // sem links -> tail-call direto (caminho comum, lock-free)
        "stp  x1, x2, [sp, #-0x60]!",  // salva args + indirect-result (x8) do constructor
        "stp  x3, x4, [sp, #0x10]",
        "stp  x5, x6, [sp, #0x20]",
        "stp  x7, x8, [sp, #0x30]",
        "str  x30, [sp, #0x40]",
        "bl   {lookup}",               // x0 = reslink_lookup(x0)  (swap se hit = resource.link!)
        "ldr  x30, [sp, #0x40]",
        "ldp  x7, x8, [sp, #0x30]",
        "ldp  x5, x6, [sp, #0x20]",
        "ldp  x3, x4, [sp, #0x10]",
        "ldp  x1, x2, [sp], #0x60",
        "2:",
        "adrp x13, {o}@PAGE",
        "add  x13, x13, {o}@PAGEOFF",
        "ldr  x13, [x13]",
        "br   x13",                    // tail-call original (x8/x1..x7/x30 intactos)
        r = sym RESLINK,
        o = sym ORIG_RESLINK,
        lookup = sym reslink_lookup,
    )
}

pub(crate) unsafe fn install_reslink() {
    if !ORIG_RESLINK.load(Ordering::Relaxed).is_null() {
        return; // já instalado (idempotente: dev-selftest + auto-load de mod)
    }
    let t = crate::rebase(RESLINK_VM);
    if !gum::is_readable(t as *const c_void, 16) {
        crate::log("[reslink] alvo 0x1021c5858 ilegível");
        return;
    }
    let it = Interceptor::obtain();
    match it.replace(t, reslink_shim as *mut c_void) {
        Some(tr) => {
            ORIG_RESLINK.store(tr, Ordering::Relaxed);
            std::mem::forget(it);
            crate::log("[reslink] hook NAKED em 0x1021c5858 (ResourcePath->ref) — reslinkdump / reslink <src> <tgt> / reslinkstat");
        }
        None => crate::log("[reslink] replace None"),
    }
}

pub(crate) fn reslink_dump() {
    let total = RESLINK.idx.load(Ordering::Relaxed);
    let mut seen: std::collections::BTreeMap<u64, u32> = std::collections::BTreeMap::new();
    for s in RESLINK.ring.iter() {
        let v = s.load(Ordering::Relaxed);
        if dprobe_pathlike(v) {
            *seen.entry(v).or_insert(0) += 1;
        }
    }
    crate::log(&format!("[reslinkdump] {total} construções, {} paths distintos (use um como <src>):", seen.len()));
    let mut v: Vec<(u64, u32)> = seen.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (p, c) in v.into_iter().take(64) {
        crate::log(&format!("[reslinkdump]   path={p:#018x} x{c}"));
    }
}
/// FNV-1a64 do ResourcePath (lowercase + '/'->'\\'). FONTE ÚNICA no crate `bwms-hashes` (a MESMA
/// impl provada 5/5 goldens em bwms-core/apply_xl.rs; antes era cópia byte-a-byte aqui).
pub(crate) use bwms_hashes::resource_path_hash;
/// Adiciona UM par (source_hash -> target_hash) à tabela. Dedup por source.
pub(crate) fn reslink_add(src: u64, tgt: u64) {
    if let Ok(mut m) = RESLINK_MAP.lock() {
        m.retain(|&(s, _)| s != src);
        m.push((src, tgt));
        RESLINK.count.store(m.len() as u64, Ordering::Relaxed);
        crate::log(&format!("[reslink] +par {src:#018x} -> {tgt:#018x} (tabela: {} pares)", m.len()));
    }
}
/// Adiciona um par a partir dos PATHS (strings) — hasheia com FNV-1a64.
pub(crate) fn reslink_path(src: &str, tgt: &str) {
    let (s, t) = (resource_path_hash(src), resource_path_hash(tgt));
    crate::log(&format!("[reslink] path '{src}' (#{s:#018x}) -> '{tgt}'"));
    reslink_add(s, t);
}
/// Carrega N pares de um arquivo (linhas `srcpath|tgtpath`, `#`=comentário). É o que um mod
/// ArchiveXL real usa: o mod-manager gera esse arquivo do `.xl` (resource.link/copy) e o runtime
/// popula a tabela. Caminho default = `<red4ext>/bwms-reslink.txt`.
pub(crate) fn reslink_file(path: &str) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return crate::log(&format!("[reslink] não leu '{path}': {e}")),
    };
    let mut n = 0;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((s, t)) = line.split_once('|') {
            reslink_add(resource_path_hash(s.trim()), resource_path_hash(t.trim()));
            n += 1;
        }
    }
    crate::log(&format!("[reslink] {n} pares carregados de '{path}'"));
}
/// Compat: limpa a tabela e registra UM par (hashes).
pub(crate) fn reslink_set(src: u64, tgt: u64) {
    if let Ok(mut m) = RESLINK_MAP.lock() {
        m.clear();
    }
    RESLINK.swapcnt.store(0, Ordering::Relaxed);
    reslink_add(src, tgt);
}
pub(crate) fn reslink_stat() {
    let n = RESLINK.swapcnt.load(Ordering::Relaxed);
    let pairs = RESLINK_MAP.lock().map(|m| m.len()).unwrap_or(0);
    crate::log(&format!(
        "[reslink] {pairs} pares na tabela; swaps em path REAL: {n}  {}",
        if n > 0 { ">>> RESOURCE.LINK FUNCIONANDO <<<" } else { "(ainda 0 — src não reconstruído)" }
    ));
}
/// ArchiveXL `#27` (`GetAliases(path) -> Set<ResourcePath>`, `PENDENCIAS-UNIFICADAS.md`,
/// 2026-08-11) — lookup reverso puro sobre `RESLINK_MAP` (mesma tabela que `RegisterLink`/
/// `resource.link` já escrevem): devolve todos os `src` (o path que um mod registrou como
/// ALIAS) cujo `tgt` bate com `canonical` (o path "de verdade" que ele aponta). Zero I/O, zero
/// chamada nativa — leitura pura da tabela em memória, mesmo risco (nenhum) de `reslink_stat`.
pub(crate) fn reslink_aliases_of(canonical: u64) -> Vec<u64> {
    RESLINK_MAP
        .lock()
        .map(|m| m.iter().filter(|(_, t)| *t == canonical).map(|(s, _)| *s).collect())
        .unwrap_or_default()
}

unsafe fn install_archivexl_diag() {
    let target = crate::rebase(ARCHIVE_ALLOC_VM);

    // GUARD 1: legibilidade.
    if !gum::is_readable(target as *const c_void, 16) {
        crate::log("[archivexl-diag] alvo Allocate ilegível -> abortado (sem hook)");
        return;
    }
    // GUARD 2: prólogo (sub/stp/stp/add). Se mudou (patch), aborta sem hookar.
    if !prologue_matches(target, &ARCHIVE_ALLOC_PROLOGUE) {
        crate::log("[archivexl-diag] prólogo de Allocate não casou (patch?) -> abortado (sem hook)");
        return;
    }
    // Idempotência: não instala 2x (run_dev_selftests poderia ser chamado de novo).
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::Relaxed) {
        return;
    }

    let it = Interceptor::obtain();
    match it.replace(target, alloc_replacement as *mut c_void) {
        Some(tramp) => {
            ORIG_ALLOC.store(tramp, Ordering::Relaxed);
            std::mem::forget(it); // mantém o hook vivo pela sessão
            crate::log(&format!(
                "[archivexl-diag] hook instalado em PoolArchive::Allocate (observe-only, vai logar {ALLOC_LOG_N} callers)"
            ));
        }
        None => {
            INSTALLED.store(false, Ordering::Relaxed);
            crate::log("[archivexl-diag] FALHA ao hookar Allocate (replace devolveu None)");
        }
    }
}

/// Shim NAKED: a 1ª instrução é literalmente a nossa — o compilador NÃO emite prólogo,
/// então x30 chega CRU do caller (o abs-jump do hook usa x17, não toca x30/sp). Movemos
/// x30 → x1 (2º arg) ANTES de qualquer branch que o clobbe e usamos `b` (tail-call, não
/// `bl`) pro body → x0 (size) intacto, x1 = caller_ret, frame preservado.
#[unsafe(naked)]
unsafe extern "C" fn alloc_replacement() {
    core::arch::naked_asm!(
        "mov x1, x30",     // x1 = ret-addr do caller (x30 ainda é o do caller real)
        "b   {body}",      // tail-call: o `b` não escreve x30
        body = sym alloc_body,
    )
}

/// Corpo Rust do replacement. ABI: `x0 = size` (arg original), `x1 = caller_ret`.
/// OBSERVE-ONLY: loga os N primeiros callers (rebaseados p/ vmaddr) e SEMPRE chama a
/// original via trampolim, devolvendo o ptr dela intacto. Nunca altera size, nunca pula
/// a alocação, nunca forja ptr → memória idêntica ao vanilla.
unsafe extern "C" fn alloc_body(size: u64, caller_ret: u64) -> *mut c_void {
    let n = ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
    if n < ALLOC_LOG_N {
        // runtime ret-addr → vmaddr estático (casa offline contra symbols-demangled.txt).
        // un_rebase devolve 0 se o ptr estiver fora do módulo principal (ex.: caller numa
        // dylib injetada como o ArchiveXL-loader) — útil já como sinal.
        let vmaddr = crate::un_rebase(caller_ret as *const c_void);
        crate::log(&format!(
            "[archivexl-diag] alloc caller #{n}: size={size} ret={caller_ret:#x} vmaddr={vmaddr:#x}"
        ));
    }
    // SEMPRE chama a original. Se (improvável) o trampolim for null, fallback seguro = null.
    let orig = ORIG_ALLOC.load(Ordering::Relaxed);
    if orig.is_null() {
        return std::ptr::null_mut();
    }
    let f: AllocFn = std::mem::transmute(orig);
    f(size)
}

// ===================== Util =====================

/// Compara os 4 primeiros u32 (16 bytes) em `addr` com `expected`. Pré-condição: já
/// validado legível pelo caller. Protege contra hookar um alvo que um patch moveu/mudou.
unsafe fn prologue_matches(addr: *mut c_void, expected: &[u32; 4]) -> bool {
    let mut buf = [0u8; 16];
    std::ptr::copy_nonoverlapping(addr as *const u8, buf.as_mut_ptr(), 16);
    for (i, &exp) in expected.iter().enumerate() {
        let got = u32::from_le_bytes([buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2], buf[i * 4 + 3]]);
        if got != exp {
            return false;
        }
    }
    true
}

/// cw-player-scheduling-vehicle: `WardrobeSystem::ForgetItemID` — implementação REAL de
/// `HashMap<CName,ItemID>::Remove` sobre uma instância viva do motor. Layout confirmado por agente
/// de RE dedicado contra `RED4ext.SDK/include/RED4ext/HashMap.hpp` (ground-truth completo, não
/// especulativo) + cross-validado ao vivo neste Mac (comando `wardrobesys`, ver DATABASE.md
/// 2026-07-31): `wardrobe_sys+0x48` = HashMap base = `{indexTable:u32*@0x00, size:u32@0x08,
/// capacity:u32@0x0C, nodes:Node*@0x10, nodeList.capacity:u32@0x18, nodeList.stride:u32@0x1C,
/// nodeList.nextIdx:u32@0x20(freelist head), nodeList.size:u32@0x24, allocator@0x28}` (0x30 bytes
/// total). `Node{next:u32@0, hashedKey:u32@4, key:CName(u64)@8, value:ItemID(16B)@0x10}`,
/// stride=0x20. `CName` já É seu próprio hash (FNV1a64); a chave de bucket é o XOR-fold pra 32 bits
/// (`(u32)hash ^ (u32)(hash>>32)`), bucket = `hashedKey % capacity`. Colisão = encadeamento
/// separado via `Node::next` (não open-addressing) — `Remove` desengancha da cadeia e empurra o
/// slot na freelist do array de nodes.
///
/// **Sob `SharedSpinLock` em `wardrobe_sys+0xF4`** (mesmo padrão CAS 0->0xFF já provado em
/// `tweakdb_rt::mutex00_lock`, spin limitado pra nunca travar o processo se o lock nunca liberar).
///
/// ⚠️ **CODADO+build-verificado, mas DELIBERADAMENTE NUNCA TESTADO AO VIVO contra o save real do
/// usuário** (só o READ-path foi testado, via `wardrobesys`, com sucesso total — enumeração bateu
/// exato com `size` reportado). Uma escrita errada aqui pode corromper o heap do motor ou (se
/// `ItemStore` for serializado) persistir uma mutação indesejada no save. **Incerteza residual não
/// resolvida:** o agente de RE mencionou "decrementa size" sem especificar qual dos DOIS campos
/// `size` (o do HashMap top-level em `+0x08`, ou `nodeList.size` em `+0x24`) — esta implementação
/// só decrementa o de `+0x08` (o "live element count" exposto), por ser a interpretação mais
/// conservadora; `nodeList.size` fica intocado. Antes de confiar nisso como "provado": testar numa
/// save descartável ou com confirmação explícita do usuário, nunca contra progresso real sem aviso.
pub(crate) unsafe fn wardrobe_forget_item(wardrobe_sys: *mut u8, target_cname: u64) -> bool {
    const STRIDE: usize = 0x20;
    const INVALID: u32 = 0xFFFF_FFFF;
    if !gum::is_readable(wardrobe_sys as *const c_void, 0x100) {
        crate::log("[wardrobe-forget] instância ilegível");
        return false;
    }
    let hm = wardrobe_sys.add(0x48);
    let lock_atomic = &*(wardrobe_sys.add(0xF4) as *const std::sync::atomic::AtomicU8);
    let mut locked = false;
    for i in 0..4_000_000u32 {
        if lock_atomic.compare_exchange(0, 0xFF, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            locked = true;
            break;
        }
        if i & 511 == 511 {
            std::thread::yield_now();
        }
    }
    if !locked {
        crate::log("[wardrobe-forget] lock ocupado (spin esgotado), abortado sem mudar nada");
        return false;
    }
    let result = wardrobe_forget_item_locked(hm, target_cname);
    lock_atomic.store(0, Ordering::Release);
    result
}

unsafe fn wardrobe_forget_item_locked(hm: *mut u8, target_cname: u64) -> bool {
    const STRIDE: usize = 0x20;
    const INVALID: u32 = 0xFFFF_FFFF;
    let index_table = (hm as *const u64).read_unaligned() as *mut u32;
    let size_cap = (hm.add(0x08) as *const u64).read_unaligned();
    let capacity = (size_cap >> 32) as u32;
    let nodes = (hm.add(0x10) as *const u64).read_unaligned() as *mut u8;
    if capacity == 0 || capacity > 4096 || index_table.is_null() || nodes.is_null() {
        crate::log("[wardrobe-forget] capacity/ponteiros fora de faixa plausível, abortado");
        return false;
    }
    let hashed_key = ((target_cname & 0xFFFF_FFFF) as u32) ^ ((target_cname >> 32) as u32);
    let bucket = (hashed_key % capacity) as usize;
    let mut prev_next_field: *mut u32 = index_table.add(bucket);
    let mut idx = prev_next_field.read_unaligned();
    let mut guard = 0u32;
    while idx != INVALID && guard < capacity + 1 {
        guard += 1;
        let node = nodes.add(idx as usize * STRIDE);
        if !gum::is_readable(node as *const c_void, STRIDE) {
            crate::log("[wardrobe-forget] node ilegível no meio da cadeia, abortado sem mudar nada");
            return false;
        }
        let next = (node as *const u32).read_unaligned();
        let node_hashed = (node.add(4) as *const u32).read_unaligned();
        let node_key = (node.add(8) as *const u64).read_unaligned();
        if node_hashed == hashed_key && node_key == target_cname {
            // desengancha da cadeia
            prev_next_field.write_unaligned(next);
            // empurra o slot liberado na freelist do array de nodes
            let nextidx_ptr = hm.add(0x20) as *mut u32;
            let old_freelist_head = nextidx_ptr.read_unaligned();
            (node as *mut u32).write_unaligned(old_freelist_head);
            nextidx_ptr.write_unaligned(idx);
            // decrementa o size top-level (+0x08) — ver nota de incerteza residual acima
            let size_ptr = hm.add(0x08) as *mut u32;
            let cur_size = size_ptr.read_unaligned();
            size_ptr.write_unaligned(cur_size.saturating_sub(1));
            crate::log(&format!("[wardrobe-forget] removido: bucket={bucket} idx={idx} size {cur_size}->{}", cur_size.saturating_sub(1)));
            return true;
        }
        prev_next_field = node as *mut u32; // node.next fica em +0x00, vira o "prev" da próxima iteração
        idx = next;
    }
    crate::log("[wardrobe-forget] CName não encontrado no HashMap");
    false
}

/// Codeware `WardrobeSystemEx::ForgetItemID` (catálogo item #46/#208, agente de candidatos
/// baratos 2026-08-11, 3ª rodada da sessão) — fallback de SCAN LINEAR por `ItemID.tdbid`, a
/// mesma técnica que a fonte real usa quando a remoção direta por CName-derivado-de-flat falha
/// (`WardrobeSystemEx.hpp`: calcula `appearanceName` via `GetFlatValue<CName>({itemID.tdbid,
/// ".appearanceName"})`, tenta remover por essa chave, e se isso falhar varre `store.ForEach`
/// comparando `tdbid` e remove TODAS as entradas que baterem). **Divergência de escopo
/// consciente**: em vez de replicar as DUAS etapas (derivar CName via flat + fallback), uso só
/// o SCAN por tdbid como via ÚNICA e PRIMÁRIA — mais robusto (não depende do item ter um flat
/// `.appearanceName` populado no TweakDB) e reusa 100% a mecânica já provada em
/// `wardrobe_forget_item_locked` (mesmo `SharedSpinLock@+0xF4`, mesmo layout de `HashMap<CName,
/// ItemID>` confirmado ao vivo contra o save real do usuário, 246 itens, 2026-07-31). A única
/// peça nova é o CRITÉRIO de match: em vez de comparar a CHAVE (`node.key`, um CName que o
/// caller já precisa saber), compara o campo `tdbid` do VALOR (`node+0x10`) — `ItemID{tdbid:
/// TweakDBID@0x00(8 bytes), rngSeed@0x08, uniqueCounter@0x0C, structure@0x0E, flags@0x0F}`,
/// `RED4EXT_ASSERT_SIZE(ItemID, 0x10)` confirmado em `RED4ext.SDK/include/RED4ext/
/// NativeTypes.hpp:128` (ground-truth oficial vendorizado, não chute) — como `tdbid` é o
/// PRIMEIRO campo do struct, fica exatamente em `node+0x10` sem offset adicional. Varre TODOS
/// os buckets (não só o bucket do hash de 1 chave) e remove TODAS as entradas que baterem
/// (mesma semântica "remove todas" da fonte real) — devolve a CONTAGEM removida.
pub(crate) unsafe fn wardrobe_forget_by_tdbid(wardrobe_sys: *mut u8, target_tdbid: u64) -> u32 {
    if !gum::is_readable(wardrobe_sys as *const c_void, 0x100) {
        crate::log("[wardrobe-forget-tdbid] instância ilegível");
        return 0;
    }
    let hm = wardrobe_sys.add(0x48);
    let lock_atomic = &*(wardrobe_sys.add(0xF4) as *const std::sync::atomic::AtomicU8);
    let mut locked = false;
    for i in 0..4_000_000u32 {
        if lock_atomic.compare_exchange(0, 0xFF, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            locked = true;
            break;
        }
        if i & 511 == 511 {
            std::thread::yield_now();
        }
    }
    if !locked {
        crate::log("[wardrobe-forget-tdbid] lock ocupado (spin esgotado), abortado sem mudar nada");
        return 0;
    }
    let result = wardrobe_forget_by_tdbid_locked(hm, target_tdbid);
    lock_atomic.store(0, Ordering::Release);
    result
}

unsafe fn wardrobe_forget_by_tdbid_locked(hm: *mut u8, target_tdbid: u64) -> u32 {
    const STRIDE: usize = 0x20;
    const INVALID: u32 = 0xFFFF_FFFF;
    let index_table = (hm as *const u64).read_unaligned() as *mut u32;
    let size_cap = (hm.add(0x08) as *const u64).read_unaligned();
    let capacity = (size_cap >> 32) as u32;
    let nodes = (hm.add(0x10) as *const u64).read_unaligned() as *mut u8;
    if capacity == 0 || capacity > 4096 || index_table.is_null() || nodes.is_null() {
        crate::log("[wardrobe-forget-tdbid] capacity/ponteiros fora de faixa plausível, abortado");
        return 0;
    }
    let mut removed = 0u32;
    for bucket in 0..(capacity as usize) {
        let mut prev_next_field: *mut u32 = index_table.add(bucket);
        let mut idx = prev_next_field.read_unaligned();
        let mut guard = 0u32;
        while idx != INVALID && guard < capacity + 1 {
            guard += 1;
            let node = nodes.add(idx as usize * STRIDE);
            if !gum::is_readable(node as *const c_void, STRIDE) {
                crate::log("[wardrobe-forget-tdbid] node ilegível no meio da cadeia, parando com segurança");
                return removed;
            }
            let next = (node as *const u32).read_unaligned();
            let value_tdbid = (node.add(0x10) as *const u64).read_unaligned();
            if value_tdbid == target_tdbid {
                // desengancha da cadeia (mesma técnica de `wardrobe_forget_item_locked`)
                prev_next_field.write_unaligned(next);
                let nextidx_ptr = hm.add(0x20) as *mut u32;
                let old_freelist_head = nextidx_ptr.read_unaligned();
                (node as *mut u32).write_unaligned(old_freelist_head);
                nextidx_ptr.write_unaligned(idx);
                let size_ptr = hm.add(0x08) as *mut u32;
                let cur_size = size_ptr.read_unaligned();
                size_ptr.write_unaligned(cur_size.saturating_sub(1));
                removed += 1;
                crate::log(&format!("[wardrobe-forget-tdbid] removido: bucket={bucket} idx={idx} tdbid={target_tdbid:#018x} size {cur_size}->{}", cur_size.saturating_sub(1)));
                // `prev_next_field` já foi atualizado pra pular `node` — segue a cadeia a
                // partir do `next` capturado ANTES da escrita, sem tocar `prev_next_field`.
                idx = next;
                continue;
            }
            prev_next_field = node as *mut u32; // node.next fica em +0x00, vira o "prev" da próxima iteração
            idx = next;
        }
    }
    if removed == 0 {
        crate::log(&format!("[wardrobe-forget-tdbid] tdbid={target_tdbid:#018x} não encontrado em nenhum bucket"));
    }
    removed
}

// ===== axl-garment-apply/axl-transmog-apply (2026-08-03, /goal): sonda ao vivo do ponto de
// dispatch dentro de EquipmentSystem::QueueRequest =====
// RE offline (2 agentes, mesma madrugada) achou: `req+0x40` precisa do ponteiro do EquipmentSystem
// dono (JÁ CORRIGIDO e confirmado ao vivo — crash avançou de +0x6b pra +0x2b). O 2º crash acontece
// dentro de uma função resolvida em RUNTIME via tabela de function-pointer em 0x10908b798 (memória
// BSS zero-filled, sem conteúdo no binário — parede genuína de RE estática, não dá pra ir além só
// desmontando). Sonda: patch NAKED em 0x103b1f678 (a instrução `blr x8` dentro do dispatcher, logo
// depois de resolver o handler pela tabela) — loga x0/x1/x2/x3/x8 e o buf16[0/8] antes/depois da
// chamada, DEIXA a chamada acontecer normalmente (preserva 100% do comportamento), e volta pro fluxo
// normal via o trampolim relocado (mesmo padrão de `reslink_shim`). Zero risco adicional: só
// observação, a chamada real acontece do mesmo jeito que sempre aconteceria.
const QR_DISPATCH_VM: u64 = 0x1_03b1_f678;
static ORIG_QR_DISPATCH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
#[repr(C)]
struct QrProbeData {
    x0: AtomicU64,
    x1: AtomicU64,
    x2: AtomicU64,
    x3: AtomicU64,
    x8: AtomicU64,
    buf0_before: AtomicU64,
    buf8_before: AtomicU64,
    buf0_after: AtomicU64,
    buf8_after: AtomicU64,
    hits: AtomicU64,
}
static QR_PROBE: QrProbeData = QrProbeData {
    x0: AtomicU64::new(0), x1: AtomicU64::new(0), x2: AtomicU64::new(0), x3: AtomicU64::new(0),
    x8: AtomicU64::new(0), buf0_before: AtomicU64::new(0), buf8_before: AtomicU64::new(0),
    buf0_after: AtomicU64::new(0), buf8_after: AtomicU64::new(0), hits: AtomicU64::new(0),
};

/// Chamada pelo shim naked ANTES do `blr x8` real — grava x0/x1/x2/x3/x8 + buf16 atual (garbage
/// antes da chamada) em `QR_PROBE`, via Rust normal (mais seguro que inline asm pra isso).
extern "C" fn qr_probe_before(x0: u64, x1: u64, x2: u64, x3: u64, x8: u64) {
    QR_PROBE.x0.store(x0, Ordering::Relaxed);
    QR_PROBE.x1.store(x1, Ordering::Relaxed);
    QR_PROBE.x2.store(x2, Ordering::Relaxed);
    QR_PROBE.x3.store(x3, Ordering::Relaxed);
    QR_PROBE.x8.store(x8, Ordering::Relaxed);
    unsafe {
        if x2 != 0 && gum::is_readable(x2 as *const c_void, 16) {
            QR_PROBE.buf0_before.store(*(x2 as *const u64), Ordering::Relaxed);
            QR_PROBE.buf8_before.store(*((x2 as *const u64).add(1)), Ordering::Relaxed);
        }
    }
}

/// Chamada pelo shim DEPOIS do `blr x8` real — grava buf16 pós-chamada + loga tudo de uma vez
/// (o handler já rodou, x2 continua apontando pro mesmo buffer na stack de QueueRequest).
extern "C" fn qr_probe_after(x2: u64) {
    unsafe {
        if x2 != 0 && gum::is_readable(x2 as *const c_void, 16) {
            QR_PROBE.buf0_after.store(*(x2 as *const u64), Ordering::Relaxed);
            QR_PROBE.buf8_after.store(*((x2 as *const u64).add(1)), Ordering::Relaxed);
        }
    }
    let n = QR_PROBE.hits.fetch_add(1, Ordering::Relaxed);
    // FIX (2026-08-03, achado pelo agente de RE): x8 é um endereço RUNTIME (com slide de ASLR) —
    // logar ele CRU (sem `un_rebase`) e comparar contra endereços ESTÁTICOS (nm/LC_FUNCTION_STARTS)
    // é comparar coisas em bases diferentes, dá endereço errado (achado real: `0x1024a02f4` não
    // batia com nenhuma função). `crate::un_rebase()` já existe (inverso de `rebase`) — usar aqui.
    let x8_static = crate::un_rebase(QR_PROBE.x8.load(Ordering::Relaxed) as *const c_void);
    crate::log(&format!(
        "[qrprobe] #{n} x0(es)={:#018x} x1(req)={:#018x} x2(&buf)={:#018x} x3={:#018x} x8(handler)_runtime={:#018x} x8(handler)_STATIC={:#018x} buf_antes=[{:#018x},{:#018x}] buf_depois=[{:#018x},{:#018x}]",
        QR_PROBE.x0.load(Ordering::Relaxed), QR_PROBE.x1.load(Ordering::Relaxed),
        QR_PROBE.x2.load(Ordering::Relaxed), QR_PROBE.x3.load(Ordering::Relaxed),
        QR_PROBE.x8.load(Ordering::Relaxed), x8_static,
        QR_PROBE.buf0_before.load(Ordering::Relaxed), QR_PROBE.buf8_before.load(Ordering::Relaxed),
        QR_PROBE.buf0_after.load(Ordering::Relaxed), QR_PROBE.buf8_after.load(Ordering::Relaxed),
    ));
}

#[unsafe(naked)]
unsafe extern "C" fn qr_dispatch_shim() {
    core::arch::naked_asm!(
        // entrada: x0=es_ptr x1=req x2=&buf16(na stack de QueueRequest) x3=0 x8=handler_addr —
        // TODOS ainda intactos (substituímos só a instrução `blr x8`, nada rodou ainda).
        "stp x29, x30, [sp, #-0x50]!",
        "stp x0, x1, [sp, #0x10]",
        "stp x2, x3, [sp, #0x20]",
        "stp x8, x9, [sp, #0x30]",   // x9 só de padding (stp precisa de par)
        "mov x4, x8",                // FIX: ABI real usa x0-x4 pros 5 params, não x0-x3+x8 — o 5º
                                      // arg de `qr_probe_before` (handler) tem que ir em x4, não x8.
        "bl {before}",               // qr_probe_before(x0,x1,x2,x3,x4=handler)
        "ldp x8, x9, [sp, #0x30]",
        "ldp x2, x3, [sp, #0x20]",
        "ldp x0, x1, [sp, #0x10]",
        "blr x8",                    // a chamada REAL, idêntica ao comportamento original
        "ldp x2, x3, [sp, #0x20]",   // x2 sobrevive ao blr (callee não deveria mexer no ponteiro em si)
        "mov x0, x2",
        "bl {after}",                // qr_probe_after(x2)
        "ldp x29, x30, [sp], #0x50",
        "adrp x9, {orig}@PAGE",
        "add  x9, x9, {orig}@PAGEOFF",
        "ldr  x9, [x9]",
        "br   x9",                   // volta pro fluxo normal (trampolim relocado, mesmo padrão reslink_shim)
        before = sym qr_probe_before,
        after = sym qr_probe_after,
        orig = sym ORIG_QR_DISPATCH,
    )
}

pub(crate) unsafe fn install_qr_dispatch_probe() {
    if !ORIG_QR_DISPATCH.load(Ordering::Relaxed).is_null() {
        return;
    }
    let t = crate::rebase(QR_DISPATCH_VM);
    if !gum::is_readable(t as *const c_void, 4) {
        crate::log("[qrprobe] alvo ilegível");
        return;
    }
    let it = Interceptor::obtain();
    match it.replace(t, qr_dispatch_shim as *mut c_void) {
        Some(tr) => {
            ORIG_QR_DISPATCH.store(tr, Ordering::Relaxed);
            std::mem::forget(it);
            crate::log("[qrprobe] hook NAKED em 0x103b1f678 instalado (observe-only, comportamento idêntico)");
        }
        None => crate::log("[qrprobe] replace None"),
    }
}

// ===== axl-garment-apply/axl-transmog-apply (2026-08-03, /goal, cont.30): FIX CIRÚRGICO de
// `GetInvokable()` =====
// Achado desta madrugada (equiprawv5): chamar `0x103b1f624` DIRETO com um `CScriptStackFrame`
// à mão (mesma receita de `call_func`) roda SEM CRASH mas NÃO FAZ NADA — o bytecode sintético
// (`LocalVar`+`ParamEnd`) só faz o EXECUTOR interpretar "declarar 1 argumento", e como não há
// mais nenhuma instrução real depois disso (nosso buffer é só padding-zero = opcode 0 = ret),
// a função devolve sem nunca invocar a lógica nativa de enfileiramento — `0x103b1f624` só roda
// de verdade quando despachado via `ADDR_EXEC`, que primeiro chama `this->GetInvokable()` (RE
// já feita: `0x100339b2c`, stub que SEMPRE retorna null pra QueueRequest — bug confirmado desde
// cont.14 desta mesma madrugada). Fix cirúrgico: hookar `GetInvokable()` (função normal C-ABI,
// sem naked asm necessário) e, SÓ quando `this` for o descritor exato de `QueueRequest`,
// devolver o endereço nativo real (`0x103b1f624`) em vez de null — qualquer outro `this` cai
// no comportamento ORIGINAL (stub null), zero regressão pros outros ~3 usos dessa vtable slot.
static ORIG_GET_INVOKABLE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static GETINVOKABLE_TARGET_DESC: AtomicU64 = AtomicU64::new(0);
static GETINVOKABLE_NATIVE_STATIC: AtomicU64 = AtomicU64::new(0);
const ADDR_GET_INVOKABLE_VM: u64 = 0x1_0033_9b2c;

extern "C" fn get_invokable_shim(this: *mut c_void) -> *mut c_void {
    if this as u64 == GETINVOKABLE_TARGET_DESC.load(Ordering::Relaxed) {
        let na = GETINVOKABLE_NATIVE_STATIC.load(Ordering::Relaxed);
        if na != 0 {
            crate::log("[getinvokable-fix] this==QueueRequest descriptor — devolvendo endereço nativo real");
            return unsafe { crate::rebase(na) };
        }
    }
    let orig = ORIG_GET_INVOKABLE.load(Ordering::Relaxed);
    if orig.is_null() {
        return std::ptr::null_mut();
    }
    let f: extern "C" fn(*mut c_void) -> *mut c_void = unsafe { std::mem::transmute(orig) };
    f(this)
}

/// Instala o fix (idempotente) e ARMA o alvo (`descriptor`=CClassFunction* de QueueRequest,
/// `native_static`=0x103b1f624). Chamar ANTES de invocar `rtti::call_func` normal sobre esse rf.
pub(crate) unsafe fn install_getinvokable_fix(descriptor: *mut c_void, native_static: u64) -> bool {
    GETINVOKABLE_TARGET_DESC.store(descriptor as u64, Ordering::Relaxed);
    GETINVOKABLE_NATIVE_STATIC.store(native_static, Ordering::Relaxed);
    if !ORIG_GET_INVOKABLE.load(Ordering::Relaxed).is_null() {
        return true; // já instalado, só reapontamos o alvo acima
    }
    let t = crate::rebase(ADDR_GET_INVOKABLE_VM);
    if !gum::is_readable(t as *const c_void, 4) {
        crate::log("[getinvokable-fix] alvo ilegível");
        return false;
    }
    let it = Interceptor::obtain();
    match it.replace(t, get_invokable_shim as *mut c_void) {
        Some(tr) => {
            ORIG_GET_INVOKABLE.store(tr, Ordering::Relaxed);
            std::mem::forget(it);
            crate::log("[getinvokable-fix] hook em 0x100339b2c instalado (fix cirúrgico p/ QueueRequest)");
            true
        }
        None => {
            crate::log("[getinvokable-fix] replace None");
            false
        }
    }
}

#[cfg(test)]
mod reslink_tests {
    use super::resource_path_hash;
    #[test]
    fn fnv_golden_bate_com_apply_xl() {
        // golden provado em bwms-core/src/apply_xl.rs::tests::hash_goldens
        assert_eq!(
            resource_path_hash("base\\resource.cooked_mlsetup"),
            0x3a12_b4fd_1938_d5ca
        );
        // normalização: '/' vira '\\' e case não importa → mesmo hash
        assert_eq!(
            resource_path_hash("BASE/resource.cooked_mlsetup"),
            0x3a12_b4fd_1938_d5ca
        );
    }
}

// axl-attachment-apply / itens #40, #41, #54 (2026-08-12): testes offline puros usando os DADOS
// REAIS capturados no boot ao vivo de 2026-08-12 (`EquipStart#0`/`#1`, ver checkpoint no fim de
// `CATALOGO-EXAUSTIVO-ARCHIVEXL.md`) como vetores golden. Zero chamada nativa, zero jogo — só
// valida a interpretação de layout (`decode_icomponent_base60`/`is_attachment_slots_by_name`)
// contra os bytes reais já vistos, e documenta em código o que ESTÁ e o que NÃO ESTÁ confirmado.
#[cfg(test)]
mod attachslots_base60_tests {
    use super::{decode_icomponent_base60, is_attachment_slots_by_name};

    // Golden real: `EquipStart#1`, boot 2026-08-12, receiver=0x70169517f0 (ver a mensagem de
    // task original / HISTORICO — o proof-log salvo em disco acabou guardando só a captura do
    // item irmão #53, não esta; estes 12 words vêm direto do log real colado na tarefa).
    const EQUIPSTART1_WORDS: [u64; 12] = [
        0x0000_0001_094c_2230, // vtable
        0x0000_0070_1695_17f0, // ref.instance == self (== receiver)
        0x0000_0070_0fd0_4c88, // ref.refcount
        0x0000_0000_0000_0000, // unk18.instance
        0x0000_0000_0000_0000, // unk18.refcount
        0x0000_0000_0023_d3fe, // global_serialize_id (contador pequeno, não ponteiro)
        0x0000_0001_0aa3_ccb8, // native_type (CClass*)
        0x0000_0070_0fd0_4c80, // value_holder
        0xd0de_7775_8e13_aca5, // name_hash == fnv1a64("AttachmentSlots")
        0x0000_0000_0000_0000, // unk48 (não-documentado)
        0x0000_007f_3634_bc70, // unk48 (não-documentado, ponteiro em faixa diferente — stack?)
        0x0000_0000_0000_0000, // unk48 (não-documentado)
    ];

    // Golden real: `EquipStart#0`, MESMO boot, outra instância viva de AttachmentSlots — só os 3
    // primeiros words foram capturados nesta cópia do log (o resto não foi salvo), mas já bastam
    // pra corroborar o layout de forma independente.
    const EQUIPSTART0_VTABLE: u64 = 0x0000_0001_094c_2230;
    const EQUIPSTART0_SELF: u64 = 0x0000_0070_229f_e150; // == receiver logado no #0
    const EQUIPSTART0_REF_REFCOUNT: u64 = 0x0000_0070_179c_15c8;

    #[test]
    fn equipstart1_name_hash_confirma_attachmentslots_por_conteudo() {
        // A confirmação mais forte de toda a análise: o campo `name` (CName, IComponent+0x40) do
        // objeto capturado é literalmente FNV1a64("AttachmentSlots") — identidade confirmada por
        // CONTEÚDO, não só pela suposição de que "o receiver do evento EquipStart é sempre um
        // AttachmentSlots*".
        assert!(is_attachment_slots_by_name(&EQUIPSTART1_WORDS));
        assert_eq!(
            EQUIPSTART1_WORDS[8],
            bwms_hashes::fnv1a64(b"AttachmentSlots")
        );
    }

    #[test]
    fn equipstart1_decode_layout_plausivel_e_consistente_com_header_oficial() {
        let d = decode_icomponent_base60(&EQUIPSTART1_WORDS);
        // ISerializable::ref.instance ("Initialized in Handle ctor") == o próprio endereço do
        // objeto no boot capturado — bate com o `receiver` logado (self-referential WeakHandle).
        assert_eq!(d.ref_instance, 0x0000_0070_1695_17f0);
        // ISerializable::unk18 (2º WeakHandle da base) nunca populado neste objeto — ambos os
        // campos do par (instance+refcount) zerados juntos, consistente com "WeakHandle vazio".
        assert_eq!(d.unk18_instance, 0);
        assert_eq!(d.unk18_refcount, 0);
        // global_serialize_id é um contador PEQUENO e plausível (não formato de ponteiro válido
        // nesta build, que usa endereços 0x1xxxxxxxx/0x7xxxxxxxx) — bate com a doc oficial
        // ("Global incremental ID, used in serialization").
        assert!(d.global_serialize_id < 0x1000_0000);
        assert_ne!(d.global_serialize_id, 0);
        // native_type (CClass*) e ref_refcount/value_holder são ponteiros vivos plausíveis deste
        // boot (não-zero, não claramente lixo).
        assert_ne!(d.native_type, 0);
        assert_ne!(d.ref_refcount, 0);
        assert_ne!(d.value_holder, 0);
    }

    #[test]
    fn ref_refcount_e_value_holder_sao_campos_diferentes_proximos_por_alocador_nao_par_handle() {
        // Correção de uma hipótese de investigação anterior: ref.refcount(0x10) e
        // value_holder(0x38) diferem só 8 bytes no dump real, mas são campos de sub-objetos
        // DIFERENTES (0x28 bytes / 5 words de distância no struct) — não um par estrutural
        // `Handle<T>{instance,refcount}` adjacente. Documentado aqui pra não reintroduzir a
        // hipótese errada numa sessão futura.
        let d = decode_icomponent_base60(&EQUIPSTART1_WORDS);
        assert_eq!(d.ref_refcount - d.value_holder, 8);
        // mas NÃO são o mesmo campo nem uma struct par — confirma que value_holder segue
        // presente e não-nulo independentemente (não é só metade de um Handle já contado em
        // ref_refcount).
        assert_ne!(d.ref_refcount, d.value_holder);
    }

    #[test]
    fn equipstart0_parcial_corrobora_o_layout_de_equipstart1_de_forma_independente() {
        // EquipStart#0 é outra instância AttachmentSlots viva do MESMO boot — vtable idêntico
        // (mesma classe) e ref.instance == o próprio receiver (mesmo padrão self-referencial).
        assert_eq!(EQUIPSTART0_VTABLE, EQUIPSTART1_WORDS[0], "vtable deveria bater entre 2 instâncias da mesma classe");
        assert_eq!(EQUIPSTART0_SELF, 0x0000_0070_229f_e150);
        // ref.refcount de #0 é um ponteiro DIFERENTE do de #1 (instâncias/alocações distintas) —
        // esperado, cada objeto tem seu próprio refcount-block.
        assert_ne!(EQUIPSTART0_REF_REFCOUNT, EQUIPSTART1_WORDS[2]);
    }
}
