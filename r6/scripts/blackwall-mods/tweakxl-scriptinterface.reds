// tweakxl-scriptinterface.reds — TweakXL `#43` (`ScriptInterface`, `PENDENCIAS-UNIFICADAS.md`),
// declarações PERMANENTES pras 3 natives já fechadas mas nunca declaradas fora de smoke tests
// removidos (achado 2026-08-10: BwmsGetFlatFloat/BwmsGetRecordCount estavam REGISTRADOS no Rust
// desde cont.163/164 mas sem NENHUM .reds ativo declarando-os — nenhum mod conseguia chamá-los).
native func BwmsGetFlatFloat(path: TweakDBID) -> Float
native func BwmsGetRecordCount(className: String) -> Int32
native func BwmsGetRecordByIndex(className: String, index: Int32) -> ref<IScriptable>

// 2026-08-10 (cont.188/189): `GetRecords<R>() -> array<ref<R>>` — a peça final do item #43 — FECHADA
// POR COMPLETO. 1ª native deste projeto a devolver um `array<T>` inteiro (toda anterior devolve
// escalar/handle único). `tramp_tweakxl_get_records` (register.rs): `get_records_of_class` (já
// provado) + buffer com N slots de 16B (`{instance,refCount=0}`, mesmo layout de
// `dynarray_push_handle16`) + 1 slot de 8B de TRAILER — vft de allocator EMPRESTADO de um
// DynArray<Handle<T>> real/vivo (`Entity+0xA0` do player, ComponentsStorage.components,
// já confirmado gerenciado pelo motor) — mesmo mecanismo que `mkarr`/`set_flat_array_u64` já
// usam pra arrays TweakDB-flat, adaptado pro stride de 16B (handle) em vez de 8B (escalar).
// **Achado real no caminho (cont.188 -> 189)**: a 1ª tentativa (SEM trailer) crashava
// REPRODUZIVELMENTE (2/2, SIGSEGV no teardown de escopo do array LOCAL) — isolado via teste
// controlado (mesmo dylib sem chamar a native = zero crash). Com o trailer emprestado: **3/3
// boots limpos**, `ArraySize`/`IsDefined`/`GetClassName` todos exatos em TODOS. Ver
// `proofs/2026-08-10-tweakxl-43-getrecordsarray-PROVADO.log` (+ o log CRASH-NAO-FECHADO
// anterior, preservado como registro honesto da 1ª tentativa).
native func BwmsGetRecordsArray(className: String, limit: Int32) -> array<ref<IScriptable>>
