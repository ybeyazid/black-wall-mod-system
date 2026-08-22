// codeware-communitysystem.reds — Codeware `World/CommunityWrapper.reds`/`CommunitySystem`
// (`PENDENCIAS-UNIFICADAS.md` catálogo item #77, 2026-08-14, rodada offline).
//
// Fonte real (`enablers/Codeware/src/Red/CommunitySystem.hpp`): 7 dos 9 métodos de
// `Raw::CommunitySystem` são `Core::RawVFunc` — slot de VTABLE em bytes (Windows), NÃO
// `RawFunc`/`AddressLib` — mesma categoria zero-RE-de-endereço já usada pelo `MappinSystem`
// (`codeware-mappinsystem.reds`, item #142/#232): só precisa do shift Itanium Mac=Windows+0x08
// já estabelecido neste projeto pra vtable de classe C++ real.
//
//   Raw::CommunitySystem::ActivateCommunity   = RawVFunc<0x1B0, void(uint64_t, CName)>
//   Raw::CommunitySystem::DeactivateCommunity = RawVFunc<0x1B8, void(uint64_t, CName)>
//   Raw::CommunitySystem::SetCommunityPhase   = RawVFunc<0x1C0, void(uint64_t, CName, CName)>
//   Raw::CommunitySystem::ResetCommunity      = RawVFunc<0x1C8, void(uint64_t, CName)>
//   Raw::CommunitySystem::ActivateSpawner     = RawVFunc<0x1D8, void(uint64_t)>
//   Raw::CommunitySystem::DeactivateSpawner   = RawVFunc<0x1E0, void(uint64_t)>
//   Raw::CommunitySystem::ResetSpawner        = RawVFunc<0x1E8, void(uint64_t)>
//
// `GetCommunity`(0x228) — 2026-08-14 (mesma sessão, continuação): IMPLEMENTADO como
// `BwmsCommunityGetCommunity(sys, communityId) -> Uint64`, devolvendo o ponteiro `instance` cru
// do `WeakPtr<Community>` (0 = indefinido/não achado) em vez de forjar o tipo `Community`/um
// `wref<T>` real (mesma divergência de escopo consciente já usada no resto do item — `Red::
// WeakPtr<T>` é tipo próprio do Codeware, `lib/Core/SharedPtr.hpp`, nunca vendorizado neste
// projeto; assumido `{instance*,refCount*}` de 16 bytes, MESMO layout já confirmado pra todo
// `SharedPtr<T>`/`WeakHandle<T>` neste projeto). FECHOU com efeito real confirmado ao vivo
// (2 hashes distintos definidos, 1 fabricado nulo, ver `2026-08-14-codeware-77-getcommunity-
// crashfix-DECISIVO-PROVADO.log`).
//
// `GetSpawner`(0x260) — 2026-08-14 (mesma sessão, continuação): IMPLEMENTADO como
// `BwmsCommunityGetSpawner(sys, spawnerId) -> Uint64`, MESMA técnica exata (`CommunityWeakPtrSret`,
// sret real, mesma divergência de escopo — Uint64 cru em vez de forjar `Spawner`/`wref<T>`).
// Item `#77` fecha POR COMPLETO com este método — os 9 slots de vtable de `Raw::CommunitySystem`
// estão todos cobertos.
//
// `CommunitySystem` já é acessível via native VANILLA REAL confirmada (`cp77-symbols/
// redscript-src/orphans.script:11401`): `GameInstance.GetCommunitySystem(self: GameInstance)
// -> ref<CommunitySystem>` — `CommunitySystem extends ICommunitySystem extends IGameSystem`
// (`orphans.script:11605`/`50256`), tipo REAL do motor, não forjado por nós.
//
// `aCommunityID`/`aSpawnerID` = `uint64_t` cru — o hash de um `EntityID`/`gameCommunityID`
// (`gameCommunityID{entityId: EntityID}`, `enablers/Codeware/scripts/Base/Imports/
// gameCommunityID.reds`) — aceito aqui como `EntityID`/`Uint64` direto, sem forjar o wrapper
// `gameCommunityID` (mesma divergência de escopo já usada em vários itens deste catálogo:
// expor a capacidade prática sem forjar tipo novo quando o dado já é um hash simples).
//
// AS 7 NATIVAS MUTAM ESTADO REAL DE MUNDO (ativam/desativam spawn de NPC/crowd) — nunca
// chamadas automaticamente por nenhum smoke test deste projeto (mesma disciplina já usada pro
// `SetPoiMappinPhase` do MappinSystem — ver comentário em codeware-mappinsystem-smoke.reds).
// `GetCommunity` é SÓ LEITURA (não muta estado de mundo) — ainda assim testado com cautela
// (1 hash real + 1 fabricado, nunca em loop, ver `codeware-communitysystem-smoke.reds`), por
// ser dispatch de vtable genuinamente novo, nunca exercitado antes desta rodada.
// Item `#77` fica REAL_GAP até prova ao vivo dedicada (regra de ouro).

native func BwmsCommunityActivate(sys: ref<CommunitySystem>, communityId: EntityID, entryName: CName) -> Void;
native func BwmsCommunityDeactivate(sys: ref<CommunitySystem>, communityId: EntityID, entryName: CName) -> Void;
native func BwmsCommunityReset(sys: ref<CommunitySystem>, communityId: EntityID, entryName: CName) -> Void;
native func BwmsCommunitySetPhase(sys: ref<CommunitySystem>, communityId: EntityID, entryName: CName, phaseName: CName) -> Void;
native func BwmsCommunitySpawnerActivate(sys: ref<CommunitySystem>, spawnerId: Uint64) -> Void;
native func BwmsCommunitySpawnerDeactivate(sys: ref<CommunitySystem>, spawnerId: Uint64) -> Void;
native func BwmsCommunitySpawnerReset(sys: ref<CommunitySystem>, spawnerId: Uint64) -> Void;
native func BwmsCommunityGetCommunity(sys: ref<CommunitySystem>, communityId: Uint64) -> Uint64;
native func BwmsCommunityGetSpawner(sys: ref<CommunitySystem>, spawnerId: Uint64) -> Uint64;
// `BwmsCommunityScanSpawners` — 2026-08-14 (continuação): varredura em lote dos 10254 hashes
// extraídos pelo `save-parser` do node `CommunitySystem` do save ativo, procurando um hash que
// dê `GetSpawner!=0` (sinal decisivo de spawner genuíno, nunca achado nos 3 hashes testados
// manualmente antes — todos de comunidade). Read-only puro, loga só o resumo (ver register.rs).
native func BwmsCommunityScanSpawners(sys: ref<CommunitySystem>) -> Void;
// `BwmsCommunityScanSpawnersEndGame` — 2026-08-14 (continuação, retomada dedicada): MESMO
// mecanismo, mas contra os 10232 hashes extraídos de `EndGameSave-2` (`save-parser`, save
// GENUINAMENTE diferente do `AutoSave-5` — 13 hashes exclusivos, 35 hashes de AutoSave-5
// ausentes aqui). Pedido explícito de testar contra "outros saves" depois do achado negativo
// exaustivo (10254/10254 hashes de AutoSave-5 resolveram só como comunidade, zero spawner).
native func BwmsCommunityScanSpawnersEndGame(sys: ref<CommunitySystem>) -> Void;

@addMethod(CommunitySystem)
public func BwmsActivateCommunity(communityId: EntityID, entryName: CName) -> Void {
    BwmsCommunityActivate(this, communityId, entryName);
}
@addMethod(CommunitySystem)
public func BwmsDeactivateCommunity(communityId: EntityID, entryName: CName) -> Void {
    BwmsCommunityDeactivate(this, communityId, entryName);
}
@addMethod(CommunitySystem)
public func BwmsResetCommunity(communityId: EntityID, entryName: CName) -> Void {
    BwmsCommunityReset(this, communityId, entryName);
}
@addMethod(CommunitySystem)
public func BwmsSetCommunityPhase(communityId: EntityID, entryName: CName, phaseName: CName) -> Void {
    BwmsCommunitySetPhase(this, communityId, entryName, phaseName);
}
@addMethod(CommunitySystem)
public func BwmsActivateSpawner(spawnerId: Uint64) -> Void {
    BwmsCommunitySpawnerActivate(this, spawnerId);
}
@addMethod(CommunitySystem)
public func BwmsDeactivateSpawner(spawnerId: Uint64) -> Void {
    BwmsCommunitySpawnerDeactivate(this, spawnerId);
}
@addMethod(CommunitySystem)
public func BwmsResetSpawner(spawnerId: Uint64) -> Void {
    BwmsCommunitySpawnerReset(this, spawnerId);
}
@addMethod(CommunitySystem)
public func BwmsGetCommunity(communityId: Uint64) -> Uint64 {
    return BwmsCommunityGetCommunity(this, communityId);
}
@addMethod(CommunitySystem)
public func BwmsGetSpawner(spawnerId: Uint64) -> Uint64 {
    return BwmsCommunityGetSpawner(this, spawnerId);
}
@addMethod(CommunitySystem)
public func BwmsScanSpawners() -> Void {
    BwmsCommunityScanSpawners(this);
}
@addMethod(CommunitySystem)
public func BwmsScanSpawnersEndGame() -> Void {
    BwmsCommunityScanSpawnersEndGame(this);
}
