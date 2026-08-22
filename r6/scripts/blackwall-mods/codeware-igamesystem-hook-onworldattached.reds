// codeware-igamesystem-hook-onworldattached.reds — Codeware `#117` (`App/Callback/CallbackSystem.hpp`),
// continuação 2 (2026-08-18), depois de `codeware-igamesystem-hook-ongamerestored.reds` ter sido
// testado ao vivo (2026-08-16) e confirmado NUNCA disparar, mesmo com breakpoint real via lldb,
// 90s+ de gameplay real pós-autocontinue.
//
// Segue a recomendação exata deixada por essa rodada: testar um slot que dispare MAIS CEDO/mais
// perto do momento em que o mundo é montado — `OnWorldAttached` (mac=0x118, evento real
// 'Session/BeforeStart', Windows offset 0x110 confirmado no header hand-written do SDK), 1 dos 5
// slots com endereço ÚNICO/não-ICF-colapsado (fn=0x106398e80 no boot da rodada de 2026-08-16,
// prólogo real `SUB SP,SP,#0xB0`).
//
// ABI real (`RED4ext.SDK/.../gameIGameSystem.hpp:26`): `virtual void
// OnWorldAttached(world::RuntimeScene* aScene);` — `this` + 1 ponteiro extra, void. 100%
// OBSERVE-ONLY: o hook Rust sempre repassa pro original com os MESMOS args, nunca suprime/muda
// nada — mesmo idioma já provado 7800+ vezes pelo `AnimationSystem_FrameBeginReset` (RED4ext
// `#461`). A vtable patcheada é a vtable de CLASSE de `MappinSystem` (singleton, sem blast radius
// pra outras classes).
native func BwmsMappinSystemHookOnWorldAttached(sys: ref<MappinSystem>) -> Bool;
