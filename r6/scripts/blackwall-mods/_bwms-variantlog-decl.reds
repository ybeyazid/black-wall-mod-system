// Declaração mínima (temporária, teste 2026-08-03) — só o `native func`, sem o
// @wrapMethod(PlayerPuppet) OnGameAttached auto-disparado (movido pra /tmp/bwms-temp-excluded/
// pra teste isolado). bwms-settings-poc.reds precisa desta declaração pra compilar.
native func BwmsVariantLog(a: Variant, b: Variant) -> Void;
