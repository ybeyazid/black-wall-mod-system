// archivexl-77-journalmanager-tracking.reds — ArchiveXL item #77 (`Red/JournalManager.hpp`),
// 6 RawVFunc nunca resolvidos: `GetTrackedQuest@0x1F8`/`GetTrackedPointOfInterest@0x208`/
// `GetEntryByHash@0x220`/`GetEntryHash@0x230`/`TrackQuestByPath@0x298`/`TrackPointOfInterest@0x2A0`.
//
// Achado (2026-08-12, round 5, agente dedicado, 100% offline): a mesma técnica de string-xref/
// `RegisterEventConnector` já foi exaustivamente tentada hoje pra este item (zero hit, ver
// checkpoints anteriores no mesmo arquivo `.md`) — a vtable ESTÁTICA de `JournalManager` também
// já foi tentada e refutada (`journalvtdump`/2026-08-11, "offset 0 = vtable" não bate pra este
// objeto). Ângulo NOVO desta rodada: ler o `.hpp` real mostra que os 6 métodos são `RawVFunc`
// (offset de vtable puro, sem `AddressLib`/hash) — mas cruzando contra o `redscript-src`
// DECOMPILADO (não mais o header C++ do ArchiveXL), `JournalManager` já expõe TODA a capacidade
// prática equivalente como NATIVE VANILLA direto, zero RE, zero vtable:
//   `GetEntry(hash: Uint32) -> wref<JournalEntry>`        (= GetEntryByHash)
//   `GetEntryHash(entry) -> Int32`                        (nome IDÊNTICO ao native vanilla — não
//                                                           precisa wrap, já é literalmente igual)
//   `GetTrackedEntry() -> wref<JournalEntry>`             (usado dentro do PRÓPRIO corpo de
//                                                           `JournalManager` real,
//                                                           `journalManager.script:179`:
//                                                           `this.GetTrackedEntry() as
//                                                           JournalQuestObjective`)
//   `TrackEntry(entry) -> Void` / `UntrackEntry()` / `IsEntryTracked(entry) -> Bool`
//   `GetEntryByString(uniquePath: String, className: String) -> wref<JournalEntry>` — usado em
//   PRODUÇÃO real pelo próprio jogo (`scriptedPuppet.script:563`, `readAction.script:12`,
//   `player.script:2253`, todos `GetEntryByString(path, "gameJournalOnscreen")` pra
//   shard-reading) — confirma que o mecanismo funciona de verdade em gameplay real, não é
//   suposição.
//
// Divergência CONSCIENTE (política já estabelecida pra frameworks de 3os: API própria, sem
// obrigação de replicar o RawVFunc/endereço — mesmo padrão já usado em `codeware-
// journalmanager.reds`/Codeware#47): composto os 6 nomes do ArchiveXL sobre as natives vanilla
// confirmadas. `GetEntryHash` NÃO é redeclarado (já é native idêntico — redeclarar seria erro de
// redefinição). `GetEntryByHash` é 1:1 direto. `GetTrackedQuest`/`GetTrackedPointOfInterest`
// filtram por TIPO o único tracked-entry vanilla (cast null-safe, mesmo idioma já usado dentro
// do próprio `JournalManager` real). `TrackQuestByPath`/`TrackPointOfInterest` DIVERGEM do C++
// real de propósito: o `TrackQuestByPath` do Windows recebe `void* aPath` (struct `JournalPath`
// opaco, RE não feita) — aqui uso `GetEntryByString` (mecanismo DIFERENTE, string+className,
// já usado pelo próprio jogo) em vez do struct binário.
//
// Nomes de classe pro `GetEntryByString` (confirmados em `WolvenKit.RED4.Types.Classes`, não
// chutados): `"gameJournalQuest"` (quest) / `"gameJournalQuestMapPin"` (map-pin de POI, o
// concreto mais comum da família `gameJournalQuestMapPinBase`).
//
// NÃO TESTADO AO VIVO (100% offline por instrução desta rodada — golden rule do projeto: nunca
// declarar "fechado" sem artefato de boot). Confiança alta (zero RE de endereço, zero forge de
// classe, composição pura de natives já em uso real pelo PRÓPRIO jogo) mas item `#77` permanece
// REAL_GAP até prova ao vivo. Ver smoke test em `archivexl-77-journalmanager-tracking-smoke.reds`.
//
// ACRÉSCIMO 2026-08-15 (rodada dedicada aos 3 métodos COMENTADOS do header C++, ChangeEntryState/
// GetEntryState/GetEntryTimestamp): a arqueologia de bookkeeping de hoje mais cedo já tinha
// corrigido a caracterização desses 3 ("nunca resolvido oficialmente" era ERRADO — são
// `Core::RawVFunc`/slot de vtable, offset Windows conhecido: `ChangeEntryState@0x268`/
// `GetEntryState@0x2B0`/`GetEntryTimestamp@0x2C8`, ver checkpoint 2026-08-15 no catálogo). Antes
// de implementar via vtable Mac (+0x08 Itanium), confirmei em `redscript-src/core/systems/
// journalManager.script:40-64` algo AINDA MELHOR: os 3 (+ o irmão `ChangeEntryStateByHash`) já
// são `native (const) func` declarados DIRETO na classe `JournalManager` real, com os MESMOS
// nomes/assinaturas — **zero vtable Mac, zero shift Itanium, zero `@addMethod` necessário**,
// literalmente `jm.GetEntryState(entry)`/`jm.GetEntryTimestamp(entry)`/`jm.ChangeEntryState(...)`/
// `jm.ChangeEntryStateByHash(...)` já compilam e despacham como qualquer outra native vanilla.
// Confirmado por uso extensivo em PRODUÇÃO real (não suposição): `quest_tracker.script`,
// `questLog.script`, `journal_wrapper.script`, `messengerDialogView.script`,
// `messagePopup.script`, `codexUtils.script`, `newHudPhoneGameController.script` — dezenas de
// call-sites reais na UI do próprio jogo (quest log, telefone, codex, notificações), todos
// chamando exatamente essas 4 funções sobre entries reais. `GetEntryTimestamp` devolve `GameTime`
// (struct), não o `uint32_t` cru do header C++ — divergência trivial, `GameTime.GetSeconds(self)`
// (também native vanilla) converte pro mesmo domínio de valor. Smoke test estendido em
// `archivexl-77-journalmanager-tracking-smoke.reds` — lê `GetEntryState`/`GetEntryTimestamp` do
// tracked-entry real (se existir, prova o ramo POSITIVO que faltava também pro `GetTrackedQuest`
// já implementado acima) + exercita `ChangeEntryState`/`ChangeEntryStateByHash` com
// identificadores FABRICADOS (mesma disciplina de `TrackQuestByPath` acima — garantido não bater
// nenhuma entry real, zero risco de mutação de quest do usuário).

@addMethod(JournalManager)
public func GetEntryByHash(hash: Uint32) -> wref<JournalEntry> {
    return this.GetEntry(hash);
}

@addMethod(JournalManager)
public func GetTrackedQuest() -> wref<JournalQuest> {
    return this.GetTrackedEntry() as JournalQuest;
}

@addMethod(JournalManager)
public func GetTrackedPointOfInterest() -> wref<JournalQuestMapPinBase> {
    return this.GetTrackedEntry() as JournalQuestMapPinBase;
}

@addMethod(JournalManager)
public func TrackQuestByPath(uniquePath: String) -> Bool {
    let entry: wref<JournalEntry> = this.GetEntryByString(uniquePath, "gameJournalQuest");
    if !IsDefined(entry) {
        return false;
    };
    this.TrackEntry(entry);
    return true;
}

@addMethod(JournalManager)
public func TrackPointOfInterest(uniquePath: String) -> Bool {
    let entry: wref<JournalEntry> = this.GetEntryByString(uniquePath, "gameJournalQuestMapPin");
    if !IsDefined(entry) {
        return false;
    };
    this.TrackEntry(entry);
    return true;
}
