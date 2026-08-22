// codeware-systembyname.reds — `BwmsGetSystemByName` (achado do boot de descoberta 2026-08-14,
// `gisysmapdump 200`), Codeware `PENDENCIAS-UNIFICADAS.md` itens #75/#76/#138/#139/#141/#143.
//
// Generaliza pro redscript o comando de diagnóstico `sysbymap`/`gisysmapdump` (já provado ao
// vivo, RED4ext `#472`, 2026-08-11) — lê `GameInstance+0x08` (`systemMap: HashMap<IType*,
// Handle<IGameSystem>>`) por NOME, sem executar nenhum código do motor (mesma disciplina
// read-only já estabelecida). Cobre a lacuna real: `GameInstance.GetXSystem()` só existe pra uma
// fração dos ~149 sistemas registrados — qualquer sistema SEM accessor vanilla dedicado
// (`gamePopulationSystem`/`gamePersistencySystem`/`gameContainerManager`/`gameJournalManager`/
// `questWorldStateSystem`/`gameDynamicEntityIDSystem`/`gameEntityStubSystem`/
// `gameEntitySpawnerEventsBroadcasterImpl`/`gameGameTagSystem`/etc.) fica acessível assim.
//
// `wref<IScriptable>` (não-owning) — os `IGameSystem` são singletons ENGINE-gerenciados, vivem
// pelo resto da sessão, nunca precisam de retenção forte (mesma semântica já usada por
// `BwmsInkLayerGetWindow`/`GetGameController`).
//
// ACHADO HONESTO (não escondido): resolver o ponteiro NÃO abre despacho de método — 2 dos alvos
// testados (`questWorldStateSystem`/`gamePopulationSystem`) têm 0 métodos expostos via RTTI
// (`funclistdump`, confirmado ao vivo) — precisam de RE de vtable-slot POR CLASSE (mesma
// categoria já resolvida pra `MappinSystem`/`CommunitySystem`) pra virar capacidade de verdade.
// O valor desta peça é resolver de vez a caracterização "singleton nunca localizado" — o
// bloqueador que resta é mais estreito (RE de método, não mais "não sei onde o objeto vive").
native func BwmsGetSystemByName(name: CName) -> wref<IScriptable>;
