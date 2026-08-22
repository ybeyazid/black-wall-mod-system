// codeware-scriptableservice-state.reds — Codeware `#188` (`PENDENCIAS-UNIFICADAS.md`,
// `ScriptableServiceContainerState`/`ScriptableServiceContainer::SaveState`/`LoadState`), 2026-08-12
// (Codeware round 7, agente offline dedicado).
//
// FONTE REAL (`App/Scripting/ScriptableServiceContainer.hpp`): `SaveState`/`LoadState` são métodos
// PRIVADOS de `ScriptableServiceContainer` (C++, nunca redscript-facing) — o mecanismo real serializa
// TODOS os serviços registrados de uma vez (`ScriptableServiceContainerState{services:
// DynArray<Handle<ScriptableService>>}`, `RTTI_PERSISTENT(services)`) num arquivo próprio
// (`m_stateFilePath`, fora do save do jogo), no boot (`Build(stateDir)`) e no shutdown
// (`OnUninitialize`). NÃO é uma API que um mod chama — é infraestrutura interna do framework.
//
// BOOKKEEPING (achado desta rodada, ver HISTORICO.md 2026-08-12 "Codeware round 7"): a CAPACIDADE
// PRÁTICA que este item persegue ("serviço/mod com estado que sobrevive a REINÍCIO do jogo, não só
// da sessão") **já existe no BWMS desde 2026-08-05**, sob um mecanismo totalmente diferente e nunca
// cruzado contra este item do catálogo — `bwms-mod-settings.reds` (`BwmsModConfigGet`/
// `BwmsModConfigSet`, namespaced por mod-id) sobre `BwmsConfigGet`/`BwmsConfigSet`
// (`redscript-mod-persistence`, FECHADO 2026-07-13, arquivo externo `~/.bwms-modconfig.txt`, fora
// do save). É uma capacidade GENÉRICA (qualquer classe, não só `ScriptableSystem`) — mesma regra de
// nome-próprio-pra-framework-de-3os já estabelecida no projeto (a forma diverge da C++ real
// completamente: 1 arquivo flat key=value em vez de serialização binária de um `DynArray<Handle<T>>`
// inteiro), mas o EFEITO prático (estado persistente cross-boot) é idêntico.
//
// Peça nova desta rodada (ergonomia, zero native novo): 2 métodos em `ScriptableSystem` (a base
// VANILLA REAL já usada por TODOS os ports Codeware que o projeto rotula "ScriptableService" —
// `LocalizationSystem`/`PuppetStateSystem`/`CustomPopupManager`/etc., ver correção já documentada
// no item `#109`: "`ScriptableService` não auto-descobre no motor real deste build, `ScriptableSystem`
// sim") — auto-namespaced por `GetClassName()`, pra um serviço real chamar
// `this.BwmsSaveState("volume","0.8")`/`this.BwmsLoadState("volume")` sem ter que montar a própria
// chave namespaced à mão. Composição pura de 2 primitivas já 100% provadas (zero RE, zero native
// novo): `IScriptable.GetClassName()` (vanilla real, `orphans.script:11258`) + `NameToString`
// (global vanilla real, já em uso no projeto) + `BwmsModConfigGet`/`BwmsModConfigSet`
// (`bwms-mod-settings.reds`, já deployado).
//
// Checagem de colisão de nome (ANTES de escrever, lição `cw-real-mod-e2e`/`AttachController`):
// grep em `cp77-symbols/redscript-src/*.script` por `func SaveState(`/`func LoadState(` — ZERO hits
// em todo o corpus decompilado (1764 arquivos). Usando de qualquer forma o prefixo `Bwms*` (mesmo
// padrão de dezenas de outros itens deste catálogo) por segurança extra e pra deixar claro que é
// capacidade PRÓPRIA do BWMS, não um método oficial do Codeware.
//
// `ScriptableSystem` confirmado vanilla real: `NativeHudManager extends ScriptableSystem`
// (`orphans.script:17620`).

@addMethod(ScriptableSystem)
public func BwmsSaveState(key: String, value: String) -> Bool {
    return BwmsModConfigSet(NameToString(this.GetClassName()), key, value);
}

@addMethod(ScriptableSystem)
public func BwmsLoadState(key: String) -> String {
    return BwmsModConfigGet(NameToString(this.GetClassName()), key);
}
