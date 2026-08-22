// codeware-igamesystem-vtdump.reds — Codeware `App/Callback/CallbackSystem.hpp` (catálogo item
// #117, os 9 eventos de sessão nunca modelados via override de vtable de `Red::IGameSystem`),
// RODADA 2026-08-16 (100% offline).
//
// Achado desta rodada: `RED4ext.SDK/include/RED4ext/Scripting/Natives/gameIGameSystem.hpp` (um
// header HAND-WRITTEN, não `Generated/`) documenta os 20 métodos VIRTUAIS de `Red::IGameSystem`
// com o offset Windows exato de cada um, em comentário (`OnWorldAttached // 110` ...
// `OnUninitialize // 1A8`). Cruzado contra `enablers/Codeware/src/App/Callback/
// CallbackSystem.{hpp,cpp}` (fonte real do item `#1`, já FECHADO neste projeto): confirma quais
// 11 desses 20 slots o `CallbackSystem` real override, e pra qual evento (`CName`) cada um
// mapeia. Ver a nota grande em `cp77-console/src/register.rs`
// (`register_mappinsystem_igamesystem_vtdump`) pro detalhe completo — inclusive o que ISTO NÃO
// resolve (a via segura de fazer o motor chamar nosso código nesses slots continua em aberto).
//
// Este arquivo só DECLARA a native (registrada em Rust, gated por `reds_uses(...)`, zero
// registro se este arquivo não for deployado). 100% OBSERVE-ONLY: a implementação Rust só LÊ o
// ponteiro de função de cada slot (nunca chama), então é seguro chamar automaticamente num
// smoke test — ver `codeware-igamesystem-vtdump-smoke.reds`.
native func BwmsMappinSystemDumpIGameSystemSlots(sys: ref<MappinSystem>) -> Void;
