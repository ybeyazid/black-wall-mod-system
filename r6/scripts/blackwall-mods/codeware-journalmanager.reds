// codeware-journalmanager.reds — Codeware item #47 (Quest/JournalManager.reds).
//
// Fonte real: `@addMethod(JournalManager) public native func GetEntries(request:
// script_ref<JournalRequestContext>) -> array<wref<JournalEntry>>` — a implementação C++ real
// (`App/Journal/JournalManagerEx.hpp`) chama `Raw::JournalManager::GetEntries(this, *request,
// entries)`, um RawFunc GENÉRICO que internamente decide qual categoria buscar (endereço nativo
// nunca resolvido no Mac — RE genuína, fora de escopo).
//
// Divergência CONSCIENTE (política já estabelecida do projeto: framework de 3os = API própria,
// sem obrigação de bater nome/assinatura 1:1): em vez do RawFunc genérico, `BwmsGetEntries`
// recebe um `kind` explícito e despacha pro getter REAL correspondente — `GetQuests`/
// `GetMetaQuests`/`GetContacts`/`GetTarots`/`GetInternetSites`/`GetInternetPages`/
// `GetCodexCategories`/`GetBriefings`, TODOS `native const func` confirmados reais em
// `redscript-src/core/systems/journalManager.script` (já dispatcháveis pela engine, sem RE
// nenhuma). 100% redscript puro — zero código Rust, zero forge de classe.
public enum BwmsJournalKind {
    Quests = 0,
    MetaQuests = 1,
    Contacts = 2,
    Tarots = 3,
    InternetSites = 4,
    InternetPages = 5,
    CodexCategories = 6,
    Briefings = 7,
}

@addMethod(JournalManager)
public func BwmsGetEntries(context: JournalRequestContext, kind: BwmsJournalKind, out entries: array<wref<JournalEntry>>) -> Void {
    switch kind {
        case BwmsJournalKind.Quests:
            this.GetQuests(context, entries);
            break;
        case BwmsJournalKind.MetaQuests:
            this.GetMetaQuests(context, entries);
            break;
        case BwmsJournalKind.Contacts:
            this.GetContacts(context, entries);
            break;
        case BwmsJournalKind.Tarots:
            this.GetTarots(context, entries);
            break;
        case BwmsJournalKind.InternetSites:
            this.GetInternetSites(context, entries);
            break;
        case BwmsJournalKind.InternetPages:
            this.GetInternetPages(context, entries);
            break;
        case BwmsJournalKind.CodexCategories:
            this.GetCodexCategories(context, entries);
            break;
        case BwmsJournalKind.Briefings:
            this.GetBriefings(context, entries);
            break;
        default:
            break;
    }
}
