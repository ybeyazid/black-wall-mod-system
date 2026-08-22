// codeware-igamesystem-hook-ongamerestored.reds — Codeware `#117` (`App/Callback/CallbackSystem.hpp`),
// continuação de `codeware-igamesystem-vtdump.reds` (offsets já confirmados ao vivo em 2026-08-16,
// `proofs/2026-08-16-codeware-117-igamesystem-slots-CONFIRMADO.log`).
//
// Tenta a via "b" que a rodada anterior deixou não-testada: `vtable_hook` de verdade num dos 5
// slots com endereço ÚNICO (nunca um dos 6 ICF-compartilhados, ver recomendação exata no catálogo,
// item #117). Escolhido `OnGameRestored` (mac=0x158, fn=0x10639d0a8, evento real 'Session/Ready')
// em vez de `OnGameLoad` (mac=0x150) por ter ABI muito mais simples e segura: `bool
// OnGameRestored()` (fonte real, `enablers/Codeware/src/App/Callback/CallbackSystem.hpp:82` —
// só `this`, retorna bool) contra `void OnGameLoad(const Red::JobGroup&, bool& aSuccess, void*
// aStream)` (3 args, UM deles out-param que pode FALHAR o load do jogo se escrito errado).
//
// 100% OBSERVE-ONLY: o hook Rust (`bwms_igamesystem_ongamerestored_hook`) só loga + repassa pro
// original (nunca suprime, nunca muda o retorno) — mesmo idioma já provado 7800+ vezes pelo hook
// `AnimationSystem_FrameBeginReset` (RED4ext #461 Opção A, `selftest.rs`). A vtable patcheada é a
// vtable de CLASSE (compartilhada por TODAS as instâncias de `MappinSystem`, C++ padrão) — como
// `MappinSystem` é singleton, isto NÃO cria blast radius pra outras classes: só afeta chamadas
// via este vtable específico.
native func BwmsMappinSystemHookOnGameRestored(sys: ref<MappinSystem>) -> Bool;
