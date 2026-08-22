module Bwms.Test55

// RED4ext.SDK #55 — caso de teste do MODULE-SCOPING na resolução de global.
//
// Esta função é 100% REDSCRIPT PURO dentro de um `module`. NÃO declarar `native func` aqui:
// `module` + `native func` no mesmo escopo é crash de bind já documentado (o compilador qualifica
// o `fullName` e o registro Rust usa nome bare — ver memória cp77-redscript-module-plus-nativefunc).
//
// O gap: `CRTTISystem::GetFunction` resolve por HASH de CName, então o RTTI guarda
// "Bwms.Test55.BwmsModuleScopedProbe" e pedir o nome BARE nunca acha.
//
// Como testar ao vivo (canal de comando, em gameplay):
//   callg BwmsModuleScopedProbe      -> ANTES do fix: "global não resolveu"
//                                       DEPOIS: resolve e devolve 5521
public static func BwmsModuleScopedProbe() -> Int32 {
  return 5521;
}
