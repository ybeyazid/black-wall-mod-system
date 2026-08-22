// codeware-utils.reds — declarações PERMANENTES pro cluster `cw-utils` (Bits/Hash/Number/
// String/Compatibility) + Reflection getf/setf/callf/CallMethod/RegisterCallback pro redscript.
//
// ACHADO 2026-08-10 (mesma sessão, varredura de bookkeeping): TODAS estas natives estão
// registradas no Rust há semanas (várias desde 2026-07-13, algumas desde 2026-06-25) e têm
// prova ao vivo arquivada (`proofs/2026-08-07-utils-rename-bwms-nomeproprio-PROVADO.log`,
// `proofs/*-tweakxl-43-*`, cont.144 em HISTORICO.md 2026-08-09, etc.) — mas NENHUM arquivo
// canônico persistente jamais declarou o cluster inteiro: cada prova usou um smoke test próprio,
// removido do deploy ativo depois (mesmo padrão já achado e corrigido 3x nesta sessão pra
// BwmsHashToTweakDBID/BwmsGetFlatFloat/BwmsGetRecordCount/BwmsResourceExists). Resultado: nenhum
// mod real consegue chamar NENHUMA dessas ~30 funções hoje, apesar de todas estarem marcadas
// "FECHADO"/"PROVADO" no catálogo. Este arquivo fecha o gap de bookkeeping de uma vez.
//
// Bits/Hash/Number/String usam nome próprio BWMS (não Codeware — framework de 3os, sem
// obrigação de replicar nome, ver dist/MODDER-API.md). Bloco de registro no Rust é gated por
// `reds_uses(...)` — precisa de >=1 identificador deste arquivo pra ativar o cluster inteiro.

// ===== Utils/Bits.reds — 16 funções (4 larguras x 4 operações) =====
native func BwmsBitTest8(value: Uint8, bit: Int32) -> Bool
native func BwmsBitSet8(value: Uint8, bit: Int32, state: Bool) -> Uint8
native func BwmsBitShiftL8(value: Uint8, n: Int32) -> Uint8
native func BwmsBitShiftR8(value: Uint8, n: Int32) -> Uint8
native func BwmsBitTest16(value: Uint16, bit: Int32) -> Bool
native func BwmsBitSet16(value: Uint16, bit: Int32, state: Bool) -> Uint16
native func BwmsBitShiftL16(value: Uint16, n: Int32) -> Uint16
native func BwmsBitShiftR16(value: Uint16, n: Int32) -> Uint16
native func BwmsBitTest32(value: Uint32, bit: Int32) -> Bool
native func BwmsBitSet32(value: Uint32, bit: Int32, state: Bool) -> Uint32
native func BwmsBitShiftL32(value: Uint32, n: Int32) -> Uint32
native func BwmsBitShiftR32(value: Uint32, n: Int32) -> Uint32
native func BwmsBitTest64(value: Uint64, bit: Int32) -> Bool
native func BwmsBitSet64(value: Uint64, bit: Int32, state: Bool) -> Uint64
native func BwmsBitShiftL64(value: Uint64, n: Int32) -> Uint64
native func BwmsBitShiftR64(value: Uint64, n: Int32) -> Uint64

// ===== Utils/Hash.reds — 3 funções =====
native func BwmsFNV1a64(data: String, opt seed: Uint64) -> Uint64
native func BwmsFNV1a32(data: String, opt seed: Uint32) -> Uint32
native func BwmsMurmur3(data: String, opt seed: Uint32) -> Uint32

// ===== Utils/Number.reds — 8 funções (Parse*, base decimal fixa) =====
native func BwmsParseInt8(s: String) -> Int8
native func BwmsParseInt16(s: String) -> Int16
native func BwmsParseInt32(s: String) -> Int32
native func BwmsParseInt64(s: String) -> Int64
native func BwmsParseUint8(s: String) -> Uint8
native func BwmsParseUint16(s: String) -> Uint16
native func BwmsParseUint32(s: String) -> Uint32
native func BwmsParseUint64(s: String) -> Uint64

// ===== Utils/String.reds — 6 funções (UTF-8, por codepoint) =====
native func BwmsUTF8StrLen(s: String) -> Int32
native func BwmsUTF8StrLeft(s: String, length: Int32) -> String
native func BwmsUTF8StrRight(s: String, length: Int32) -> String
native func BwmsUTF8StrMid(s: String, start: Int32, length: Int32) -> String
native func BwmsUTF8StrLower(s: String) -> String
native func BwmsUTF8StrUpper(s: String) -> String

// ===== Utils/Compatibility.reds — GameFileExists =====
native func BwmsGameFileExists(path: String) -> Bool

// ===== Reflection getf/setf/callf pro redscript, no PLAYER vivo (registro incondicional no
// Rust, sempre ativo — só faltava a declaração) =====
native func BwmsGetPlayerField(field: CName) -> Float
native func BwmsSetPlayerField(field: CName, value: Float) -> Bool
native func BwmsCallPlayerMethod(method: CName) -> Bool

// ===== CallbackSystem — vias GLOBAIS alternativas (mesma capacidade já exposta via a classe
// forjada CallbackSystem.RegisterCallback, mas em forma de função global — gated, precisa
// deste arquivo pra ativar) =====
native func BwmsCallMethod(target: ref<IScriptable>, method: CName) -> Bool
native func BwmsRegisterCallback(eventName: CName, target: ref<IScriptable>, methodName: CName) -> Bool
