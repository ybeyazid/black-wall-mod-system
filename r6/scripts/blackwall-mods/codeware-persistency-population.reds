// codeware-persistency-population.reds — Codeware `PENDENCIAS-UNIFICADAS.md` itens
// #75/#76/#138/#139/#141/#143, continuação direta do achado do systemMap (2026-08-14,
// `codeware-systembyname.reds`). Aquele achado destravou o PONTEIRO de `gamePersistencySystem`/
// `gamePopulationSystem`/`gameContainerManager`/`questWorldStateSystem`, mas `funclistdump`
// confirmou (2 dos 4 testados) que essas classes não expõem API própria pro redscript — mesma
// categoria de `MappinSystem`/`CommunitySystem`, precisam de dispatch por SLOT DE VTABLE. Este
// arquivo ataca o item de MENOR risco/MAIOR valor: `IPersistencySystem`/`IPopulationSystem`
// (RED4ext.SDK vendorizado, `Scripting/Natives/gameIPersistencySystem.hpp`/
// `gameIPopulationSystem.hpp`, headers com nomes de método REAIS documentados — ao contrário de
// `IContainerManager`, cujo header não lista nenhum método além de `IGameSystem`, sem offset
// disponível). 2 GETTERS PUROS (sem mutação), mesmo shift Itanium Mac=Windows+0x08 já
// estabelecido neste projeto: `IPersistencySystem::GetEntityStatus(EntityID)` (Windows 0x1E8 →
// Mac 0x1F0) e `IPopulationSystem::IsSpawning(EntityID)` (Windows 0x1C8 → Mac 0x1D0).
//
// `sys` é `ref<IScriptable>` genérico (mesmo padrão já provado de `BwmsCallMethod`) — passe o
// handle devolvido por `GameInstance.GetPersistencySystem(game)` (VANILLA REAL,
// `orphans.script:11431`) ou por `BwmsGetSystemByName(n"gamePopulationSystem")` (não há
// `GameInstance.GetPopulationSystem` vanilla — zero ocorrência em `redscript-src`).
//
// `GetEntityStatus` retorna 0=NotPersisted/1=CanBePersisted/2=Persisted (enum C++ real, sem
// equivalente RTTI redscript — mesma divergência já documentada pro `gamedataMappinPhase` do
// MappinSystem). Ver detalhe completo em `cp77-console/src/register.rs`
// (`register_persistency_population_natives`).
native func BwmsPersistencySystemGetEntityStatus(sys: ref<IScriptable>, entityId: EntityID) -> Int32;
native func BwmsPopulationSystemIsSpawning(sys: ref<IScriptable>, entityId: EntityID) -> Bool;
