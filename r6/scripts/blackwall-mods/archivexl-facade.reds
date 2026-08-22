// BWMS — ArchiveXL Facade (redscript puro, 2026-08-05, achado de auditoria).
//
// A `App::Facade` real do ArchiveXL (`enablers/ArchiveXL/src/App/Facade.hpp`) tem `Version`/
// `Require`/`Reload`/`RegisterDir`/`RegisterArchive`/`GetBodyType`/`EnableGarmentOffsets`/
// `DisableGarmentOffsets` — nenhum portado apesar do catálogo marcar ArchiveXL "28/28 IMPL"
// (esses 28 gaps cobrem os mecanismos de resource/appearance, não esta fachada de controle).
//
// `Version`/`Require`/`RegisterArchive`/`RegisterDir`/`Reload` aqui — os 5 com backing funcional
// real implementado (`register_facade_methods` compartilhado c/ Codeware/TweakXL, ver
// `register.rs`; `RegisterArchive`/`RegisterDir` generalizam `axl-pathb-injection-arbitrary`, já
// PROVADO ao vivo em 10 boots — `lib.rs::facade_register_archive`/`load_archive_into_group`;
// `Reload` reusa `mod_pipeline::reload_archivexl_tables`, extraído de `boot_phase()`, mesmo padrão
// já fechado de `TweakXL.Reload`).
//
// `EnableGarmentOffsets`/`DisableGarmentOffsets` (item #97, 2026-08-11): fonte real é um
// `static inline bool` puro (`Garment/Extension.cpp:883-890`), zero endereço nativo — adicionados
// aqui, backing real registrado (`GARMENT_OFFSETS_ENABLED`, `register.rs`). Divergência
// documentada: o flag ainda não tem consumidor real (`ApplyOffsetOverrides`/item #67 depende de
// `ComputePlayerGarment`, `RawFunc` não resolvido nesta sessão) — plumbing correto, efeito em
// jogo pendente de RE futura.
//
// `GetBodyType` (item #97, 2026-08-11, FECHA O ITEM POR COMPLETO 8/8): fonte real
// (`App/Facade.cpp:18-21`) é 1 linha repassando pra `PuppetStateExtension::GetBodyType(aPuppet)`
// — A MESMA função já lida por completo pra fechar o item #32 (`PuppetStateSystem`). Precondição
// já confirmada (`Extension.cpp:142-190`): `s_bodyTags` só é populado pelo `.xl player.bodyTypes`,
// nunca wired neste projeto — SEMPRE vazio, logo `GetBodyType` é DETERMINÍSTICO e sempre devolve
// `s_baseBodyType` = `"BaseBody"` (`BaseBodyName`, `Extension.hpp:18`), independente do puppet.
// Zero endereço nativo novo — backing real registrado (`tramp_archivexl_get_body_type`,
// `register.rs`), mesma disciplina de todo native argful deste projeto (drena o param do frame).
//
// `RegisterLink`/`GetAliasCount`/`GetAliasHash` (item #27, 2026-08-11): via redscript-callable
// de `resource.link` (registrar 1 par em runtime), reusando a MESMA tabela (`RESLINK_MAP`) que
// `resource.link`/`InitializeArchives` já usam — mecanismo PROVADO ao vivo desde 2026-07-13.
// `GetAliases` real devolveria `Set<ResourcePath>` (array) — decisão de escopo: 2 natives
// escalares (`GetAliasCount`+`GetAliasHash` por índice) em vez de 1 native com `array<T>` de
// saída, evitando de propósito a categoria de risco que já CROU este projeto (`TweakXL #43`/
// `GetRecordsArray`, precisa de trailer de allocator-vft pra não corromper o teardown do array
// local). `ResourcePath` é hash puro — devolve `Uint64`, não `String` decodificada (mesma
// divergência já aceita em `Casts.reds`/`BwmsHashToResRef`).
//
// `IsDynamicValue`/`ProcessDynamicString` (itens #21/#23, 2026-08-12): núcleo de `App::
// DynamicAppearanceController` (templating de string por atributo, `Garment/Dynamic.cpp`) exposto
// ao redscript PELA 1ª VEZ — o algoritmo já tinha sido portado/testado offline em `bwms-core`
// (mod-install-time, CLI), mas nunca tinha chegado ao dylib runtime carregado pelo jogo (crates
// separados, sem dependência entre si). `ProcessDynamicString` aceita só 1 atributo LOCAL (nome+
// valor) por decisão de escopo — mesmo padrão que `MeshExtension::ExpandResourcePath`/item `#48`
// já usa internamente (1 atributo `material`), evita marshalling de mapa/array arbitrário. `#20`/
// `#23` (o sistema `DynamicAppearanceController` inteiro) seguem REAL_GAP — faltam
// `GetSuffixData`/`GetCustomizationData`/`UpdateState`/`ResolveName`/`ResolvePath`, todos
// precisando de endereço nativo genuíno ou `Entity*` real.
//
// `GetVisualTagOverrideCount`/`ApplyVisualTagOverride` (itens #21/#22, round 2, 2026-08-12):
// `App::OverrideTagManager` (a tabela de 14 tags built-in do `GetTagManager`, item #21, já
// fechada offline desde 2026-08-11) composta com a escrita de campo já provável do `#22`
// (`ComponentWrapper::SetChunkMask`, campo cru, offsets confirmados contra o header RTTI
// oficial) — a peça que faltava pra tornar a tabela genuinamente APLICÁVEL num componente
// vivo, não só uma função pura testável offline. Enumera os componentes da entidade, compara
// `IComponent::name` (CName, EXATO — `Tags.hpp` declara `OverrideTagDefinition =
// Core::Map<Red::CName, ChunkMask>`) contra as chaves da tag, aplica onde bater.
// `GetVisualTagOverrideCount` é dry-run (zero mutação) — sempre testar antes de
// `ApplyVisualTagOverride`. Divergência de escopo CONSCIENTE: só a metade "1 mod aplica
// direto" — a combinação MULTI-MOD por hash (`ComponentState`, já fechada offline em
// `bwms-core`) não está portada aqui; 2 mods na mesma tag/componente não fazem merge, o
// último a chamar vence. ⚠️ Nunca testado ao vivo — itens `#21`/`#22` continuam REAL_GAP.
//
// `IsUniqueAppearanceName` (item #20, round 3, 2026-08-12): porte 1:1 de
// `GarmentExtension::IsUniqueAppearanceName` (`Garment/Extension.cpp:909`) — ZERO RE/ZERO
// endereço nativo, 3 comparações de CName sentinela (`"default"`/`"random"`/
// `"empty_appearance_default"`). Item #20 continua REAL_GAP no todo (o resto depende de
// `EntityState`/`IComponent::UpdateRenderer`), mas é a peça isolada e determinística que o
// próprio catálogo já tinha identificado como "implementável já".
//
// `IsDynamicAppearanceRef`/`AppearanceRefName`/`AppearanceRefWeight`/
// `AppearanceRefMatchesVariant` (item #23, round 3, 2026-08-12): porte pro dylib runtime de
// `App::DynamicAppearanceRef` (`Garment/Dynamic.cpp:194-266`), já fechado e testado OFFLINE em
// `bwms-core/src/xl.rs` desde 2026-08-11 mas nunca creditado ao dylib que o jogo carrega —
// mesma fronteira de crate já corrigida hoje mais cedo pro núcleo irmão `ProcessString`/
// `IsDynamicValue`. Parseia `nome!v1!v2&c1&c2` (variantes+condições) — a sintaxe que
// `EntityState::ToggleConditionalComponents`/`SelectDynamicAppearance` (item #22) usam pra
// escolher componente/aparência condicional ativo. Decisão de escopo CONSCIENTE (evita o
// risco de `array<T>`-retorno já responsável pelo crash de `TweakXL #43`): expõe getters
// escalares (`Name`/`Weight`/`MatchesVariant`) em vez de devolver `Set<Uint64>` de
// variantes/condições. Item #23 continua REAL_GAP (faltam `GetSuffixData`/
// `GetCustomizationData`/`UpdateState`/`ParseAppearance`, todos precisando de endereço
// nativo genuíno ou `Entity*` real).
//
// `ExpandResourcePath` (item #48, round 3, 2026-08-12): porte pro dylib runtime de
// `MeshExtension::ExpandResourcePath` (`Mesh/Extension.cpp:900-925`), já fechado e testado
// OFFLINE em `bwms-core/src/xl.rs::expand_resource_path` desde 2026-08-11 (mesma fronteira de
// crate) — reusa o MESMO núcleo de `ProcessDynamicString` com o atributo local `material`
// pré-injetado (fiel à fonte real). Devolve string vazia se o processamento falhar/o atributo
// faltar (convenção "vazio=falhou" já usada em `ProcessDynamicString`). Item #48 continua
// REAL_GAP (os 4 hooks reais de `MeshExtension`/`ProcessDynamicMaterials`/
// `FinalizeDynamicMaterial`/`CloneMaterialInstance`/`ExpandMaterialInheritance` seguem de
// fora, precisam de `CMaterialInstance`/`JobQueue` reais).
//
// `ApplyVisualTagOverrideForMod`/`RemoveVisualTagOverrideForMod` (item #22, round 6,
// 2026-08-12): porte do algoritmo `App::ComponentState` (`Garment/States.cpp`, já fechado e
// testado OFFLINE em `bwms-core::xl.rs` desde 2026-08-11 — AND-cumulativo por-hash pra
// hiding/OR-cumulativo por-hash pra showing) pro dylib runtime, composto com a escrita de
// campo já provável de `ApplyVisualTagOverride` (round 2, acima). Fecha exatamente a
// divergência de escopo que `ApplyVisualTagOverride` já documentava: 2 mods (`modHash`
// diferentes) aplicando a MESMA tag no MESMO componente agora COMBINAM em vez de o último a
// chamar vencer sozinho — `RemoveVisualTagOverrideForMod` desfaz só a contribuição de 1 mod
// (uninstall-safety, a regra-mãe do projeto), recompondo o resultado dos mods restantes ou
// voltando pro baseline cacheado se era o último. `ApplyVisualTagOverride`/
// `GetVisualTagOverrideCount` (single-shot, sem tracking) continuam existindo e inalterados
// pra uso simples. Item #22 continua REAL_GAP (nada abaixo testado ao vivo — a leitura/escrita
// de memória do componente segue dependendo dos offsets de campo do round anterior, também sem
// prova ao vivo).
//
// `ApplyVisualTagOverrideForModWithRefresh`/`RemoveVisualTagOverrideForModWithRefresh` (item
// #22, round 6, 2026-08-12, mesma sessão): variantes das 2 acima que TAMBÉM disparam
// `RefreshAppearance` (`entSkinnedMeshComponent` vtable `+0x288`, `0x100c7eda4` — a MESMA
// função já PROVADA AO VIVO 2x, zero crash, via o comando `togglerebuild`/2026-08-07) em cada
// componente `SkinnedFamily` tocado — zero endereço nativo NOVO, só uma composição nova de uma
// função já em produção. Fecha o gap entre "campo `chunkMask` escrito" e "efeito VISUAL de
// verdade", que a nota de `EnableGarmentOffsets`/`#97` (acima) já documentava como pendente
// ("plumbing correto, efeito visual pendente"). Restrito a `SkinnedFamily` de propósito — o
// offset `0x100c7eda4` nunca foi confirmado independentemente pra `MeshComponent`/
// `MorphTarget` (variantes SEM refresh continuam existindo em separado, pra não misturar 2
// categorias de risco — escrita de campo puro vs. dispatch de função nativa — na mesma prova
// ao vivo pendente).
// `IsDynamicAppearanceName`/`AppearanceNameBase`/`AppearanceNameVariant`/`AppearanceNameContext`/
// `AppearanceMatchesReference` (item #23, 2026-08-15, IMPLEMENTAÇÃO GENUINAMENTE NOVA — zero
// código Rust/redscript prévio pra este pedaço específico, mesmo dia que fechou o "próximo
// passo" que a nota anterior do item deixava explícito: "ParseAppearance/DynamicAppearanceName
// (struct irmã, não lida ainda)"). Porte de `App::DynamicAppearanceName`
// (`Garment/Dynamic.hpp:12-26`+`Dynamic.cpp:96-192`) — o VALOR real de uma aparência/componente
// (`nome!variante[+parte[=valor]]...[%contexto][&condicao...]`), papel espelhado/complementar do
// `DynamicAppearanceRef` (o CRITÉRIO, já fechado 2026-08-12) — juntos formam o par completo que
// `DynamicAppearanceController::MatchReference` usa pra decidir qual variante condicional de um
// componente está ativa. 100% composição pura sobre `bwms_hashes::fnv1a64`/`fnv1a64_seeded` (já
// provados em produção via `DynamicAppearanceRef`), zero endereço nativo, zero campo cru.
//
// Achado de RE não-óbvio durante o porte (documentado no código Rust e nos testes): a tag de
// contexto (`%numero`) só parseia com sucesso se for o FINAL LITERAL da string — a ordem válida
// é "nome!variante&condicao%numero" (condição ANTES do contexto), não o inverso; testado e
// confirmado batendo com o algoritmo C++ real (`ParseInt` exige que TUDO até o fim da string
// seja numérico). Achado adicional: a forma `+chave=valor` grava em `overrides` um hash com um
// índice de substring "quirky" (`Dynamic.cpp:168` real, offset a partir do INÍCIO do trecho
// restante em vez de a partir da posição do `=`) — replicado FIELMENTE (não corrigido), mesma
// disciplina já aplicada a outros bugs upstream achados no projeto (Journal/Animation do
// Codeware).
//
// `AppearanceMatchesReference` reusa `DynamicAppearanceRef::parse`+`DynamicAppearanceName::parse`
// e compõe a lógica real de `MatchReference` — divergência de escopo consciente: esta via NUNCA
// acopla `EntityState`/#22 (que vive num registry global separado), então sempre reproduz o ramo
// "entidade sem estado registrado" da fonte real quando a referência tem condições (`false`
// sempre, mesmo com a variante batendo) — decisivo e 100% autocontido, não depende de nenhum
// estado de entidade/save real, só de 2 strings que o próprio chamador fornece.
public abstract native class ArchiveXL {
  public static native func Require(version: String) -> Bool;
  public static native func Version() -> String;
  public static native func RegisterArchive(path: String) -> Bool;
  public static native func RegisterDir(path: String) -> Bool;
  public static native func RegisterLink(path: String, link: String) -> Bool;
  public static native func GetAliasCount(path: String) -> Int32;
  public static native func GetAliasHash(path: String, index: Int32) -> Uint64;
  public static native func Reload() -> Void;
  public static native func EnableGarmentOffsets() -> Void;
  public static native func DisableGarmentOffsets() -> Void;
  public static native func GetBodyType(puppet: wref<GameObject>) -> CName;
  public static native func IsDynamicValue(value: String) -> Bool;
  public static native func ProcessDynamicString(input: String, attrName: String, attrValue: String) -> String;
  // #23 DynamicAppearanceController.ResolveName/ResolvePath — pass-through fiel nesta build
  // (m_states nunca populado, GetSuffixData/GetCustomizationData/UpdateState bloqueados por RE
  // de endereço nativo; ver doc completo em register.rs). Assinatura simplificada (sem
  // DynamicPartList — nunca consultada no único ramo alcançável).
  public static native func ResolveName(entity: wref<Entity>, name: String) -> String;
  public static native func ResolvePath(entity: wref<Entity>, pathStr: String) -> String;
  public static native func GetVisualTagOverrideCount(entity: wref<Entity>, tag: String) -> Int32;
  public static native func ApplyVisualTagOverride(entity: wref<Entity>, tag: String) -> Int32;
  public static native func ApplyVisualTagOverrideForMod(entity: wref<Entity>, tag: String, modHash: Uint64) -> Int32;
  public static native func RemoveVisualTagOverrideForMod(entity: wref<Entity>, tag: String, modHash: Uint64) -> Int32;
  public static native func ApplyVisualTagOverrideForModWithRefresh(entity: wref<Entity>, tag: String, modHash: Uint64) -> Int32;
  public static native func RemoveVisualTagOverrideForModWithRefresh(entity: wref<Entity>, tag: String, modHash: Uint64) -> Int32;
  public static native func IsUniqueAppearanceName(name: CName) -> Bool;
  public static native func IsDynamicAppearanceRef(reference: String) -> Bool;
  public static native func AppearanceRefName(reference: String) -> Uint64;
  public static native func AppearanceRefWeight(reference: String) -> Int32;
  public static native func AppearanceRefMatchesVariant(reference: String, variant: String) -> Bool;
  public static native func ExpandResourcePath(pathStr: String, materialName: String) -> String;
  public static native func IsDynamicAppearanceName(name: String) -> Bool;
  public static native func AppearanceNameBase(name: String) -> Uint64;
  public static native func AppearanceNameVariant(name: String) -> Uint64;
  public static native func AppearanceNameContext(name: String) -> Uint64;
  public static native func AppearanceMatchesReference(appearanceName: String, reference: String) -> Bool;
  public static native func GetBaseAppearanceName(name: String) -> String;

  // Rodada 15 (2026-08-17), item #22: `EntityState::ApplyAppearanceOverride`/
  // `ToggleConditionalComponents` (`States.cpp:659-697`/`535-592`) — os 2 métodos "de aplicação
  // em componente vivo" que a rodada 14 deixou mapeados mas não implementados. `ParseReference`/
  // `MatchReference` (já provados acima, `IsDynamicAppearanceRef`/`AppearanceMatchesReference`) +
  // `FindComponentState`/`FindResourceState` (bookkeeping NOVO desta rodada, registry Rust
  // key-by-entity-pointer, ver `cp77-console/src/register.rs` seção "ArchiveXL #22 rodada 15")
  // são compostos com o `SetAppearanceName`/`RefreshAppearance` já provados (item #168, rodada
  // 14). `LinkComponentToPart`/`LinkPartToAppearanceName`/`AddAppearanceOverrideForComponent` são
  // o setup (equivalentes aos métodos públicos homônimos de `EntityState` que constroem o
  // bookkeeping consumido pelos 2 métodos de aplicação) — sem eles, `ApplyAppearanceOverride`
  // devolve `false` (early-return honesto, nunca crash) e `ToggleConditionalComponents` pula
  // todo componente condicional (fica só com o pass-through dos não-condicionais).
  //
  // `SelectDynamicAppearance` (o 3º método que a rodada 14 apontava) FICA DE FORA — precisa de
  // `Raw::EntityTemplate::FindAppearance` (já tentado isolado em 2026-07-29 e CONFIRMADO CRASHAR)
  // ou de layout de `AppearanceResource::appearances`/`AppearanceDefinition` nunca RE'd — RE de
  // endereço nativo genuína, não composição pura. Não implementado.
  //
  // ⚠️ Divergência de forma: `ToggleConditionalComponents` recebe a ENTIDADE (não um
  // `array<ref<IComponent>>` por parâmetro — técnica de marshalling nunca exercitada nesta
  // codebase) e lê `Entity+0xA0` internamente, o MESMO array que `Entity.GetComponents()`
  // (item Codeware #24, já fechado) devolve — semanticamente equivalente, sem inventar
  // marshalling novo. `resourcePath: Uint64` é um HASH (não `ResRef`) — mesma convenção já usada
  // por `AppearanceRefName`/`GetAliasHash` acima nesta mesma Facade.
  public static native func LinkComponentToPart(entity: wref<Entity>, component: ref<IComponent>, resourcePath: Uint64) -> Bool;
  public static native func LinkPartToAppearanceName(entity: wref<Entity>, resourcePath: Uint64, appearance: String) -> Bool;
  public static native func AddAppearanceOverrideForComponent(entity: wref<Entity>, component: ref<IComponent>, modHash: Uint64, appearance: CName) -> Bool;
  public static native func ApplyAppearanceOverride(entity: wref<Entity>, component: ref<IComponent>) -> Bool;
  public static native func ToggleConditionalComponents(entity: wref<Entity>) -> Int32;

  // Item #67 (2026-08-16): `EntityState::AddOffsetOverride`/`RemoveOffsetOverrides`/
  // `GetOffsetOverride` (`States.cpp:172-190,367-381,748-754`) — a metade "registry" de
  // `ApplyOffsetOverrides` (o gap residual real de `ComputePlayerGarment`, já hookado/confirmado
  // disparando desde 2026-07-28/2026-08-05-06 — ver `cp77-console/src/selftest.rs`,
  // `GARMENT_COMPUTEPLAYERGARMENT_VM`). ACHADO: `ApplyOffsetOverrides` não precisa de RE de
  // endereço nativo nenhuma — é composição pura sobre um `Core::Map` que o PRÓPRIO ArchiveXL
  // possui (nunca lê memória do motor), mesma categoria já fechada várias vezes acima nesta
  // Facade. `resourcePath: Uint64` é HASH (mesma convenção de `AppearanceRefName`/`GetAliasHash`
  // acima), `modHash: Uint64` identifica quem registrou (pra poder desfazer seletivamente ao
  // desequipar). ZERO mutação de memória nativa — só o registry Rust próprio do BWMS. A escrita
  // de volta em `ComputePlayerGarment.aOffsets` (o `DynArray<int32_t>` que o motor realmente lê)
  // fica pra uma rodada futura com boot ao vivo — ver probe observe-only em `selftest.rs`.
  public static native func AddOffsetOverride(entity: wref<Entity>, modHash: Uint64, resourcePath: Uint64, offset: Int32) -> Bool;
  public static native func RemoveOffsetOverrides(entity: wref<Entity>, modHash: Uint64) -> Uint32;
  public static native func GetOffsetOverride(entity: wref<Entity>, resourcePath: Uint64) -> Int32;
}
