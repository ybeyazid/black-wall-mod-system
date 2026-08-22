// codeware-componentwrapper.reds — Codeware `App::ComponentWrapper` (item #168, catálogo
// exaustivo Codeware, 2026-08-12 round 2 do sweep). Fonte real (`App/Entity/ComponentWrapper.
// cpp`) é um helper C++ interno (NÃO RTTI-exposto pelo Codeware oficial — `ComponentEx.hpp` só
// expõe `ChangeResource`/`ChangeAppearance`/`LoadAppearance`/`RefreshAppearance`/
// `ResetMaterialCache`, nunca `SetEnabled`/`GetChunkMask`/`SetChunkMask` diretamente) que
// despacha por TIPO de componente (`entMeshComponent`/`entSkinnedMeshComponent`/
// `entGarmentSkinnedMeshComponent`/`entMorphTargetSkinnedMeshComponent`) pra ler/escrever
// campos crus. Como não é API oficial do Windows, expor isso é uma capacidade PRÓPRIA do BWMS
// (dentro da filosofia já estabelecida do projeto pra frameworks de 3os — ver CLAUDE.md "REGRA
// DE NOME/API"), não um requisito de compatibilidade de mod real.
//
// `GetChunkMask`/`SetChunkMask` (`Wrapper.cpp:316-350`) reusam 100% a mecânica já provada via
// console (`chunkmaskget`/`chunkmaskover`, ArchiveXL `#22`, `cp77-console/src/register.rs`) —
// zero endereço nativo novo, só campo cru nos offsets confirmados contra o header RTTI oficial
// (`RED4ext.SDK/.../Generated/ent/*.hpp`, gerado da Reflection do jogo real):
//   entMeshComponent                    chunkMask @ 0x198
//   entSkinnedMeshComponent/
//   entGarmentSkinnedMeshComponent      chunkMask @ 0x248
//   entMorphTargetSkinnedMeshComponent  chunkMask @ 0x240
//
// `IsEnabled`/`SetEnabled` (`Wrapper.cpp:138-147`, `m_component->isEnabled` puro) NÃO
// precisam de declaração nenhuma aqui — `IComponent.IsEnabled() -> Bool` e
// `IComponent.Toggle(on: Bool) -> Void` já são NATIVAS VANILLA REAIS (`redscript-src/
// orphans.script:16667-16668`), callable hoje por qualquer mod sem nenhum trabalho do BWMS.
//
// ⚠️ REGRA DE OURO: nada abaixo foi testado ao vivo (mesmo status do `chunkmaskget`/
// `chunkmaskover` de origem, ArchiveXL `#22` — offsets confirmados por header oficial, nunca
// contra memória viva desta build Mac). Item `#168` continua REAL_GAP até um boot confirmar.
// Divergência honesta: NÃO resolve a pergunta "por que enabled=1 sozinho nunca bastou pro
// torso do full-body" (já refutada 7x em 2026-08-06/07) — é só a metade "leitura/escrita de
// campo" ganhando forma de API redscript, sem mudar nenhuma conclusão anterior da saga.
native func BwmsComponentGetChunkMask(component: ref<IComponent>) -> Uint64
native func BwmsComponentSetChunkMask(component: ref<IComponent>, mask: Uint64) -> Bool
native func BwmsComponentRefreshAppearance(component: ref<IComponent>) -> Bool

@addMethod(IComponent)
public func GetChunkMask() -> Uint64 {
    return BwmsComponentGetChunkMask(this);
}

@addMethod(IComponent)
public func SetChunkMask(mask: Uint64) -> Bool {
    return BwmsComponentSetChunkMask(this, mask);
}

// `RefreshAppearance() -> Bool` (nome OFICIAL do Codeware, `IComponent.reds`, item `#28` já
// FECHADO nesta sessão só como "pergunta de mecanismo respondida" — ver nota grande no Rust —
// mas NUNCA tinha ganho declaração `.reds` própria; nenhum `.reds` deste bundle expunha esse
// nome antes desta linha). Reusa o MESMO endereço já provado ao vivo 2x via `togglerebuild`
// (`APPEARANCE_REBUILD_PROXY_VM`, 2026-08-07) — zero RE nova. Só funciona pra componentes
// `SkinnedFamily` (`entSkinnedMeshComponent`/`entGarmentSkinnedMeshComponent`, ex. 'torso') —
// devolve `false` seguro pra `MeshComponent`/`MorphTarget` (endereço deles nunca resolvido).
@addMethod(IComponent)
public func RefreshAppearance() -> Bool {
    return BwmsComponentRefreshAppearance(this);
}

// ---------------------------------------------------------------------------------------------
// Rodada 14 (2026-08-16): os métodos que a `PRÓXIMA AÇÃO` da rodada 13 (2026-08-15) apontava
// como faltantes de `#168`/`#22` — `SetResourcePath`/`SetAppearanceName`/`ResetMaterialCache`.
// `ResetMaterialCache` FICA DE FORA (precisa de 2 `Core::RawFunc` sem endereço Mac resolvido,
// RE de endereço nativo genuína — ver nota grande em `register_componentwrapper_extra_natives`,
// `cp77-console/src/register.rs`). `SetResourcePath`/`GetResourcePath` e `SetAppearanceName` são
// campo cru puro — zero RE nova, offsets cross-validados contra os headers RTTI oficiais (mesma
// confiança do `GetChunkMask`/`SetChunkMask` acima). **`SetResourcePath`/`GetResourcePath`
// PROVADOS AO VIVO no boot desta rodada** (swap decisivo entre 2 resourcePath REAIS de
// componentes diferentes do player, round-trip exato, restaurado). `IsMeshComponent()` (bônus
// zero-custo) fecha o gate real da fonte (`ComponentWrapper::IsMeshComponent()`,
// `Wrapper.hpp:20`).
//
// ⚠️ ACHADO REAL AO VIVO (não bookkeeping, boot desta rodada) — `GetAppearanceName` (o nome real
// de `ComponentWrapper.hpp`) NÃO é declarado aqui: `IComponent.GetAppearanceName() -> CName` já
// é `final native const` VANILLA REAL (`redscript-src/orphans.script:16659`). A 1ª versão desta
// rodada assumiu (bookkeeping, sem testar) que ela lia o MESMO campo que `SetAppearanceName`
// escreve — **ERRADO, refutado ao vivo**: escrever via `SetAppearanceName` e reler via
// `GetAppearanceName()` (vanilla) sempre deu `matchesExpected=false` (a escrita funcionou —
// `wroteOk=true` — só não é observável por essa native). Causa raiz: `IComponent.
// GetAppearanceName()` lê `IComponent::appearanceName` (campo da CLASSE BASE, o mesmo usado por
// `ComponentWrapper::GetUniqueId()`, `Wrapper.cpp:125-136`), enquanto `SetAppearanceName` escreve
// `meshAppearance` (campo do SUBTIPO `entMeshComponent`/`entSkinnedMeshComponent`/etc.) — são 2
// campos GENUINAMENTE DIFERENTES que só compartilham um nome conceitual parecido. Fix: o getter
// do campo que `SetAppearanceName` realmente usa é exposto sob nome PRÓPRIO
// (`GetMeshAppearanceName`, sem colisão com o `final` vanilla — mesma lição de risco da saga
// `#87`/`cw-real-mod-e2e`).
//
// ⚠️ REGRA DE OURO: nada abaixo foi testado ao vivo ANTES desta rodada. `SetResourcePath`/
// `GetResourcePath`/`GetMeshAppearanceName`/`SetAppearanceName`/`IsMeshComponent` PROVADOS AO
// VIVO nesta rodada (2 boots). `ApplyChunkMaskOverride` GATED (o gate `isEnabled` real, não
// bypassado) TAMBÉM provado — achou um componente `enabled=true && IsMeshComponent()=true`
// genuíno (`t0_000_pma_base__full_shadow`/`t0_000_pwa_base__full_shadow`, mesh de sombra do
// corpo base) e aplicou/restaurou o chunk mask através do gate composto de verdade — ver
// `codeware-componentwrapper-smoke.reds`.
native func BwmsComponentGetResourcePath(component: ref<IComponent>) -> ResRef
native func BwmsComponentSetResourcePath(component: ref<IComponent>, path: ResRef) -> Bool
native func BwmsComponentGetMeshAppearanceName(component: ref<IComponent>) -> CName
native func BwmsComponentSetAppearanceName(component: ref<IComponent>, appearance: CName) -> Bool
native func BwmsComponentIsMeshComponent(component: ref<IComponent>) -> Bool

@addMethod(IComponent)
public func GetResourcePath() -> ResRef {
    return BwmsComponentGetResourcePath(this);
}

@addMethod(IComponent)
public func SetResourcePath(path: ResRef) -> Bool {
    return BwmsComponentSetResourcePath(this, path);
}

// Divergência de nome CONSCIENTE vs a fonte real (`ComponentWrapper::GetAppearanceName`) — ver
// nota grande acima. Lê o campo `meshAppearance` do SUBTIPO, não `IComponent::appearanceName`
// (esse já é lido pela native vanilla `IComponent.GetAppearanceName()`).
@addMethod(IComponent)
public func GetMeshAppearanceName() -> CName {
    return BwmsComponentGetMeshAppearanceName(this);
}

@addMethod(IComponent)
public func SetAppearanceName(appearance: CName) -> Bool {
    return BwmsComponentSetAppearanceName(this, appearance);
}

// Nome real (`ComponentWrapper.hpp:20`); zero colisão conhecida (`grep` em `redscript-src`
// inteiro por "IsMeshComponent" -> zero hit, mesma checagem já feita pro resto do item).
@addMethod(IComponent)
public func IsMeshComponent() -> Bool {
    return BwmsComponentIsMeshComponent(this);
}

// ---------------------------------------------------------------------------------------------
// 2026-08-15 (RE ao vivo dedicada) — `CMesh::FindAppearance` FECHADO SEM ENDEREÇO NATIVO:
// scan-linear de `CMesh::appearances@0x1E0` (`DynArray<Handle<mesh::MeshAppearance>>`),
// comparando `MeshAppearance::name@0x30`, 100% composição sobre offsets do header OFICIAL do
// RED4ext.SDK (`CMesh.hpp`/`Generated/mesh/MeshAppearance.hpp`) — ver nota grande em
// `cp77-console/src/register.rs` (`garment_mesh_handle_offset`/`cmesh_find_appearance`). Isso
// remove o bloqueador que a rodada 18 (2026-08-15, 100% offline) tinha identificado como
// precisando de RE ao vivo (breakpoint/memory-watch sobre um ptr-to-member Itanium).
//
// `FindAppearanceProbe` NÃO é a `FindAppearance` real (que devolve o Handle inteiro) — é um
// DIAGNÓSTICO observe-only que devolve o `name` da entrada achada (0/vazio se não achou),
// pensado pra cross-validar contra `GetMeshAppearanceName()` (o nome que o componente já
// diz estar usando): se baterem, a cadeia inteira (Handle do mesh já resolvido + DynArray +
// campo `name`) está confirmada por um mecanismo independente do campo cru já provado.
//
// `ResetMaterialCache` (o método que dá nome ao item `#168`) CONTINUA fora de escopo — falta
// a metade "mutação" (`MeshAppearanceEx::ResetMaterialCache`: `WaitForJob`+lock+
// `RenderData::Release`+`Map::Clear()`), que precisa de RE de endereço nativo genuína
// (2 funções nunca resolvidas neste projeto) — não tentada aqui por risco (mexer em cache de
// render live sem entender o `Clear()` por completo).
native func BwmsComponentFindAppearanceProbe(component: ref<IComponent>) -> CName
native func BwmsComponentAppearanceCount(component: ref<IComponent>) -> Int32

@addMethod(IComponent)
public func FindAppearanceProbe() -> CName {
    return BwmsComponentFindAppearanceProbe(this);
}

@addMethod(IComponent)
public func AppearanceCount() -> Int32 {
    return BwmsComponentAppearanceCount(this);
}
