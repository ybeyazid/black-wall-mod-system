//! API C-ABI exposta aos plugins Rust (`BwmsApi`). O plugin recebe um `*const BwmsApi`
//! no entry e chama a vtable pra HOOKAR e REFLETIR sem linkar nada do nosso crate.
//!
//! v1 (ALFA): `log` + `vtable_hook`/`vtable_unhook` (o motor de hook = RED4ext, já provado
//! in-game, COW em __DATA_CONST) + `field_ptr`/`call_method` (reflection por nome = Codeware
//! getf/setf/callf). Roadmap v2: inline hook, `register_native`, ImGui — campos novos SÓ no
//! fim da struct + `abi_version` cresce (ABI estável pra plugins antigos).

use std::ffi::{c_void, CStr};
use std::os::raw::c_char;

/// Vtable C-ABI passada ao plugin. `#[repr(C)]` => layout estável; o plugin declara a MESMA
/// struct (idêntica) e chama os ponteiros. Campos novos entram só no FIM (compat pra frente).
#[repr(C)]
pub struct BwmsApi {
    /// Versão da ABI (= `plugins::BWMS_PLUGIN_API`). O plugin pode checar antes de usar campos novos.
    pub abi_version: u32,
    /// Escreve no log do BWMS (`/tmp/cp77-console.log`). `msg` = C-string UTF-8.
    pub log: unsafe extern "C" fn(*const c_char),
    /// Hook de VTABLE: troca o slot `slot_idx` (índice em u64) de `vtbl` por `repl`; devolve o
    /// ponteiro ORIGINAL do slot (pra encadear). null = falhou. Não patcha __TEXT (COW na vtable).
    pub vtable_hook: unsafe extern "C" fn(vtbl: *mut u64, slot_idx: usize, repl: *const c_void) -> *const c_void,
    /// Desfaz um `vtable_hook` (restaura `orig` no slot).
    pub vtable_unhook: unsafe extern "C" fn(vtbl: *mut u64, slot_idx: usize, orig: *const c_void),
    /// Endereço do CAMPO `field` (por nome, via RTTI) no objeto vivo `obj`. null = não achou.
    /// O plugin lê/escreve direto nesse ponteiro — é o getf/setf cru.
    pub field_ptr: unsafe extern "C" fn(obj: *mut c_void, field: *const c_char) -> *mut c_void,
    /// Chama o método `method` SEM args em `obj` (por nome); escreve 16 bytes de retorno em
    /// `ret16` (pode ser null). true = chamou. (call com args tipados = futuro.)
    pub call_method: unsafe extern "C" fn(obj: *mut c_void, method: *const c_char, ret16: *mut u8) -> bool,
    // --- v2 (abi_version 2): hook INLINE (qualquer função, não só método virtual) ---
    /// Hook inline: troca `target` por `repl`; devolve o TRAMPOLIM (chame-o pra invocar a
    /// função original) ou null. Relocator arm64 + COW em __TEXT (provado in-game).
    pub inline_hook: unsafe extern "C" fn(target: *mut c_void, repl: *mut c_void) -> *mut c_void,
    /// Desfaz um `inline_hook` em `target`.
    pub inline_revert: unsafe extern "C" fn(target: *mut c_void),
    // --- v3 (abi_version 3): registrar native global no RTTI (Codeware) ---
    /// Registra uma native global no RTTI chamável do redscript: `full`=nome completo, `short`=nome
    /// curto, `handler`=`extern "C" fn(ctx, frame, ret, a4)`. Devolve true se re-resolve OK.
    /// (Sem-args por ora; argful/method = roadmap.) PROVADO internamente (register_all/BlackwallPing).
    pub register_native: unsafe extern "C" fn(
        full: *const c_char,
        short: *const c_char,
        handler: crate::register::NativeHandler,
    ) -> bool,
    // --- v4 (abi_version 4): API simétrica ao núcleo — nativa COM ARGS + emitir evento ---
    /// Registra uma native global COM ARGS chamável do redscript. `param_types` = array de
    /// `n_params` C-strings com os NOMES DOS TIPOS RED (ex.: `"Float"`, `"CName"`, `"Int32"`).
    /// O handler lê os args via o frame (padrão read_params). Devolve true se re-resolve OK.
    /// Fecha a assimetria: antes o plugin só registrava nativa SEM-args (o núcleo já fazia argful).
    pub register_native_argful: unsafe extern "C" fn(
        full: *const c_char,
        short: *const c_char,
        handler: crate::register::NativeHandler,
        param_types: *const *const c_char,
        n_params: usize,
    ) -> bool,
    /// Dispara um evento do CallbackSystem por nome (ex.: `"Input/Custom"`): chama todos os
    /// callbacks registrados nesse evento. Devolve quantos dispararam. Deixa um plugin EMITIR
    /// eventos que mods (redscript/plugin) escutam — a outra ponta do RegisterCallback.
    pub fire_event: unsafe extern "C" fn(name: *const c_char) -> usize,
    // --- v5 (abi_version 5): Reflection TIPADA (get/set de campo por nome, sem ponteiro cru) ---
    /// Lê um campo Float por nome no objeto vivo. `0.0` se não achar (o plugin não faz cast de
    /// ponteiro na mão como no `field_ptr`). Parity com o getf interno.
    pub prop_get_f32: unsafe extern "C" fn(obj: *mut c_void, field: *const c_char) -> f32,
    /// Escreve um campo Float por nome. Devolve true se achou+escreveu.
    pub prop_set_f32: unsafe extern "C" fn(obj: *mut c_void, field: *const c_char, val: f32) -> bool,
    /// Lê um campo Int32/Uint32 por nome (0 se não achar).
    pub prop_get_i32: unsafe extern "C" fn(obj: *mut c_void, field: *const c_char) -> i32,
    // --- v6 (abi_version 6): call de método COM ARGS tipados (parity com o callf interno) ---
    /// Chama `method` em `obj` com `n_args` args tipados. Cada `args[i]` é uma C-string no formato
    /// do loop de cmd: `i:5` (Int32), `f:1.5` (Float), `b:true` (Bool), `n:Nome` (CName), `s:txt`
    /// (String), `e:3` (Enum). Escreve 16B de retorno em `ret16` (pode ser null). true = chamou.
    /// Fecha o gap do `call_method` sem-args: reusa `parse_cmd_arg` + `call_func` (callf, provados).
    pub call_method_args: unsafe extern "C" fn(
        obj: *mut c_void,
        method: *const c_char,
        args: *const *const c_char,
        n_args: usize,
        ret16: *mut u8,
    ) -> bool,
    // --- v7 (abi_version 7): registrar MÉTODO novo numa classe EXISTENTE (@addMethod-style) ---
    /// Registra um método novo, chamável do redscript, numa classe REAL já existente (ex.:
    /// GameObject, Entity, gameGodModeSystem — o padrão real do Codeware via `@addMethod`).
    /// `class`=nome da classe-alvo, `full`=nome completo do método, `short`=nome curto,
    /// `handler`=`extern "C" fn(ctx, frame, ret, a4)`. Devolve true se registrou OK. Fecha
    /// `red4ext-register-method-api`: até aqui só `register_native` (função GLOBAL) era exposto
    /// a plugins — isto fecha o caso "estender classe existente", já provado internamente
    /// (register_method/regmethod_selftest, `gameGodModeSystem.BwmsRegTest`, 2026-07-12).
    pub register_method: unsafe extern "C" fn(
        class: *const c_char,
        full: *const c_char,
        short: *const c_char,
        handler: crate::register::NativeHandler,
    ) -> bool,
    // --- v8 (abi_version 8): TweakDB — flat escalar por nome + clone com herança (tweakxl-mod-api) ---
    /// Lê o valor escalar (4 bytes crus em `+0x08` do FlatValue) do flat `name` (formato
    /// `Record.campo`, ex. `"Items.GrenadeIncendiarySticky.deepWaterDepth"`). Devolve os bits em
    /// `out_bits` (o plugin reinterpreta como f32/i32 conforme o tipo do campo); true = achou.
    pub tweakdb_get_flat: unsafe extern "C" fn(name: *const c_char, out_bits: *mut u32) -> bool,
    /// Escreve o valor escalar de `name` (mesmo formato). ⚠️ afeta TODOS os records que
    /// compartilham o mesmo FlatValue (records clonados via `tweakdb_clone_record` sem override
    /// prévio nesse campo). true = achou+escreveu.
    pub tweakdb_set_flat: unsafe extern "C" fn(name: *const c_char, bits: u32) -> bool,
    /// Clona `source` (record existente) como `new_name`, herdando os flats reais (stats) via
    /// `InheritFlats`, e registra o record novo no TweakDB vivo (`CreateRecord`). Assíncrono
    /// (roda numa thread própria, ~1.5s de atraso) — chamar de dentro de um handler de native/
    /// callback é seguro, o resultado aparece no log (`[clone] N flats herdados...`). Sempre
    /// devolve true se os 3 argumentos forem C-strings válidas (aceito para processamento; não
    /// é confirmação de sucesso — ver log).
    pub tweakdb_clone_record: unsafe extern "C" fn(
        class_name: *const c_char,
        source: *const c_char,
        new_name: *const c_char,
    ) -> bool,
    // --- v9 (abi_version 9): logger por nível + SemVer runtime (parte de `red4ext-sdk-plumbing`) ---
    /// Loga `msg` marcado com `level` (0=Trace 1=Debug 2=Info 3=Warning 4=Error 5=Critical —
    /// mesma escala do Logger do RED4ext.SDK real). Níveis fora de 0..5 caem em "Info". Parity
    /// com o `log` sem-nível (v1), que continua funcionando igual.
    pub log_level: unsafe extern "C" fn(level: u8, msg: *const c_char),
    /// Compara duas versões `"major.minor.patch"` (sufixos após o patch são ignorados — não é
    /// SemVer completo com pre-release/build, cobre o caso real de `Codeware.Require("1.2.0")`
    /// checando "a versão instalada é >= a exigida"). Devolve `actual >= required`; strings
    /// malformadas ou faltando componentes tratam a parte ausente como 0. Ex.:
    /// `semver_satisfies("1.2.0", "1.3.0") == true`.
    pub semver_satisfies: unsafe extern "C" fn(required: *const c_char, actual: *const c_char) -> bool,
    // --- v10 (abi_version 10): ImGui pro plugin NÃO-lua desenhar (`cet-imgui-thirdparty`) ---
    /// Registra `cb` pra ser chamado a cada frame DENTRO da janela onDraw (mesmo ponto que os
    /// mods Lua usam via `ImGui.Begin/Text/End`, gated por `overlay::in_draw()` — só roda com o
    /// overlay BWMS aberto). O plugin usa `imgui_begin/imgui_text/imgui_end` (abaixo) de dentro
    /// de `cb` pra desenhar sua PRÓPRIA janela, sem linkar imgui-rs/cimgui — a API crua do Dear
    /// ImGui já roda no NOSSO binário, o plugin só chama os wrappers finos. Multi-registro (Vec
    /// interno) — plugins não pisam uns nos outros. Devolve true (sempre aceita).
    pub register_draw_callback: unsafe extern "C" fn(cb: extern "C" fn()) -> bool,
    /// `ImGui::Begin(title)` — abre uma janela nova. NO-OP (devolve `true`, "está visível") fora
    /// do onDraw. Mesma chamada crua (`igBegin`) que o binding Lua usa.
    pub imgui_begin: unsafe extern "C" fn(title: *const c_char) -> bool,
    /// `ImGui::Text(s)` — texto sem formatação na janela aberta por `imgui_begin`.
    pub imgui_text: unsafe extern "C" fn(text: *const c_char),
    /// `ImGui::End()` — fecha a janela aberta por `imgui_begin`. SEMPRE chamar em par (mesmo se
    /// `imgui_begin` devolveu `false`), igual à API real do Dear ImGui.
    pub imgui_end: unsafe extern "C" fn(),
    // --- v11 (abi_version 11): `scripts.Add(path)` — plugin registra um `.reds` FORA de
    // r6/scripts pro compilador incluir (`red4ext-scripts-add`) ---
    /// Registra `path` (caminho ABSOLUTO de um `.reds` fora de `r6/scripts`) num manifesto
    /// persistente (`~/.bwms-scripts-add.txt`, 1 path por linha, dedup). **NÃO recompila nem
    /// afeta o boot ATUAL** — este processo já carregou o `final.redscripts` que o compile
    /// anterior gerou, ANTES do plugin sequer rodar (redscript não hot-recarrega em runtime,
    /// diferente de Lua/CET). O efeito aparece no PRÓXIMO ciclo compile+boot: o passo de
    /// compile (`bwms-fastboot.sh compile`, ou o instalador) lê o manifesto e passa cada path
    /// via `-compilePathsFile` do `scc` (achado nesta rodada — `scc` aceita uma lista de
    /// arquivos AVULSOS, além do diretório `r6/scripts` normal, no MESMO passe de compilação;
    /// verificado num scratch-copy seguro antes de mexer no deploy real). Devolve `true` se
    /// escreveu (ou já estava) no manifesto; `false` só em erro de I/O real.
    pub scripts_add: unsafe extern "C" fn(path: *const c_char) -> bool,
    // --- v12 (abi_version 12): `GameStates.Add` pragmático (red4ext-gamestates-add) ---
    /// Registra callbacks de ciclo de vida de estado de jogo (parity com RED4ext `GameStates.Add`).
    /// `state_type`: 0=BaseInitialization 1=Initialization 2=Running 3=Shutdown (EGameStateType).
    /// Callbacks recebem null pra CGameApplication (pragmático — passamos null em vez do ponteiro
    /// real que o RED4ext real receberia). **Cobertura completa dos 4 estados (fechado 2026-08-11,
    /// PENDENCIAS-UNIFICADAS.md `#4`/`#32`/`#36`/`#37`/`#38`)**, todos disparando na ordem real
    /// `BaseInit→Init→Running→Shutdown`, mapeados nos eventos já detectados:
    ///   BaseInit.OnEnter/OnExit → disparam 1x, cedo no boot (logo após `boot_phase()`, o 1º ponto
    ///                             idempotente onde plugins já tiveram chance de `add_game_state`)
    ///   Init.OnEnter            → dispara na sequência, mesmo instante (mesmo guard)
    ///   Init.OnUpdate           → dispara a cada ~2s de heartbeat, enquanto ainda em Initialization
    ///   Init.OnExit             → dispara na transição de PRESENÇA do player em `cp77_tick` (mesmo
    ///                             mecanismo confiável do Running.OnEnter, não mais o byte de fase —
    ///                             que uma investigação anterior (cont.192) provou ser flaky nessa
    ///                             leitura), IMEDIATAMENTE ANTES de Running.OnEnter — latch único
    ///                             (Initialization só transiciona pra Running 1x por processo)
    ///   Running.OnEnter  → dispara quando o player spawna (presença detectada no cp77_tick)
    ///   Running.OnUpdate → dispara a cada ~180 ticks de gameplay (enquanto player presente)
    ///   Running.OnExit   → dispara quando o player despawna
    ///   Shutdown.OnEnter → dispara quando `exit()` é hookado (clean-exit)
    /// Divergência que PERMANECE (não é risco, é gap documentado): `aApp` (ponteiro pra
    /// `CGameApplication`) continua sempre `null` em todo callback — corrigir isso precisaria de
    /// `CGameEngine::Get()`/#475, RE nova fora de escopo (o dado em si nunca foi necessário pra
    /// nenhum uso real até agora, só a notificação do estado).
    pub add_game_state: unsafe extern "C" fn(
        state_type: u32,
        on_enter: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
        on_update: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
        on_exit: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    ) -> bool,
    // --- v13 (abi_version 13): PluginHandle real (RED4ext.SDK `#31`, `Hooking::Detach`) ---
    /// Handle ÚNICO deste plugin (atribuído sequencialmente em `plugins::load_one`, 1 cópia de
    /// `BwmsApi` por plugin — antes só existia UMA instância `static` compartilhada por todos).
    /// Guardar este valor e passá-lo pras 2 chamadas abaixo — é o que o RED4ext real exige que
    /// todo plugin faça com o `PluginHandle` recebido em `Main()`.
    pub my_handle: u64,
    /// Igual a `inline_hook`, mas REGISTRA `handle` como DONO do hook em `target` — habilita
    /// `inline_detach` a recusar outro plugin tentando soltá-lo.
    pub inline_hook_owned: unsafe extern "C" fn(handle: u64, target: *mut c_void, repl: *mut c_void) -> *mut c_void,
    /// `Hooking::Detach(handle, target)`: desfaz o hook em `target` SÓ se `handle` for o dono
    /// registrado por `inline_hook_owned` (case contrário: recusa, loga, devolve `false` — o
    /// hook do outro plugin continua intacto). `target` sem dono rastreado (nunca hookado via
    /// `inline_hook_owned`, ou já solto) também recusa.
    pub inline_detach: unsafe extern "C" fn(handle: u64, target: *mut c_void) -> bool,
    /// Igual a `vtable_hook`, mas REGISTRA `handle` como dono do slot `(vtbl, slot_idx)`.
    pub vtable_hook_owned: unsafe extern "C" fn(handle: u64, vtbl: *mut u64, slot_idx: usize, repl: *const c_void) -> *const c_void,
    /// `Hooking::Detach` pro caso vtable: restaura o slot original SÓ se `handle` for o dono
    /// (o `orig` fica guardado no registro interno — o plugin não precisa lembrar dele, ao
    /// contrário do `vtable_unhook` cru).
    pub vtable_detach: unsafe extern "C" fn(handle: u64, vtbl: *mut u64, slot_idx: usize) -> bool,
    // --- v14 (abi_version 14): `CompareSemVerPrerelease` (RED4ext.SDK `#43`) — precedência SemVer
    // 2.0 COMPLETA (major.minor.patch + identificadores de pre-release), não só o triplet. Fecha
    // o bug real onde `semver_satisfies`/`Codeware.Require` tratava `1.2.3-rc1` como IGUAL a
    // `1.2.3` (deveria ser MENOR — regra 11.3 da spec: release > pre-release no mesmo triplet).
    /// `CompareSemVerPrerelease(lhs, rhs) -> int32`: `<0` se `lhs<rhs`, `0` se iguais, `>0` se
    /// `lhs>rhs`, pela precedência SemVer 2.0 completa (não só major.minor.patch). Strings
    /// inválidas/nulas comparam como `0.0.0` sem pre-release (nunca crasha).
    pub compare_semver_prerelease: unsafe extern "C" fn(lhs: *const c_char, rhs: *const c_char) -> i32,
}

impl Clone for BwmsApi {
    fn clone(&self) -> Self {
        *self
    }
}
impl Copy for BwmsApi {}

/// Próximo handle a atribuir (sequencial, começa em 1 — `0` fica reservado como "sem dono"
/// nos registros de ownership abaixo, nunca um handle real).
static NEXT_PLUGIN_HANDLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Chamado 1x por plugin em `plugins::load_one`, antes de montar a cópia pessoal da API.
pub(crate) fn next_plugin_handle() -> u64 {
    NEXT_PLUGIN_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

/// Dono registrado de cada hook INLINE (`target addr -> handle`). Um novo `inline_hook_owned`
/// no MESMO target sobrescreve o dono (mesma semântica LIFO já provada em `gum::Interceptor` —
/// o registro só precisa saber quem é o dono ATUAL/topo pra decidir um `Detach`).
static INLINE_HOOK_OWNERS: std::sync::Mutex<Vec<(usize, u64)>> = std::sync::Mutex::new(Vec::new());
/// Dono registrado de cada slot de vtable hookado (`(vtbl addr, slot_idx) -> (handle, orig)`).
static VTABLE_HOOK_OWNERS: std::sync::Mutex<Vec<(usize, usize, u64, usize)>> = std::sync::Mutex::new(Vec::new());

fn register_inline_owner(target: usize, handle: u64) {
    if let Ok(mut v) = INLINE_HOOK_OWNERS.lock() {
        v.retain(|(t, _)| *t != target);
        v.push((target, handle));
    }
}

fn take_inline_owner(target: usize) -> Option<u64> {
    if let Ok(mut v) = INLINE_HOOK_OWNERS.lock() {
        if let Some(pos) = v.iter().position(|(t, _)| *t == target) {
            return Some(v.remove(pos).1);
        }
    }
    None
}

fn register_vtable_owner(vtbl: usize, slot: usize, handle: u64, orig: usize) {
    if let Ok(mut v) = VTABLE_HOOK_OWNERS.lock() {
        v.retain(|(vp, s, _, _)| !(*vp == vtbl && *s == slot));
        v.push((vtbl, slot, handle, orig));
    }
}

fn take_vtable_owner(vtbl: usize, slot: usize) -> Option<(u64, usize)> {
    if let Ok(mut v) = VTABLE_HOOK_OWNERS.lock() {
        if let Some(pos) = v.iter().position(|(vp, s, _, _)| *vp == vtbl && *s == slot) {
            let (_, _, h, o) = v.remove(pos);
            return Some((h, o));
        }
    }
    None
}

/// Registry de callbacks de estado de jogo (add_game_state / red4ext-gamestates-add).
struct GameStateEntry {
    state_type: u32,
    on_enter: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    on_update: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    on_exit: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
}

static GAME_STATES: std::sync::Mutex<Vec<GameStateEntry>> = std::sync::Mutex::new(Vec::new());

// ===== Janela de registro restrita ao load do plugin (CET item #46, `PENDENCIAS-UNIFICADAS.md`)
// — CET remove `registerForEvent`/`registerHotkey`/`registerInput` do ambiente Lua logo após
// `init.lua` terminar: um mod só pode se registrar durante sua PRÓPRIA inicialização, nunca
// depois. Mesma robustez pro BWMS: `plugins::load_one` abre a janela só durante a chamada
// SÍNCRONA de `bwms_plugin_main`, fecha assim que ela retorna — as 5 funções de REGISTRO
// (register_native/register_native_argful/register_method/add_game_state/
// register_draw_callback) recusam fora dessa janela. NÃO se aplica a hooks/reflection/TweakDB
// (chamáveis a qualquer momento por design — só "registrar uma capacidade nomeada nova" é
// restrito, o mesmo escopo exato do CET real).
static REGISTRATION_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn registration_window_open() -> bool {
    REGISTRATION_OPEN.load(std::sync::atomic::Ordering::Acquire)
}

/// RAII: `plugins::load_one` chama `open_registration_window()` antes de `entry()` e deixa o
/// guard cair no escopo — fecha a janela na saída (retorno normal OU panic desenrolado por
/// `catch_unwind`, nunca fica presa aberta). Mesmo espírito do `ExecDepthGuard` já usado no
/// projeto (contador thread_local + `Drop`, `selfboot.rs`) — RAII em vez de par
/// abre/fecha manual, que um `return`/panic no meio esqueceria de fechar.
pub(crate) struct RegistrationWindowGuard;

impl Drop for RegistrationWindowGuard {
    fn drop(&mut self) {
        REGISTRATION_OPEN.store(false, std::sync::atomic::Ordering::Release);
    }
}

pub(crate) fn open_registration_window() -> RegistrationWindowGuard {
    REGISTRATION_OPEN.store(true, std::sync::atomic::Ordering::Release);
    RegistrationWindowGuard
}

/// Chama on_enter de todos os estados registrados com `state_type`.
pub fn call_game_state_enter(state_type: u32) {
    if let Ok(states) = GAME_STATES.lock() {
        for s in states.iter().filter(|s| s.state_type == state_type) {
            if let Some(f) = s.on_enter {
                let _ = unsafe { f(std::ptr::null_mut()) };
            }
        }
    }
}

/// Chama on_update de todos os estados registrados com `state_type`.
pub fn call_game_state_update(state_type: u32) {
    if let Ok(states) = GAME_STATES.lock() {
        for s in states.iter().filter(|s| s.state_type == state_type) {
            if let Some(f) = s.on_update {
                let _ = unsafe { f(std::ptr::null_mut()) };
            }
        }
    }
}

/// Chama on_exit de todos os estados registrados com `state_type`.
pub fn call_game_state_exit(state_type: u32) {
    if let Ok(states) = GAME_STATES.lock() {
        for s in states.iter().filter(|s| s.state_type == state_type) {
            if let Some(f) = s.on_exit {
                let _ = unsafe { f(std::ptr::null_mut()) };
            }
        }
    }
}

unsafe extern "C" fn api_add_game_state(
    state_type: u32,
    on_enter: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    on_update: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    on_exit: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
) -> bool {
    if !registration_window_open() {
        crate::log("[api] add_game_state: recusado — chamado fora da janela de registro (só durante bwms_plugin_main)");
        return false;
    }
    if let Ok(mut states) = GAME_STATES.lock() {
        states.push(GameStateEntry { state_type, on_enter, on_update, on_exit });
        crate::log(&format!("[api] add_game_state: type={state_type} on_enter={} on_update={} on_exit={}",
            on_enter.is_some(), on_update.is_some(), on_exit.is_some()));
        return true;
    }
    false
}

unsafe extern "C" fn api_log(msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    crate::log(&format!("[plugin] {}", CStr::from_ptr(msg).to_string_lossy()));
}

unsafe extern "C" fn api_vtable_hook(vtbl: *mut u64, slot_idx: usize, repl: *const c_void) -> *const c_void {
    crate::gum::vtable_hook(vtbl, slot_idx, repl).unwrap_or(std::ptr::null())
}

unsafe extern "C" fn api_vtable_unhook(vtbl: *mut u64, slot_idx: usize, orig: *const c_void) {
    crate::gum::vtable_unhook(vtbl, slot_idx, orig);
}

unsafe extern "C" fn api_field_ptr(obj: *mut c_void, field: *const c_char) -> *mut c_void {
    if obj.is_null() || field.is_null() {
        return std::ptr::null_mut();
    }
    let name = match CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let cls = crate::rtti::class_of(obj);
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    let p = crate::rtti::find_property_in_class(cls, name);
    if p.is_null() {
        return std::ptr::null_mut();
    }
    let vo = crate::rtti::prop_value_offset(p) as usize;
    (obj as *mut u8).add(vo) as *mut c_void
}

unsafe extern "C" fn api_call_method(obj: *mut c_void, method: *const c_char, ret16: *mut u8) -> bool {
    if obj.is_null() || method.is_null() {
        return false;
    }
    let name = match CStr::from_ptr(method).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let cls = crate::rtti::class_of(obj);
    if cls.is_null() {
        return false;
    }
    match crate::rtti::resolve_in_class(cls, name) {
        Some(rf) => match crate::rtti::call_func(&rf, obj, &[]) {
            Some(r) => {
                if !ret16.is_null() {
                    std::ptr::copy_nonoverlapping(r.as_ptr(), ret16, 16);
                }
                true
            }
            None => false,
        },
        None => false,
    }
}

unsafe extern "C" fn api_inline_hook(target: *mut c_void, repl: *mut c_void) -> *mut c_void {
    if target.is_null() || repl.is_null() {
        return std::ptr::null_mut();
    }
    crate::gum::Interceptor::obtain().replace(target, repl).unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn api_inline_revert(target: *mut c_void) {
    if target.is_null() {
        return;
    }
    crate::gum::Interceptor::obtain().revert(target);
}

unsafe extern "C" fn api_inline_hook_owned(handle: u64, target: *mut c_void, repl: *mut c_void) -> *mut c_void {
    if target.is_null() || repl.is_null() {
        return std::ptr::null_mut();
    }
    let trampoline = crate::gum::Interceptor::obtain().replace(target, repl).unwrap_or(std::ptr::null_mut());
    if !trampoline.is_null() {
        register_inline_owner(target as usize, handle);
        crate::log(&format!("[api] inline_hook_owned: handle={handle} target={target:p} -> ok (dono registrado)"));
    }
    trampoline
}

unsafe extern "C" fn api_inline_detach(handle: u64, target: *mut c_void) -> bool {
    if target.is_null() {
        return false;
    }
    match take_inline_owner(target as usize) {
        Some(owner) if owner == handle => {
            crate::gum::Interceptor::obtain().revert(target);
            crate::log(&format!("[api] inline_detach: handle={handle} target={target:p} -> OK"));
            true
        }
        Some(owner) => {
            // Devolve a posse (a checagem falhou, o hook do OUTRO dono continua intacto).
            register_inline_owner(target as usize, owner);
            crate::log(&format!(
                "[api] inline_detach: RECUSADO — handle={handle} tentou soltar hook de target={target:p} pertencente a handle={owner}"
            ));
            false
        }
        None => {
            crate::log(&format!("[api] inline_detach: target={target:p} sem dono rastreado (handle={handle}) — recusado"));
            false
        }
    }
}

unsafe extern "C" fn api_vtable_hook_owned(handle: u64, vtbl: *mut u64, slot_idx: usize, repl: *const c_void) -> *const c_void {
    match crate::gum::vtable_hook(vtbl, slot_idx, repl) {
        Some(orig) => {
            register_vtable_owner(vtbl as usize, slot_idx, handle, orig as usize);
            crate::log(&format!(
                "[api] vtable_hook_owned: handle={handle} vtbl={vtbl:p} slot={slot_idx} -> ok (dono registrado)"
            ));
            orig
        }
        None => std::ptr::null(),
    }
}

unsafe extern "C" fn api_vtable_detach(handle: u64, vtbl: *mut u64, slot_idx: usize) -> bool {
    match take_vtable_owner(vtbl as usize, slot_idx) {
        Some((owner, orig)) if owner == handle => {
            crate::gum::vtable_unhook(vtbl, slot_idx, orig as *const c_void);
            crate::log(&format!("[api] vtable_detach: handle={handle} vtbl={vtbl:p} slot={slot_idx} -> OK"));
            true
        }
        Some((owner, orig)) => {
            register_vtable_owner(vtbl as usize, slot_idx, owner, orig);
            crate::log(&format!(
                "[api] vtable_detach: RECUSADO — handle={handle} tentou soltar slot {slot_idx} de vtbl={vtbl:p} pertencente a handle={owner}"
            ));
            false
        }
        None => {
            crate::log(&format!(
                "[api] vtable_detach: vtbl={vtbl:p} slot={slot_idx} sem dono rastreado (handle={handle}) — recusado"
            ));
            false
        }
    }
}

unsafe extern "C" fn api_register_native(
    full: *const c_char,
    short: *const c_char,
    handler: crate::register::NativeHandler,
) -> bool {
    if !registration_window_open() {
        crate::log("[api] register_native: recusado — chamado fora da janela de registro (só durante bwms_plugin_main)");
        return false;
    }
    if full.is_null() || short.is_null() {
        return false;
    }
    let fulls = match CStr::from_ptr(full).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let shorts = match CStr::from_ptr(short).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    // mesmo caminho do register_all: Registry::obtain + proto global clonável (Cos/Sin/...).
    let reg = match crate::rtti::Registry::obtain() {
        Some(r) => r,
        None => return false,
    };
    let mut proto = std::ptr::null_mut();
    for n in ["Cos", "Sin", "AbsF", "SqrtF", "LogF"] {
        let p = crate::register::get_function(&reg, n);
        if crate::rtti::sane(p) {
            proto = p;
            break;
        }
    }
    if !crate::rtti::sane(proto) {
        return false;
    }
    crate::register::register_global(&reg, proto, fulls, shorts, handler)
}

unsafe extern "C" fn api_register_method(
    class: *const c_char,
    full: *const c_char,
    short: *const c_char,
    handler: crate::register::NativeHandler,
) -> bool {
    if !registration_window_open() {
        crate::log("[api] register_method: recusado — chamado fora da janela de registro (só durante bwms_plugin_main)");
        return false;
    }
    if class.is_null() || full.is_null() || short.is_null() {
        return false;
    }
    let classs = match CStr::from_ptr(class).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let fulls = match CStr::from_ptr(full).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let shorts = match CStr::from_ptr(short).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let reg = match crate::rtti::Registry::obtain() {
        Some(r) => r,
        None => return false,
    };
    // proto = método estático nativo conhecido (mesmo donor do register_codeware_facade/
    // regmethod_selftest, já provado in-game 2026-07-12).
    let proto = match crate::rtti::resolve_any(&reg, &["gameGodModeSystem"], "AddGodMode") {
        Some(rf) => rf.func,
        None => return false,
    };
    crate::register::register_method(&reg, classs, proto, fulls, shorts, handler, true)
}

unsafe extern "C" fn api_register_native_argful(
    full: *const c_char,
    short: *const c_char,
    handler: crate::register::NativeHandler,
    param_types: *const *const c_char,
    n_params: usize,
) -> bool {
    if !registration_window_open() {
        crate::log("[api] register_native_argful: recusado — chamado fora da janela de registro (só durante bwms_plugin_main)");
        return false;
    }
    if full.is_null() || short.is_null() {
        return false;
    }
    let fulls = match CStr::from_ptr(full).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let shorts = match CStr::from_ptr(short).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    // lê os NOMES DOS TIPOS do array de C-strings (null em qualquer um = aborta seguro).
    let mut types: Vec<&str> = Vec::with_capacity(n_params);
    if n_params > 0 {
        if param_types.is_null() {
            return false;
        }
        for i in 0..n_params {
            let p = *param_types.add(i);
            if p.is_null() {
                return false;
            }
            match CStr::from_ptr(p).to_str() {
                Ok(s) => types.push(s),
                Err(_) => return false,
            }
        }
    }
    let reg = match crate::rtti::Registry::obtain() {
        Some(r) => r,
        None => return false,
    };
    // proto = uma global argful já existente (Cos/Sin tomam Float) — mesma descoberta do register_native.
    let mut proto = std::ptr::null_mut();
    for n in ["Cos", "Sin", "AbsF", "SqrtF", "LogF"] {
        let p = crate::register::get_function(&reg, n);
        if crate::rtti::sane(p) {
            proto = p;
            break;
        }
    }
    if !crate::rtti::sane(proto) {
        return false;
    }
    crate::register::register_argful_by_types(&reg, proto, &types, fulls, shorts, handler)
}

unsafe extern "C" fn api_fire_event(name: *const c_char) -> usize {
    if name.is_null() {
        return 0;
    }
    match CStr::from_ptr(name).to_str() {
        Ok(s) => crate::register::fire_event(s),
        Err(_) => 0,
    }
}

/// Resolve o PONTEIRO da propriedade `field` (por nome, via RTTI) no objeto `obj`. Base dos get/set
/// tipados (v5). null-safe (obj/field nulos → null).
unsafe fn resolve_prop(obj: *mut c_void, field: *const c_char) -> *mut c_void {
    if obj.is_null() || field.is_null() {
        return std::ptr::null_mut();
    }
    let name = match CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };
    let cls = crate::rtti::class_of(obj);
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    crate::rtti::find_property_in_class(cls, name)
}

unsafe extern "C" fn api_prop_get_f32(obj: *mut c_void, field: *const c_char) -> f32 {
    let p = resolve_prop(obj, field);
    if p.is_null() {
        return 0.0;
    }
    crate::rtti::prop_get_f32(p, obj)
}
unsafe extern "C" fn api_prop_set_f32(obj: *mut c_void, field: *const c_char, val: f32) -> bool {
    let p = resolve_prop(obj, field);
    if p.is_null() {
        return false;
    }
    crate::rtti::prop_set_f32(p, obj, val);
    true
}
unsafe extern "C" fn api_prop_get_i32(obj: *mut c_void, field: *const c_char) -> i32 {
    let p = resolve_prop(obj, field);
    if p.is_null() {
        return 0;
    }
    crate::rtti::prop_get_u32(p, obj) as i32
}

unsafe extern "C" fn api_call_method_args(
    obj: *mut c_void,
    method: *const c_char,
    args: *const *const c_char,
    n_args: usize,
    ret16: *mut u8,
) -> bool {
    if obj.is_null() || method.is_null() {
        return false;
    }
    let name = match CStr::from_ptr(method).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    // parseia os N args (formato i:/f:/b:/n:/s:/e:, o mesmo do callf). null em qualquer um = aborta seguro.
    let mut parsed: Vec<crate::rtti::Arg> = Vec::with_capacity(n_args);
    if n_args > 0 {
        if args.is_null() {
            return false;
        }
        for i in 0..n_args {
            let p = *args.add(i);
            if p.is_null() {
                return false;
            }
            match CStr::from_ptr(p).to_str() {
                Ok(s) => parsed.push(crate::parse_cmd_arg(s)),
                Err(_) => return false,
            }
        }
    }
    let cls = crate::rtti::class_of(obj);
    if cls.is_null() {
        return false;
    }
    match crate::rtti::resolve_in_class(cls, name) {
        Some(rf) => match crate::rtti::call_func(&rf, obj, &parsed) {
            Some(r) => {
                if !ret16.is_null() {
                    std::ptr::copy_nonoverlapping(r.as_ptr(), ret16, 16);
                }
                true
            }
            None => false,
        },
        None => false,
    }
}

unsafe extern "C" fn api_tweakdb_get_flat(name: *const c_char, out_bits: *mut u32) -> bool {
    if name.is_null() || out_bits.is_null() {
        return false;
    }
    let names = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    match crate::tweakdb_rt::api_get_flat_scalar(names) {
        Some(bits) => {
            *out_bits = bits;
            true
        }
        None => false,
    }
}

unsafe extern "C" fn api_tweakdb_set_flat(name: *const c_char, bits: u32) -> bool {
    if name.is_null() {
        return false;
    }
    let names = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };
    crate::tweakdb_rt::api_set_flat_scalar(names, bits)
}

unsafe extern "C" fn api_tweakdb_clone_record(
    class_name: *const c_char,
    source: *const c_char,
    new_name: *const c_char,
) -> bool {
    if class_name.is_null() || source.is_null() || new_name.is_null() {
        return false;
    }
    let (c, s, n) = match (
        CStr::from_ptr(class_name).to_str(),
        CStr::from_ptr(source).to_str(),
        CStr::from_ptr(new_name).to_str(),
    ) {
        (Ok(c), Ok(s), Ok(n)) => (c, s, n),
        _ => return false,
    };
    crate::tweakdb_rt::clone_record_api(c, s, n);
    true
}

unsafe extern "C" fn api_log_level(level: u8, msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let tag = match level {
        0 => "TRACE",
        1 => "DEBUG",
        2 => "INFO",
        3 => "WARN",
        4 => "ERROR",
        5 => "CRIT",
        _ => "INFO",
    };
    crate::log(&format!("[plugin][{tag}] {}", CStr::from_ptr(msg).to_string_lossy()));
}

/// Parseia "major.minor.patch" (componentes ausentes/não-numéricos viram 0). Descarta o sufixo de
/// pre-release/build do SemVer (tudo a partir do 1º `-` ou `+`) ANTES de separar por `.` — senão
/// `"1.2.3-rc1"` dava `(1,2,0)` (o `"3-rc1"` não parseia como número e o patch caía pra 0). Aceita
/// também um prefixo `v`/`V` (`"v1.2.3"`). Cobre o caso real de `Codeware.Require` com versão de mod.
pub(crate) fn parse_semver_triplet(s: &str) -> (u32, u32, u32) {
    let core = s.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next().unwrap_or(""); // fora pre-release/build
    let mut parts = core.splitn(3, '.');
    let major = parts.next().and_then(|p| p.trim().parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|p| p.trim().parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|p| p.trim().parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

/// Identificador de pre-release SemVer 2.0 (ex.: em `1.2.3-rc.1`, os identificadores são `rc` e
/// `1`). Numérico vs alfanumérico importa pra precedência (regra 11.4.3 da spec): identificador
/// puramente numérico compara por VALOR; senão compara como string ASCII; numérico sempre tem
/// precedência MENOR que alfanumérico quando comparados entre si.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PreIdent {
    Num(u64),
    Alpha(String),
}

impl PreIdent {
    fn parse(s: &str) -> Self {
        match s.parse::<u64>() {
            Ok(n) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => PreIdent::Num(n),
            _ => PreIdent::Alpha(s.to_string()),
        }
    }
}

impl Ord for PreIdent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (PreIdent::Num(a), PreIdent::Num(b)) => a.cmp(b),
            (PreIdent::Alpha(a), PreIdent::Alpha(b)) => a.cmp(b),
            (PreIdent::Num(_), PreIdent::Alpha(_)) => Ordering::Less,
            (PreIdent::Alpha(_), PreIdent::Num(_)) => Ordering::Greater,
        }
    }
}
impl PartialOrd for PreIdent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Parseia só a lista de identificadores de pre-release (a parte entre `-` e `+`, dot-separated).
/// `""` (release, sem pre-release) devolve vetor vazio — usado pra decidir a regra 11.3 da spec
/// (uma versão SEM pre-release tem precedência MAIOR que a mesma versão COM pre-release).
fn parse_semver_prerelease(s: &str) -> Vec<PreIdent> {
    let core = s.trim().trim_start_matches(['v', 'V']);
    let after_dash = match core.split_once('-') {
        Some((_, rest)) => rest,
        None => return Vec::new(),
    };
    // o pre-release termina no primeiro '+' (metadado de build, sem peso de precedência).
    let pre = after_dash.split('+').next().unwrap_or("");
    if pre.is_empty() {
        return Vec::new();
    }
    pre.split('.').map(PreIdent::parse).collect()
}

/// Compara duas strings de versão pela precedência COMPLETA do SemVer 2.0 (spec item 11):
/// major.minor.patch primeiro; se empatado, versão sem pre-release > versão com pre-release;
/// se ambas têm pre-release, compara identificador por identificador (numérico por valor,
/// alfanumérico por ASCII, numérico sempre < alfanumérico), e se todos os identificadores em
/// comum empatarem, quem tem MAIS identificadores tem precedência maior.
pub(crate) fn semver_precedence(a: &str, b: &str) -> std::cmp::Ordering {
    let triplet_cmp = parse_semver_triplet(a).cmp(&parse_semver_triplet(b));
    if triplet_cmp != std::cmp::Ordering::Equal {
        return triplet_cmp;
    }
    let (pre_a, pre_b) = (parse_semver_prerelease(a), parse_semver_prerelease(b));
    match (pre_a.is_empty(), pre_b.is_empty()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater, // release > pre-release
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => pre_a.cmp(&pre_b), // Vec<PreIdent>::cmp já compara elemento-a-elemento + tamanho
    }
}

unsafe extern "C" fn api_semver_satisfies(required: *const c_char, actual: *const c_char) -> bool {
    if required.is_null() || actual.is_null() {
        return false;
    }
    let (req, act) = match (CStr::from_ptr(required).to_str(), CStr::from_ptr(actual).to_str()) {
        (Ok(r), Ok(a)) => (r, a),
        _ => return false,
    };
    semver_precedence(act, req) != std::cmp::Ordering::Less
}

/// `RED4ext.SDK` `#43` (`CompareSemVerPrerelease`) — precedência SemVer 2.0 COMPLETA como
/// `int32` (convenção `strcmp`: `<0`/`0`/`>0`). String nula/inválida em qualquer lado vira
/// `"0.0.0"` sem pre-release (nunca crasha, nunca panica).
unsafe extern "C" fn api_compare_semver_prerelease(lhs: *const c_char, rhs: *const c_char) -> i32 {
    let l = if lhs.is_null() { "0.0.0" } else { CStr::from_ptr(lhs).to_str().unwrap_or("0.0.0") };
    let r = if rhs.is_null() { "0.0.0" } else { CStr::from_ptr(rhs).to_str().unwrap_or("0.0.0") };
    match semver_precedence(l, r) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Callbacks de draw registrados por plugins (`cet-imgui-thirdparty`). Chamados 1x por frame,
/// DENTRO do onDraw (mesmo ponto/gate que os mods Lua — `overlay::in_draw()`), por
/// `overlay.rs::render_imgui`. `Mutex<Vec<>>` — plugins carregam 1x no boot (thread única),
/// mas o registro pode, em teoria, vir de qualquer thread; a CHAMADA em si é sempre na render.
static PLUGIN_DRAW_CALLBACKS: std::sync::Mutex<Vec<extern "C" fn()>> = std::sync::Mutex::new(Vec::new());

unsafe extern "C" fn api_register_draw_callback(cb: extern "C" fn()) -> bool {
    if !registration_window_open() {
        crate::log("[api] register_draw_callback: recusado — chamado fora da janela de registro (só durante bwms_plugin_main)");
        return false;
    }
    if let Ok(mut v) = PLUGIN_DRAW_CALLBACKS.lock() {
        v.push(cb);
    }
    true
}

/// Chamado pelo `overlay.rs::render_imgui`, DENTRO do onDraw, 1x por frame — dispara todos os
/// draw callbacks registrados por plugins. Isolado (`catch_unwind`) — um plugin que panica no
/// draw não derruba o frame nem os outros plugins.
pub(crate) unsafe fn run_plugin_draw_callbacks() {
    let cbs: Vec<extern "C" fn()> = match PLUGIN_DRAW_CALLBACKS.lock() {
        Ok(v) => v.clone(),
        Err(_) => return,
    };
    for cb in cbs {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb()));
    }
}

unsafe extern "C" fn api_imgui_begin(title: *const c_char) -> bool {
    if !crate::overlay::in_draw() || title.is_null() {
        return true; // fora do onDraw: no-op "visível" (mesmo padrão do binding Lua)
    }
    let s = match CStr::from_ptr(title).to_str() {
        Ok(s) => s,
        Err(_) => return true,
    };
    let c = match std::ffi::CString::new(s) {
        Ok(c) => c,
        Err(_) => return true,
    };
    imgui::sys::igBegin(c.as_ptr(), std::ptr::null_mut(), 0)
}

unsafe extern "C" fn api_imgui_text(text: *const c_char) {
    if !crate::overlay::in_draw() || text.is_null() {
        return;
    }
    if let Ok(s) = CStr::from_ptr(text).to_str() {
        if let Ok(c) = std::ffi::CString::new(s) {
            imgui::sys::igTextUnformatted(c.as_ptr(), std::ptr::null());
        }
    }
}

unsafe extern "C" fn api_imgui_end() {
    if crate::overlay::in_draw() {
        imgui::sys::igEnd();
    }
}

/// Caminho do manifesto persistente (fora do save, mesmo padrão de `~/.bwms-modconfig.txt`).
fn scripts_add_manifest_path() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(|h| std::path::Path::new(&h).join(".bwms-scripts-add.txt"))
}

/// Lê+escreve o manifesto com dedup (1 path por linha). `pub(crate)` pra `bwms-fastboot.sh`/testes
/// não precisarem — o SHELL lê o arquivo direto; isto só existe pro handler da API escrever.
unsafe extern "C" fn api_scripts_add(path: *const c_char) -> bool {
    if path.is_null() {
        return false;
    }
    let p = match CStr::from_ptr(path).to_str() {
        Ok(s) => s.trim().to_string(),
        Err(_) => return false,
    };
    if p.is_empty() {
        return false;
    }
    let manifest = match scripts_add_manifest_path() {
        Some(m) => m,
        None => return false,
    };
    let existing = std::fs::read_to_string(&manifest).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == p) {
        crate::log(&format!("[api] scripts_add: '{p}' já estava no manifesto (sem duplicar)"));
        return true;
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&p);
    content.push('\n');
    match std::fs::write(&manifest, content) {
        Ok(_) => {
            crate::log(&format!(
                "[api] scripts_add: '{p}' registrado em {manifest:?} (efetivo no PRÓXIMO compile+boot, redscript não hot-recarrega)"
            ));
            true
        }
        Err(e) => {
            crate::log(&format!("[api] scripts_add: falha ao escrever manifesto '{manifest:?}': {e}"));
            false
        }
    }
}

/// Instância única passada a TODOS os plugins (vive o processo inteiro — ponteiro sempre válido).
pub static BWMS_API: BwmsApi = BwmsApi {
    abi_version: crate::plugins::BWMS_PLUGIN_API,
    log: api_log,
    vtable_hook: api_vtable_hook,
    vtable_unhook: api_vtable_unhook,
    field_ptr: api_field_ptr,
    call_method: api_call_method,
    inline_hook: api_inline_hook,
    inline_revert: api_inline_revert,
    register_native: api_register_native,
    register_native_argful: api_register_native_argful,
    fire_event: api_fire_event,
    prop_get_f32: api_prop_get_f32,
    prop_set_f32: api_prop_set_f32,
    prop_get_i32: api_prop_get_i32,
    call_method_args: api_call_method_args,
    register_method: api_register_method,
    tweakdb_get_flat: api_tweakdb_get_flat,
    tweakdb_set_flat: api_tweakdb_set_flat,
    tweakdb_clone_record: api_tweakdb_clone_record,
    log_level: api_log_level,
    semver_satisfies: api_semver_satisfies,
    register_draw_callback: api_register_draw_callback,
    imgui_begin: api_imgui_begin,
    imgui_text: api_imgui_text,
    imgui_end: api_imgui_end,
    scripts_add: api_scripts_add,
    add_game_state: api_add_game_state,
    my_handle: 0, // prototype; `plugins::load_one` clona isto com `my_handle` real por plugin.
    inline_hook_owned: api_inline_hook_owned,
    inline_detach: api_inline_detach,
    vtable_hook_owned: api_vtable_hook_owned,
    vtable_detach: api_vtable_detach,
    compare_semver_prerelease: api_compare_semver_prerelease,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn api_version_matches() {
        assert_eq!(BWMS_API.abi_version, crate::plugins::BWMS_PLUGIN_API);
    }

    // Os ponteiros de função são ITENS => não-nulos por construção. Aqui provo os caminhos
    // null-safe (o plugin pode passar lixo sem derrubar o jogo). Tudo bate no guard antes de
    // tocar o RTTI, então roda sem o runtime/jogo (autônomo).
    #[test]
    fn null_safe_paths() {
        unsafe {
            assert!((BWMS_API.vtable_hook)(std::ptr::null_mut(), 0, std::ptr::null()).is_null());
            let f = CString::new("Health").unwrap();
            assert!((BWMS_API.field_ptr)(std::ptr::null_mut(), f.as_ptr()).is_null());
            assert!(!(BWMS_API.call_method)(std::ptr::null_mut(), f.as_ptr(), std::ptr::null_mut()));
            let mut dummy = 0u64;
            let obj = &mut dummy as *mut u64 as *mut c_void;
            assert!((BWMS_API.field_ptr)(obj, std::ptr::null()).is_null());
            assert!(!(BWMS_API.call_method)(obj, std::ptr::null(), std::ptr::null_mut()));
            assert!((BWMS_API.inline_hook)(std::ptr::null_mut(), std::ptr::null_mut()).is_null());
        }
    }

    // handler no-op só p/ satisfazer a assinatura nos testes (nunca é chamado: os guards abortam antes).
    unsafe extern "C" fn test_handler(_c: *mut c_void, _f: *mut c_void, _r: *mut c_void, _rt: i64) {}

    // v4: as duas fns novas são null-safe e não tocam o RTTI antes dos guards (autônomo, sem jogo).
    #[test]
    fn v4_null_safe() {
        unsafe {
            let dummy: crate::register::NativeHandler = test_handler;
            let s = CString::new("X").unwrap();
            // full/short null → false; param_types null com n>0 → false.
            assert!(!(BWMS_API.register_native_argful)(std::ptr::null(), s.as_ptr(), dummy, std::ptr::null(), 0));
            assert!(!(BWMS_API.register_native_argful)(s.as_ptr(), std::ptr::null(), dummy, std::ptr::null(), 0));
            assert!(!(BWMS_API.register_native_argful)(s.as_ptr(), s.as_ptr(), dummy, std::ptr::null(), 2));
            // fire_event(null) → 0; nome válido sem jogo/callbacks → 0 (sem crash).
            assert_eq!((BWMS_API.fire_event)(std::ptr::null()), 0);
            assert_eq!((BWMS_API.fire_event)(s.as_ptr()), 0);
            // v5: get/set tipado null-safe (obj/field nulos → default, sem tocar RTTI).
            assert_eq!((BWMS_API.prop_get_f32)(std::ptr::null_mut(), s.as_ptr()), 0.0);
            assert!(!(BWMS_API.prop_set_f32)(std::ptr::null_mut(), s.as_ptr(), 1.0));
            assert_eq!((BWMS_API.prop_get_i32)(std::ptr::null_mut(), s.as_ptr()), 0);
            let mut d = 0u64;
            let obj = &mut d as *mut u64 as *mut c_void;
            assert_eq!((BWMS_API.prop_get_f32)(obj, std::ptr::null()), 0.0);
            // v6: call_method_args null-safe (obj/method nulos e args null com n>0 → false, sem tocar RTTI).
            assert!(!(BWMS_API.call_method_args)(std::ptr::null_mut(), s.as_ptr(), std::ptr::null(), 0, std::ptr::null_mut()));
            assert!(!(BWMS_API.call_method_args)(obj, std::ptr::null(), std::ptr::null(), 0, std::ptr::null_mut()));
            assert!(!(BWMS_API.call_method_args)(obj, s.as_ptr(), std::ptr::null(), 2, std::ptr::null_mut()));
            // v8: TweakDB null-safe (name nulo, out_bits nulo, ou singleton indisponível fora do
            // jogo real → false/sem crash, nunca toca o TweakDB sem checar antes).
            let mut bits: u32 = 0;
            assert!(!(BWMS_API.tweakdb_get_flat)(std::ptr::null(), &mut bits));
            assert!(!(BWMS_API.tweakdb_get_flat)(s.as_ptr(), std::ptr::null_mut()));
            assert!(!(BWMS_API.tweakdb_get_flat)(s.as_ptr(), &mut bits)); // sem jogo: singleton() None
            assert!(!(BWMS_API.tweakdb_set_flat)(std::ptr::null(), 0));
            assert!(!(BWMS_API.tweakdb_set_flat)(s.as_ptr(), 0)); // sem jogo: singleton() None
            assert!(!(BWMS_API.tweakdb_clone_record)(std::ptr::null(), s.as_ptr(), s.as_ptr()));
            assert!(!(BWMS_API.tweakdb_clone_record)(s.as_ptr(), std::ptr::null(), s.as_ptr()));
            assert!(!(BWMS_API.tweakdb_clone_record)(s.as_ptr(), s.as_ptr(), std::ptr::null()));
            // v9: semver_satisfies null-safe + lógica de comparação (autônomo, sem RTTI/jogo).
            assert!(!(BWMS_API.semver_satisfies)(std::ptr::null(), s.as_ptr()));
            assert!(!(BWMS_API.semver_satisfies)(s.as_ptr(), std::ptr::null()));
        }
    }

    #[test]
    fn semver_satisfies_compara_versoes() {
        unsafe {
            let req = CString::new("1.2.0").unwrap();
            let higher = CString::new("1.3.0").unwrap();
            let equal = CString::new("1.2.0").unwrap();
            let lower = CString::new("1.1.9").unwrap();
            let major_higher = CString::new("2.0.0").unwrap();
            assert!((BWMS_API.semver_satisfies)(req.as_ptr(), higher.as_ptr()));
            assert!((BWMS_API.semver_satisfies)(req.as_ptr(), equal.as_ptr()));
            assert!(!(BWMS_API.semver_satisfies)(req.as_ptr(), lower.as_ptr()));
            assert!((BWMS_API.semver_satisfies)(req.as_ptr(), major_higher.as_ptr()));
            // componente ausente/malformado vira 0 — não crasha, só compara.
            let partial_req = CString::new("1.2").unwrap();
            let partial_act = CString::new("1").unwrap();
            assert!(!(BWMS_API.semver_satisfies)(partial_req.as_ptr(), partial_act.as_ptr()));
        }
    }

    #[test]
    fn parse_semver_triplet_pre_release_e_v_prefixo() {
        // O bug corrigido: pre-release não pode zerar o patch.
        assert_eq!(parse_semver_triplet("1.2.3-rc1"), (1, 2, 3));
        assert_eq!(parse_semver_triplet("1.2.3+build.5"), (1, 2, 3));
        assert_eq!(parse_semver_triplet("1.2.3-rc.1+build"), (1, 2, 3));
        assert_eq!(parse_semver_triplet("v1.2.3"), (1, 2, 3));
        assert_eq!(parse_semver_triplet("V2.0.0-beta"), (2, 0, 0));
        assert_eq!(parse_semver_triplet(" 1.2.3 "), (1, 2, 3));
        // sem o fix, "1.0.0" >= "1.0.0-rc1" seria falso-negativo (rc1 virava 1.0.0 tb, ok aqui;
        // o ponto é o patch não sumir): 1.2.3-rc1 satisfaz require 1.2.3.
        assert!(parse_semver_triplet("1.2.3-rc1") >= parse_semver_triplet("1.2.3"));
    }

    #[test]
    fn semver_precedence_prerelease_sequencia_oficial_semver_org() {
        // Sequência de precedência CRESCENTE do exemplo oficial (semver.org, item 11):
        // 1.0.0-alpha < 1.0.0-alpha.1 < 1.0.0-alpha.beta < 1.0.0-beta < 1.0.0-beta.2
        // < 1.0.0-beta.11 < 1.0.0-rc.1 < 1.0.0
        let seq = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for w in seq.windows(2) {
            assert_eq!(
                semver_precedence(w[0], w[1]),
                std::cmp::Ordering::Less,
                "{} deveria ser < {}",
                w[0],
                w[1]
            );
            assert_eq!(semver_precedence(w[1], w[0]), std::cmp::Ordering::Greater);
        }
        // release > pre-release no mesmo triplet — o bug original (#43): antes disto,
        // "1.2.3-rc1" e "1.2.3" comparavam IGUAIS (o sufixo era descartado antes de comparar).
        assert_eq!(semver_precedence("1.2.3-rc1", "1.2.3"), std::cmp::Ordering::Less);
        assert_eq!(semver_precedence("1.2.3", "1.2.3-rc1"), std::cmp::Ordering::Greater);
        // igual é igual (com e sem pre-release).
        assert_eq!(semver_precedence("1.2.3", "1.2.3"), std::cmp::Ordering::Equal);
        assert_eq!(semver_precedence("1.2.3-rc.1", "1.2.3-rc.1"), std::cmp::Ordering::Equal);
        // build metadata (`+...`) nunca pesa na precedência.
        assert_eq!(semver_precedence("1.2.3+build1", "1.2.3+build2"), std::cmp::Ordering::Equal);
        assert_eq!(semver_precedence("1.2.3-rc.1+build1", "1.2.3-rc.1+build9"), std::cmp::Ordering::Equal);
    }

    #[test]
    fn compare_semver_prerelease_c_abi() {
        unsafe {
            let a = CString::new("1.2.3-rc1").unwrap();
            let b = CString::new("1.2.3").unwrap();
            let eq = CString::new("1.2.3").unwrap();
            assert!((BWMS_API.compare_semver_prerelease)(a.as_ptr(), b.as_ptr()) < 0);
            assert!((BWMS_API.compare_semver_prerelease)(b.as_ptr(), a.as_ptr()) > 0);
            assert_eq!((BWMS_API.compare_semver_prerelease)(b.as_ptr(), eq.as_ptr()), 0);
            // nulo em qualquer lado = trata como "0.0.0", nunca crasha.
            assert!((BWMS_API.compare_semver_prerelease)(std::ptr::null(), b.as_ptr()) < 0);
            assert!((BWMS_API.compare_semver_prerelease)(b.as_ptr(), std::ptr::null()) > 0);
            assert_eq!((BWMS_API.compare_semver_prerelease)(std::ptr::null(), std::ptr::null()), 0);
        }
    }

    // log_level não tem valor de retorno pra checar, mas prova que não crasha (inclui msg null).
    #[test]
    fn log_level_nao_crasha() {
        unsafe {
            (BWMS_API.log_level)(2, std::ptr::null());
            let msg = CString::new("teste de log com nivel").unwrap();
            for lvl in 0..=6u8 {
                (BWMS_API.log_level)(lvl, msg.as_ptr());
            }
        }
    }
}
