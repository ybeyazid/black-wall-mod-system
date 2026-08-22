// codeware-localizationstring.reds — Codeware `Utils/LocalizationString.reds` (item #66,
// candidato barato 2026-08-11, 6ª rodada de mineração da sessão).
//
// A fonte real (`App/Utils/LocalizationString.hpp`) é trivial: `CreateLocalizationString(value)
// -> {0, value}` / `ExtractLocalizationString(value) -> value.unk08`, sobre o struct nativo
// `Red::LocalizationString{int64 unk00; CString unk08}` (RED4ext.SDK NativeTypes.hpp:329-334,
// 0x28 bytes) — o MESMO tipo que a native VANILLA REAL `LocalizationStringComponent.GetString`
// já usa como retorno (`orphans.script:21668`), então `LocalizationString` é genuinamente
// redscript-visível neste build (não Codeware-injetado, categoria diferente do CMesh-trap).
//
// Zero endereço nativo novo — composição pura (leitura/escrita do CString embutido em +0x08,
// mesma técnica já provada pra `String`/clipboard/config). DIVERGÊNCIA DE NOME (framework de 3os,
// sem obrigação de bater nome — ver regra do catálogo): `BwmsCreateLocalizationString`/
// `BwmsExtractLocalizationString` em vez dos nomes Codeware originais.
//
// Limite documentado (mesma categoria já aceita em `BwmsConfigGet`/`BwmsSetClipboardText`): a
// CRIAÇÃO só suporta strings <=19 bytes (limite SSO do `write_cstring_inline_ret` — string mais
// longa volta um LocalizationString vazio, sem erro). A EXTRAÇÃO não tem esse limite.
native func BwmsCreateLocalizationString(value: String) -> LocalizationString
native func BwmsExtractLocalizationString(value: LocalizationString) -> String

// Sugar opcional, nome Codeware original (só pra `String`->`LocalizationString`; sem `ToString`
// genérico pra evitar ambiguidade de overload com o `ToString` já usado em todo o projeto).
public func ToLocalizationString(value: String) -> LocalizationString = BwmsCreateLocalizationString(value)
