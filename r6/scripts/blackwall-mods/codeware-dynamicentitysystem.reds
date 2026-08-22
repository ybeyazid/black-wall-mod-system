// -----------------------------------------------------------------------------
// Codeware.World.DynamicEntitySystem — Fase 2 do `cw-world-depot` (2026-08-06, cont.23) — FECHADO
// -----------------------------------------------------------------------------
//
// HISTÓRICO CORRIGIDO: as 3 tentativas de cont.18-20 (inkWidget, este arquivo completo, a variante
// de isolação) crasharam com a MESMA assinatura (`EXC_BREAKPOINT`/`SIGTRAP`, `brk #1`, endereço
// estático `0x103da2a60`, assert `baseEngineInit.cpp:1094` "Failed to initialize scripts data!")
// e foram documentadas como "categoria de risco genérica desconhecida — registrar classe/método
// RTTI novo pode crashar, causa raiz não identificada". ISSO ESTAVA ERRADO. cont.23 achou a causa
// real: este arquivo (como `codeware-inkwidgethelper.reds`) tinha `module Codeware.World` no MESMO
// escopo de `native class`/`native func` — bug JÁ DOCUMENTADO no projeto (memória
// cp77-redscript-module-plus-nativefunc-crash, achado 2026-07-25 em codeware-reflection.reds, só
// não checado antes de reinvestigar do zero): o compilador redscript qualifica o fullName de toda
// declaração native dentro de um `module` com o path do módulo (ex.
// "Codeware.World.BwmsGetDynamicEntitySystem"), mas o registro Rust (`register_global`/
// `register_method`) sempre grava o nome BARE — o bind do engine não casa os dois nomes e falha
// (log capturado ao vivo cont.23: "[fb] bind orch (kind=5/global-func) FALHOU pra
// 'Codeware.World.BwmsGetDynamicEntitySystem'"), travando no mesmo assert de sempre.
//
// FIX: remover a linha `module X.Y` (mesmo fix já provado em 2026-07-25 pro Reflection). Testado
// ao vivo com a variante de isolação (3 métodos): GAMEPLAY t=86s, zero crash, `rttidump`/`newobj`/
// `callon` confirmaram a classe forjada, instanciável e com métodos REALMENTE chamáveis
// (`IsReady()->true`, `IsPopulated(tag)->false`, `DeleteTagged(tag)` logou estado real). Esta é a
// versão COMPLETA (8 métodos, incl. os 4 que usam `EntityID` — RULED OUT como gatilho por esse
// mesmo teste de isolação, nunca foi o problema).
//
// Fonte real: `enablers/Codeware/scripts/World/DynamicEntitySystem.reds` — `public native class
// DynamicEntitySystem extends IGameSystem`, 21 métodos. Subconjunto implementado (bookkeeping puro,
// registry Rust-side, zero RE de endereço, zero chamada ao motor): `IsReady`/`IsRestored`/
// `IsManaged`/`IsTagged`/`IsPopulated`/`AssignTag`/`UnassignTag`/`DeleteTagged`. FORA desta rodada:
// `CreateEntity`/`DeleteEntity`/`EnableEntity`/`DisableEntity`/`GetEntity`/`GetTags`/`GetTagged`/
// `GetTaggedIDs`/`RegisterListener`/`UnregisterListener`/`UnregisterListeners` —
// precisam de `array<T>` de RETORNO (ABI mais complexa, não tentada ainda) ou spawn real de
// entidade (já coberto pelo mecanismo antigo, `CompanionSystem.SpawnSubcharacterOnPosition`).
//
// ROUND 8 (2026-08-12, item `#73`, "bulk-por-tag"): 5 getters ESCALARES novos, evitando de propósito
// a técnica de array-retorno-com-trailer (categoria que já crashou 2x neste projeto — TweakXL
// `GetRecordsArray`/2026-08-10, ArchiveXL — só resolvida com donor real emprestado; sem boot pra
// confirmar, não seguro reusar às cegas aqui). Mesmo idioma "Count()+ByIndex(i)" já usado por
// `ReflectionClass.GetPropertyCount/GetPropertyByIndex` e `Scripting.GetStackDepth/
// GetStackFrameFunction(i)` pra dar a MESMA capacidade prática de enumerar (`GetTags(id)`/
// `GetTaggedIDs(tag)` da fonte real) sem o risco. `GetTaggedID(tag)` é o singular da fonte real,
// devolve QUALQUER entidade tagueada (não documenta ordem — aqui, a de menor valor, determinística).
// Nomes `GetTaggedCount`/`GetTaggedIDByIndex`/`GetTagCount`/`GetTagByIndex` são API PRÓPRIA do BWMS
// (a fonte real não tem equivalente indexado — regra de nome já estabelecida em CLAUDE.md pra
// framework de 3os). Deliberadamente FORA desta rodada, mesma linha de raciocínio já usada pros
// itens `#79`/`#138`/`#139` (não repetir "classe aceita chamada, nada real por trás"):
// `RegisterListener`/`UnregisterListener`/`UnregisterListeners` dispatchariam um `DynamicEntityEvent`
// de ciclo-de-vida (`Created`/`Deleted`/`Spawned`/`Despawned`/`Dead`) que nunca aconteceria de
// verdade (`CreateEntity`/`DeleteEntity`/`EnableEntity`/`DisableEntity` — as únicas fontes honestas
// desses eventos — não estão implementados nesta build, só bookkeeping de tag). `EnableTagged`/
// `DisableTagged` (bulk) ficam de fora pelo mesmo motivo.

public native class DynamicEntitySystem extends IGameSystem {
    public native func IsReady() -> Bool
    public native func IsRestored() -> Bool

    public native func IsManaged(id: EntityID) -> Bool
    public native func IsTagged(id: EntityID, tag: CName) -> Bool

    public native func IsPopulated(tag: CName) -> Bool
    public native func AssignTag(id: EntityID, tag: CName) -> Void
    public native func UnassignTag(id: EntityID, tag: CName) -> Void
    public native func DeleteTagged(tag: CName) -> Void

    // Round 8 — getters escalares (evita array-retorno de propósito, ver comentário acima).
    public native func GetTaggedID(tag: CName) -> EntityID
    public native func GetTaggedCount(tag: CName) -> Int32
    public native func GetTaggedIDByIndex(tag: CName, index: Int32) -> EntityID
    public native func GetTagCount(id: EntityID) -> Int32
    public native func GetTagByIndex(id: EntityID, index: Int32) -> CName
}

public static native func BwmsGetDynamicEntitySystem() -> ref<DynamicEntitySystem>
