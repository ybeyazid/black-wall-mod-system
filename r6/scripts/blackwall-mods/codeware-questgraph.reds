// codeware-questgraph.reds — Codeware `App/Quest/QuestPhaseGraphAccessor.hpp` (`PENDENCIAS-
// UNIFICADAS.md` catálogo item #160, 2026-08-15, sessão dedicada de varredura de itens
// "composição pura sobre native vanilla real, zero endereço nativo necessário" ainda sem
// código nenhum).
//
// Fonte real: `QuestPhaseGraphAccessor` é ~15 métodos de filtro sobre `questGraphDefinition.
// nodes` (via `Red::Cast<T>`+iteração de `DynArray`), TODOS inline C++ puro — ZERO
// `Core::RawFunc`/`Red::AddressLib` na classe inteira (lida por completo antes de codar).
// Generalizamos aqui o construto-base que a maioria dos 15 métodos reusa
// (`GetNodesOfType<T>()`), em vez de portar 1 native por método — cobre com certeza os casos
// de filtro DIRETO (`FindInputNode`/`FindManagerEvents`/`FindJournalEntries`, sem Cast
// aninhado em `.actions`/`.condition`) e serve de base pra completar o resto (Community/
// SpawnSet/Spawner/FactChanges/FactConditions/JournalConditions/LootConditions/
// LootContainerCondition/CharacterKillCondition/GetAllGraphNodePaths) numa sessão futura.
//
// Cadeia de acesso, TODOS offsets de CAMPO (zero shift Itanium — regra já estabelecida neste
// projeto pra `Core::OffsetPtr`, ao contrário de `RawVFunc`):
//   GameInstance.GetQuestsSystem(game) -> ref<QuestsSystem>   [native VANILLA real, orphans.
//                                                               script:11445]
//   QuestsSystem+0x68       = RootPhase: Handle<questPhaseInstance>   [header curado Codeware,
//                                                                       já usado por #77/#84/#157]
//   QuestPhaseInstance+0x40 = Graph: Handle<questGraphDefinition>     [mesmo header curado]
//   questGraphDefinition+0x30 = nodes: DynArray<Handle<GraphNodeDefinition>>  [header GERADO da
//                                                                       reflection real, RED4ext.
//                                                                       SDK/.../Generated/graph/
//                                                                       GraphDefinition.hpp]
//
// Disciplina anti-CMesh-trap: os ~11 tipos concretos de nó (`questInputNodeDefinition`/
// `questEventManagerNodeDefinition`/`questJournalNodeDefinition`/etc.) têm ZERO ocorrência em
// `redscript-src` — declarar a cadeia de ancestralidade inteira às cegas seria a MESMA
// categoria de risco que já crashou o `#30`/CMesh (bind RTTI recusa destrutivamente se a
// ancestralidade declarada não bate com a real). Por isso TODO handle de nó devolvido aqui é
// `ref<IScriptable>` (base vanilla real, sempre segura) — mesma disciplina já usada pro
// `#100`/`GetLayer` — o mod-author confirma o tipo concreto via `GetClassName()` (mesmo padrão
// que fechou aquele item). O filtro é por `type_hash: CName` passado pelo REDSCRIPT (o
// mod-author escreve `n"questInputNodeDefinition"` etc.) — zero hardcode nosso de nome, zero
// declaração de tipo nova.
//
// Item `#160` fica REAL_GAP até prova ao vivo (regra de ouro) — ainda que o mecanismo, após a
// extensão de 2026-08-18 (3 natives genéricas de offset arbitrário, ver bloco abaixo), cubra
// AGORA 7 dos 15 métodos originais em vez de 3 (`FindInputNode`/`FindManagerEvents`/
// `FindJournalEntries` + `FindFactChanges`/`FindCommunities`/`FindSpawnSets`/`FindSpawners`), a
// leitura nunca foi exercitada contra um `QuestsSystem` vivo.

native func BwmsQuestPhaseGetRootGraph(sys: ref<QuestsSystem>) -> ref<IScriptable>;
native func BwmsQuestGraphNodeCount(graph: ref<IScriptable>, typeName: CName) -> Int32;
native func BwmsQuestGraphNodeAt(graph: ref<IScriptable>, typeName: CName, index: Int32) -> ref<IScriptable>;

@addMethod(QuestsSystem)
public func BwmsGetRootQuestGraph() -> ref<IScriptable> {
    return BwmsQuestPhaseGetRootGraph(this);
}

// -----------------------------------------------------------------------------------------
// 2026-08-18 (madrugada, continuação 12) — extensão offline do item #160: 3 natives GENÉRICAS
// (leitor de campo `Handle<T>` num offset arbitrário + leitor de array `DynArray<Handle<T>>`
// num offset arbitrário) destravam, por COMPOSIÇÃO em redscript, os métodos que a fonte real
// resolve via Cast ANINHADO em `.actions`/`.condition`/`.type` — sem 1 função Rust nova por
// método e SEM declarar nenhum dos ~6 tipos concretos de node (mesma disciplina anti-CMesh-trap
// já em uso: filtro por `CName` passado pelo mod-author, retorno sempre `ref<IScriptable>`).
//
// Offsets confirmados nos headers GERADOS da reflection real do jogo (confiança máxima — não
// são headers "curados" do Codeware, são auto-extraídos do binário real, mesma categoria que já
// fechou dezenas de itens deste catálogo):
//   RED4ext.SDK/.../Generated/quest/SpawnManagerNodeDefinition.hpp
//     -> actions: DynArray<SpawnManagerNodeActionEntry> @ +0x48
//   RED4ext.SDK/.../Generated/quest/SpawnManagerNodeActionEntry.hpp
//     -> struct de 0x10 bytes = o próprio Handle<SpawnManagerNodeType> (offset 0 do entry)
//   RED4ext.SDK/.../Generated/quest/FactsDBManagerNodeDefinition.hpp
//     -> type: Handle<IFactsDBManagerNodeType> @ +0x48
//   RED4ext.SDK/.../Generated/quest/PauseConditionNodeDefinition.hpp
//     -> condition: Handle<IBaseCondition> @ +0x48
//
// Regra de ouro: RE offline nunca fecha item sozinha — isto só AVANÇA o item #160 (mais 4 dos
// 15 métodos originais tornam-se compostos sem RE nova), prova ao vivo continua pendente.

native func BwmsQuestNodeHandleField(node: ref<IScriptable>, offset: Int32) -> ref<IScriptable>;
native func BwmsQuestNodeArrayCount(node: ref<IScriptable>, offset: Int32, typeName: CName) -> Int32;
native func BwmsQuestNodeArrayAt(node: ref<IScriptable>, offset: Int32, typeName: CName, index: Int32) -> ref<IScriptable>;

// Offset comum aos campos Handle<T> únicos já confirmados hoje (FactsDBManagerNodeDefinition.
// type, PauseConditionNodeDefinition.condition) — ambos herdam de um ancestral de mesmo tamanho
// base (0x48 hex = 72 decimal, redscript não tem literal hex — coincidência confirmada nos 2
// headers gerados, não suposição).
public static func BwmsQuestSingleHandleFieldOffset() -> Int32 {
    return 72; // 0x48
}

// Offset do array `actions` de `questSpawnManagerNodeDefinition` (mesmo header gerado acima).
public static func BwmsQuestSpawnActionsOffset() -> Int32 {
    return 72; // 0x48
}
