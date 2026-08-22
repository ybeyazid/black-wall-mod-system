// codeware-worldstatesystem-community.reds — Codeware `App/World/WorldStateSystem.hpp`/`.cpp`
// (`PENDENCIAS-UNIFICADAS.md`/`CATALOGO-EXAUSTIVO-CODEWARE.md` item #84).
//
// RELEITURA 2026-08-14 (rodada dedicada, 100% offline — "Caminho A" do goal desta rodada:
// reler a fonte C++ REAL INTEIRA, incl. o `.cpp`, não só o header, procurando um subconjunto de
// menor risco). A investigação de 2026-08-12 (round 3, `HISTORICO.md`/catálogo) só tinha lido
// `WorldStateSystem.hpp` — concluiu "ALTO RISCO, forja de classe C++ nova do zero com 3 ponteiros
// de sistema interno nunca vinculados (`QuestPhaseRegistry`/`QuestPhaseExecutor`/`FactManager`)",
// e notou explicitamente que `ToggleNode`/`ToggleVariant` (as 2 funções de maior valor) "não têm
// sequer assinatura C++ detalhada no header lido (implementação só no `.cpp`, não inspecionada)".
//
// Lendo o `.cpp` agora (nunca lido antes neste projeto): a classe `WorldStateSystem` NÃO é uma
// peça monolítica de risco uniforme — ela reparte em 2 metades com perfil de risco MUITO
// diferente:
//
//   METADE A (Community/Spawner control por NodeRef) — `ActivateCommunity`/`DeactivateCommunity`/
//   `ResetCommunity`/`SetCommunityPhase`/`ActivatePopulationSpawner`/`DeactivatePopulationSpawner`/
//   `ResetPopulationSpawner` (+ os 2 getters `GetCommunity`/`GetPopulationSpawner`) usam SÓ
//   `m_communitySystem` — NUNCA tocam `m_questPhaseRegistry`/`m_questPhaseExecutor`/`m_factManager`.
//   E `m_communitySystem` é só `Red::GetGameSystem<Red::gameICommunitySystem>()` — o MESMO
//   `CommunitySystem` já acessível neste projeto via `GameInstance.GetCommunitySystem(self)`
//   (native VANILLA REAL, `orphans.script:11401`) e cujos 9 slots de vtable já foram
//   implementados e PROVADOS AO VIVO nesta MESMA sessão (item #77, `codeware-communitysystem.reds`
//   — `ActivateCommunity`/`SetCommunityPhase`/`ResetCommunity`/`DeactivateCommunity`/
//   `ActivateSpawner`/`ResetSpawner`/`DeactivateSpawner` chamados com hashes fabricados E reais,
//   zero crash; `GetCommunity`/`GetSpawner` confirmados decisivamente com hash real->handle
//   definido, hash fabricado->null).
//
//   METADE B (`ToggleNode`/`ToggleVariant`) — usa `m_questPhaseExecutor->ExecuteNode(...)`, que
//   PRECISA das classes forjadas `QuestPhaseExecutor`/`QuestPhaseRegistry` (risco alto confirmado,
//   igual à conclusão de 2026-08-12 — NÃO revisitado aqui, continua fora de escopo).
//
// O ÚNICO ingrediente que faltava pra Metade A não era "forjar WorldStateSystem" — era
// `Red::ResolveNodeRef(aNodeRef)`, que por sua vez é só um WRAPPER C++ fino em cima de
// `CallGlobal("ResolveNodeRef", ...)` — ou seja, é a MESMA `global func ResolveNodeRef(id: NodeRef,
// context: GlobalNodeRef) -> GlobalNodeRef` NATIVE VANILLA REAL já usada extensivamente pelo
// PRÓPRIO jogo (`redscript-src/orphans.script:31096`, chamada em `core/components/
// stimBroadcasterComponent.script`, `cyberpunk/ai/commands/*.script`, etc. — dezenas de call-
// sites reais). Zero RE, zero endereço nativo, zero classe forjada: é sugar redscript puro sobre
// 3 primitivas 100% vanilla (`ResolveNodeRef`/`GlobalNodeRef.IsDefined`/`Cast<EntityID>`) + os
// natives do #77 já provados hoje.
//
// Isso reduz `#84` de "13 métodos, TODOS bloqueados por forja de classe C++ nova" pra "9 de 13
// métodos (as mutações + getters de Community/Spawner) compostos SEM NENHUM risco novo — 0 RE,
// 0 classe forjada, 0 endereço nativo; só 4 (`IsReady`/`GetStreamingWorld`/`ToggleNode`/
// `ToggleVariant`) continuam genuinamente XL." Correção honesta da nota do catálogo de 2026-08-12
// ("TODOS declinados por segurança") — a maioria na verdade não precisava de forja nenhuma, só
// não tinha sido notada porque o `.cpp` nunca tinha sido lido.
//
// DIVERGÊNCIA DE ESCOPO CONSCIENTE (documentada, mesma disciplina de sempre): a API real do
// Codeware expõe esses métodos em `WorldStateSystem` (`GameInstance.GetWorldStateSystem()`,
// classe forjada). Aqui eles vivem como `@addMethod(CommunitySystem)` (a classe VANILLA REAL já
// usada pelo #77) — sufixo `ByNodeRef` marca a diferença. Um mod real esperando
// `GameInstance.GetWorldStateSystem().ToggleNode(...)`/`ActivateCommunity(nodeRef,...)` do
// Codeware oficial precisa adaptar pra `GameInstance.GetCommunitySystem(game).
// BwmsActivateCommunityByNodeRef(nodeRef,...)` — mesma política já aplicada a Casts/EntityID/etc.
//
// GAP HONESTO QUE PERMANECE (não escondido): a fonte real chama `Raw::CommunitySystem::Update(
// m_communitySystem, true)` (`AddressLib::CommunitySystem_Update`, hash Windows 1559565515)
// DEPOIS de toda mutação — commit explícito que o BWMS nunca resolveu (é `RawFunc`/endereço,
// não slot de vtable — precisa de RE de endereço Mac genuína, categoria diferente do resto deste
// item). SEM esse `Update`, a mutação pode não surtir efeito IMEDIATO/visível (spawn/despawn de
// NPC pode só refletir no próximo ciclo natural do `CommunitySystem`, não se sabe sem teste ao
// vivo). Documentado, não implementado — item `#84` continua REAL_GAP (regra de ouro: nem toda a
// API original está coberta, e a peça coberta nunca foi exercitada com o `Update` real).
//
// `IsNodeRefDefined`/`ResolveNodeRef`/`GlobalNodeRef.IsDefined`/`GlobalNodeID.GetRoot`/
// `Cast(GlobalNodeRef)->EntityID` — todos `native func`/`native struct` VANILLA REAIS
// (`orphans.script:58498`/`31096`/`23679`/`31114`/`23725`), zero invenção Codeware, zero RE.

@addMethod(CommunitySystem)
public func BwmsActivateCommunityByNodeRef(nodeRef: NodeRef, opt entryName: CName) -> Bool {
    let communityId: EntityID;
    if !BwmsResolveCommunityNodeRef(nodeRef, communityId) {
        return false;
    }
    this.BwmsActivateCommunity(communityId, entryName);
    return true;
}

@addMethod(CommunitySystem)
public func BwmsDeactivateCommunityByNodeRef(nodeRef: NodeRef, opt entryName: CName) -> Bool {
    let communityId: EntityID;
    if !BwmsResolveCommunityNodeRef(nodeRef, communityId) {
        return false;
    }
    this.BwmsDeactivateCommunity(communityId, entryName);
    return true;
}

@addMethod(CommunitySystem)
public func BwmsResetCommunityByNodeRef(nodeRef: NodeRef, opt entryName: CName) -> Bool {
    let communityId: EntityID;
    if !BwmsResolveCommunityNodeRef(nodeRef, communityId) {
        return false;
    }
    this.BwmsResetCommunity(communityId, entryName);
    return true;
}

@addMethod(CommunitySystem)
public func BwmsSetCommunityPhaseByNodeRef(nodeRef: NodeRef, entryName: CName, phaseName: CName) -> Bool {
    let communityId: EntityID;
    if !BwmsResolveCommunityNodeRef(nodeRef, communityId) {
        return false;
    }
    this.BwmsSetCommunityPhase(communityId, entryName, phaseName);
    return true;
}

@addMethod(CommunitySystem)
public func BwmsActivateSpawnerByNodeRef(nodeRef: NodeRef) -> Bool {
    let spawnerId: Uint64;
    if !BwmsResolveSpawnerNodeRef(nodeRef, spawnerId) {
        return false;
    }
    this.BwmsActivateSpawner(spawnerId);
    return true;
}

@addMethod(CommunitySystem)
public func BwmsDeactivateSpawnerByNodeRef(nodeRef: NodeRef) -> Bool {
    let spawnerId: Uint64;
    if !BwmsResolveSpawnerNodeRef(nodeRef, spawnerId) {
        return false;
    }
    this.BwmsDeactivateSpawner(spawnerId);
    return true;
}

@addMethod(CommunitySystem)
public func BwmsResetSpawnerByNodeRef(nodeRef: NodeRef) -> Bool {
    let spawnerId: Uint64;
    if !BwmsResolveSpawnerNodeRef(nodeRef, spawnerId) {
        return false;
    }
    this.BwmsResetSpawner(spawnerId);
    return true;
}

// Getters (só-leitura, mesmo par decisivamente provado hoje pro #77 — `BwmsGetCommunity`/
// `BwmsGetSpawner` já devolvem o ponteiro `instance` cru do `WeakPtr<T>`, 0 = indefinido).
@addMethod(CommunitySystem)
public func BwmsGetCommunityByNodeRef(nodeRef: NodeRef) -> Uint64 {
    let communityId: EntityID;
    if !BwmsResolveCommunityNodeRef(nodeRef, communityId) {
        return 0ul;
    }
    return this.BwmsGetCommunity(EntityID.ToHash(communityId));
}

@addMethod(CommunitySystem)
public func BwmsGetSpawnerByNodeRef(nodeRef: NodeRef) -> Uint64 {
    let spawnerId: Uint64;
    if !BwmsResolveSpawnerNodeRef(nodeRef, spawnerId) {
        return 0ul;
    }
    return this.BwmsGetSpawner(spawnerId);
}

// --- núcleo de resolução, compartilhado pelos 9 wrappers acima. Espelha byte-a-byte a lógica
// real de `Red::ResolveNodeRef`/`Red::ResolveEntityID` (WorldStateSystem.cpp linhas 41/57/68/
// etc.): resolve o NodeRef contra a raiz global (mesmo idioma usado em dezenas de `.script`
// vanilla reais, ex. `stimBroadcasterComponent.script:306`), falha limpo (false/EntityID
// indefinido) se o NodeRef não resolver — NUNCA chama a mutação de vtable com um ID inválido.
func BwmsResolveCommunityNodeRef(nodeRef: NodeRef, out communityId: EntityID) -> Bool {
    if !IsNodeRefDefined(nodeRef) {
        return false;
    }
    let resolved: GlobalNodeRef = ResolveNodeRef(nodeRef, Cast<GlobalNodeRef>(GlobalNodeID.GetRoot()));
    if !GlobalNodeRef.IsDefined(resolved) {
        return false;
    }
    communityId = Cast<EntityID>(resolved);
    return true;
}

func BwmsResolveSpawnerNodeRef(nodeRef: NodeRef, out spawnerId: Uint64) -> Bool {
    let communityId: EntityID;
    if !BwmsResolveCommunityNodeRef(nodeRef, communityId) {
        return false;
    }
    spawnerId = EntityID.ToHash(communityId);
    return true;
}
