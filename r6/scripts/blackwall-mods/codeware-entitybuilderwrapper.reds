// codeware-entitybuilderwrapper.reds — Codeware `#25` (`EntityBuilderWrapper.hpp`/
// `EntityBuilderWrapper.reds`, catálogo exaustivo Codeware). ITEM NUNCA TOCADO desde o catálogo
// original de 2026-08-07 (0/21 métodos portados) — trabalhado 2026-08-18 (madrugada, continuação
// 20, 100% offline, zero boot, TRABALHO SOLO sem sub-agentes).
//
// Fonte real (`enablers/Codeware/src/App/Entity/EntityBuilderWrapper.hpp`) confirma que TODOS os
// 21 métodos das 3 classes (`EntityBuilderWrapper`×12, `EntityBuilderTemplateWrapper`×5,
// `EntityBuilderAppearanceWrapper`×4) são leitura pura de campo sobre `Red::EntityBuilder`
// (`enablers/Codeware/src/Red/EntityBuilder.hpp`) — offsets confirmados pelo próprio autor via
// `RED4EXT_ASSERT_OFFSET` (`appearance@0x20`/`appearances@0x80`/`entityTemplate@0x90`/
// `request@0xA0`/`components@0xC0`/`visualTags@0xF8`/`flags@0x110`). ZERO `Core::RawFunc` nos 21
// métodos em si (só `ExtractComponentsJob`/`ScheduleExtractComponentsJob`, agendamento
// assíncrono, fora do escopo — nenhum getter/setter exposto ao redscript precisa deles).
//
// As 3 classes `extends Red::IScriptable` DIRETO (nunca `ISerializable`) — mesmo ancestral já
// usado com segurança dezenas de vezes neste projeto (`CallbackSystemHandler`/`StaticEntitySpec`/
// `TweakDBBatch`) — NÃO é CMesh-trap.
//
// **Escopo desta rodada, deliberadamente estreito**: só os 6 métodos de `EntityBuilderWrapper`
// que dependem SÓ de `EntityBuilder`+`EntityBuilderRequest` (campos escalares, marshalling de
// retorno já provado neste projeto): `GetRecordID`/`GetEntityID`/`GetAppearanceName`/`HasEntity`/
// `HasAppearance`/`HasCustomAppearances`. Detalhe completo do que fica de fora (e por quê) na nota
// grande em `register_entitybuilderwrapper`/`cp77-console/src/register.rs`.
//
// **Achado honesto que limita o valor prático**: diferente de `#33`/`#11` (caminho JÁ CONHECIDO
// pra obter uma instância viva — `FindComponentByType`/fixture), `EntityBuilderWrapper` NÃO TEM
// nenhum caminho conhecido pra obter um `EntityBuilder*` REAL — o único disparo é
// `EntityBuilderEvent` (Callback item `#8`), construído de dentro de `Raw::EntityBuilder::
// ExtractComponentsJob`/`ScheduleExtractComponentsJob` (`Core::RawFunc`, endereço Mac NUNCA
// resolvido, zero candidato levantado nesta rodada — não investigado, fora de escopo). A fixture
// (`BwmsMakeTestEntityBuilderWrapper`, smoke test) constrói um objeto 100% SINTÉTICO — prova o
// MECANISMO (forge+métodos+round-trip), não a captura de um builder real.
public native class EntityBuilderWrapper extends IScriptable {
    public native func GetRecordID() -> TweakDBID
    public native func GetEntityID() -> EntityID
    public native func GetAppearanceName() -> CName
    public native func HasEntity() -> Bool
    public native func HasAppearance() -> Bool
    public native func HasCustomAppearances() -> Bool
}
public static native func BwmsMakeTestEntityBuilderWrapper() -> ref<EntityBuilderWrapper>
