// codeware-casts.reds — `Utils/Casts.reds` do Codeware real (PENDENCIAS-UNIFICADAS.md item #63).
// `Cast()` na fonte real é 100% sugar redscript sobre nativas já providas (identidade pura de
// 8 bytes: CName/CRUID/NodeRef/EntityID/TweakDBID são todos wrappers de um único hash/u64).
// Nome próprio (regra do projeto: framework de 3os não é obrigado a replicar nome/forma).
// ACHADO: o `scc` vendorizado NÃO resolve overload por TIPO DE RETORNO (só por parâmetro) —
// o `Cast(value: Uint64) -> X` do Codeware real depende de um recurso que este compilador não
// tem (testado ao vivo: `let cruid: CRUID = BwmsCast(h)` escolhe cegamente o 1º overload
// Uint64->* declarado, erro de coerção). Em vez de replicar a ambiguidade, cada direção tem
// nome PRÓPRIO e explícito — mais simples, sem depender de um recurso do compilador ausente.
native func BwmsHashToName(hash: Uint64) -> CName
native func BwmsNameToHash(value: CName) -> Uint64
native func BwmsHashToCRUID(hash: Uint64) -> CRUID
native func BwmsCRUIDToHash(value: CRUID) -> Uint64
native func BwmsHashToNodeRef(hash: Uint64) -> NodeRef
native func BwmsNodeRefToHash(value: NodeRef) -> Uint64
// `CreateNodeRef` FECHADO 2026-08-15 (rodada 20) — achado que `Raw::NodeRef::Create` nunca
// precisou de endereço nativo: `NodeRef` é só um `uint64_t hash` e o algoritmo do hash
// (FNV-1a64 + regras de skip pra `#`/`;alias`) está 100% documentado no header oficial do SDK,
// já portado em `bwms_hashes::node_ref_hash`. Ver nota grande em `register.rs`
// (`tramp_string_to_noderef`).
native func BwmsCreateNodeRef(path: String) -> NodeRef
native func BwmsEntityIDToHash(value: EntityID) -> Uint64
native func BwmsHashToEntityID(hash: Uint64) -> EntityID

public func BwmsNodeRefToEntityID(value: NodeRef) -> EntityID = BwmsHashToEntityID(BwmsNodeRefToHash(value))
public func BwmsEntityIDToNodeRef(value: EntityID) -> NodeRef = BwmsHashToNodeRef(BwmsEntityIDToHash(value))

// TDBID.ToNumber já é NATIVA VANILLA do jogo (usada em dezenas de .script reais) — zero código
// novo.
public func BwmsTweakDBIDToUint64(value: TweakDBID) -> Uint64 = TDBID.ToNumber(value)
// `Uint64->TweakDBID` FECHADO 2026-08-10 (era "incerta, deferida" — mesma identidade-pura de 8
// bytes já provada pros outros 4 pares). Round-trip ao vivo confirmado (ver HISTORICO cont.166).
native func BwmsHashToTweakDBID(hash: Uint64) -> TweakDBID
// `ResourceAsyncRef->ResRef` FECHADO 2026-08-10 — mesma categoria (ambos wrappers flat de 1
// ResourcePath hash, `GetPath()` real é identidade pura). Nome próprio hash->ResRef (não replica
// `ResourceAsyncRef`, tipo interno do Codeware nunca modelado no BWMS). Par bidirecional
// (`ResRef->Uint64`) incluído pela mesma simetria já dada aos outros 4 pares.
native func BwmsHashToResRef(hash: Uint64) -> ResRef
native func BwmsResRefToHash(value: ResRef) -> Uint64

// `PersistentID.ToHash` (item #31, fix 2026-08-14) — algoritmo real de
// `RED4ext::game::PersistentID::GetHash()`, ver nota grande mais abaixo.
native func BwmsPersistentIDHash(entityHash: Uint64, componentHash: Uint64) -> Uint64

// Codeware `#69` (`TweakDBID.reds`, `@addMethod(TDBID) FromNumber`) FECHADO 2026-08-10 — nome
// OFICIAL do Codeware (item nomeado distinto de #63), API redscript nunca alinhada apesar do
// equivalente Rust já existir. Zero código novo: sugar puro sobre `BwmsHashToTweakDBID` (já
// provado, cont.166) — mesma identidade-pura de 8 bytes.
@addMethod(TDBID)
public static func FromNumber(number: Uint64) -> TweakDBID = BwmsHashToTweakDBID(number)

// Codeware `#65` (`Utils/EntityID.reds`) FECHADO 2026-08-10 — mesmo padrão do `#69`: nome
// OFICIAL nunca alinhado apesar do equivalente Rust já provado. Fonte real usa
// `EntityID.FromHash(value)` (um método Codeware-só, não vanilla — `EntityID.FromHash` não
// existe na RTTI do jogo, confirmado 0 hits em redscript-src) como intermediário; aqui vai
// direto no destino final já provado (`BwmsHashToEntityID`/`BwmsNodeRefToEntityID`), mesma
// identidade-pura de 8 bytes já fechada em #63 (cont.145/167). Overload por PARÂMETRO
// (Uint64 vs NodeRef) — já confirmado que o `scc` resolve isso (só falha por TIPO DE
// RETORNO, ver nota de topo).
public func ToEntityID(value: Uint64) -> EntityID = BwmsHashToEntityID(value)
public func ToEntityID(value: NodeRef) -> EntityID = BwmsNodeRefToEntityID(value)

// Codeware `#26` (`Entity/EntityID.reds`) FECHADO 2026-08-10 (cont.192) — nome OFICIAL
// `EntityID.FromHash`/`.ToHash` (confirmado 0 hits em redscript-src pros nomes de MÉTODO —
// `EntityID` como TIPO é vanilla onipresente, mas `FromHash`/`ToHash` são invenção do Codeware,
// zero colisão de nome). Sugar puro sobre `BwmsHashToEntityID`/`BwmsEntityIDToHash` (já
// provados, cont.145/167/183). Assinatura simplificada (`EntityID` por valor, não
// `script_ref<EntityID>` — mesma divergência já aceita em `TDBID.FromNumber`, chamada idêntica
// do lado do mod: `EntityID.ToHash(id)`).
@addMethod(EntityID)
public static func FromHash(hash: Uint64) -> EntityID = BwmsHashToEntityID(hash)
@addMethod(EntityID)
public static func ToHash(id: EntityID) -> Uint64 = BwmsEntityIDToHash(id)

// Codeware `#31` (`Entity/PersistentID.reds`) — `ForEntity`/`ForComponent`/`ToHash`, os 3/3
// métodos reais. `ForEntity` compõe o `Cast(EntityID)->PersistentID` VANILLA REAL
// (`orphans.script:16770`, zero RE) — devolve um `PersistentID` genuíno do MOTOR, não um valor
// forjado.
//
// `ForComponent` originalmente ficava de fora por não existir construtor vanilla
// "entity+component->PersistentID" achado — forjar o valor à mão (bytes crus) arriscaria produzir
// um `PersistentID` que `IsComponent()`/`GetComponentName()` (métodos vanilla REAIS que decodificam
// o layout interno) leriam errado. Achado 2026-08-12 (Codeware round 4, catálogo item #31): essa
// objeção só vale pra forja de bytes — existe uma 2ª via, NUNCA hand-forge, que a nota antiga não
// tinha considerado. `GameComponent` (`orphans.script:16697-16705`, `extends IComponent`) já
// declara `public final native const func GetPersistentID() -> PersistentID` — NATIVA VANILLA
// REAL, devolve um `PersistentID` genuíno computado pelo PRÓPRIO MOTOR (não um valor nosso).
// `ForComponent` (assinatura própria, divergente da original `(id: EntityID, component: CName)`
// do Codeware real — que exige o `HashMap` interno nunca RE'd — em favor de `ref<IComponent>`,
// mais direto pro caso comum "eu já tenho o componente na mão") faz o cast pra `GameComponent` e
// delega; componentes que NÃO são `GameComponent` (ex. a maioria dos `entMeshComponent`) devolvem
// um `PersistentID` default (`IsDefined()==false`) — comportamento HONESTO, não mentira: o motor
// nunca teve um PersistentID pra esses componentes de qualquer forma. Zero Rust, zero endereço
// nativo, zero risco de crash (cast falho em redscript nunca lança, só devolve `null`/indefinido).
// (`CATALOGO-EXAUSTIVO-CODEWARE.md` linha do item `#31` estava desatualizada descrevendo só
// 2026-08-10/"PARCIAL 2/3" — corrigido 2026-08-14, achado de bookkeeping puro: `ForComponent` já
// estava implementado e deployado desde 2026-08-12, nunca refletido de volta na tabela.)
//
// `ToHash` FIX 2026-08-14: antes só compunha `ExtractEntityID`+`BwmsEntityIDToHash` — CORRETO
// só pro caso ENTITY (componentName=None), que era o único caso que já existia até `ForComponent`
// ser implementado em 2026-08-12 (o bug ficou latente/nunca errado na prática). Header vendorizado
// "generated from the Game's Reflection data" (`RED4ext.SDK/Scripting/Natives/
// gamePersistentID.hpp`, confiança MÁXIMA) dá o algoritmo REAL de `PersistentID::GetHash()`
// (MurmurHash64A-like, `magic=0xC6A4A7935BD1E995`) — implementado em `persistent_id_hash`
// (`register.rs`, testado offline: `persistent_id_hash_tests`) e exposto via
// `BwmsPersistentIDHash(entityHash, componentHash)`. Agora `ToHash` cobre os 2 casos
// corretamente: `GetComponentName` (native vanilla real, `orphans.script:16761`) devolve CName
// "None" (hash 0) pro caso entity — `persistent_id_hash` detecta e devolve `entityHash` puro,
// IDÊNTICO ao comportamento antigo já provado; para o caso component, mistura os 2 hashes de
// verdade (nunca testado ao vivo antes de hoje, prova ao vivo pendente).
@addMethod(PersistentID)
public static func ForEntity(id: EntityID) -> PersistentID = Cast(id)
@addMethod(PersistentID)
public static func ToHash(id: PersistentID) -> Uint64 {
    let entityHash: Uint64 = BwmsEntityIDToHash(PersistentID.ExtractEntityID(id));
    let compHash: Uint64 = BwmsNameToHash(PersistentID.GetComponentName(id));
    return BwmsPersistentIDHash(entityHash, compHash);
}
@addMethod(PersistentID)
public static func ForComponent(component: ref<IComponent>) -> PersistentID {
    let gc: ref<GameComponent> = component as GameComponent;
    if IsDefined(gc) {
        return gc.GetPersistentID();
    }
    let undef: PersistentID;
    return undef;
}

// Codeware `#63` (`Casts.reds`) — nota de bookkeeping 2026-08-12 (Codeware round 4): a peça
// residual "`String↔LocalizationString`" (citada em notas antigas como "marshalling não resolvido
// em lugar nenhum do projeto") já está FECHADA desde o item `#66` (2026-08-12, mesmo dia, sessão
// anterior) — `ToLocalizationString(String)`/`BwmsExtractLocalizationString(LocalizationString)`
// (`codeware-localizationstring.reds`), provado ao vivo. Nunca creditado de volta a este item
// especificamente. `String↔NodeRef` FECHADO 2026-08-15 (rodada 20) — ver `BwmsCreateNodeRef`
// acima e `ToNodeRef(String)` abaixo. Item #63 completa 13/13 overloads.

// Codeware `Utils/CName.reds`/`Utils/CRUID.reds`/`Utils/NodeRef.reds` — nomes OFICIAIS nunca
// alinhados 2026-08-10 (cont.192, achado do agente de pesquisa: item #62 do catálogo já
// creditava a native Rust como "provada", mas a fonte real declara GLOBAIS (não @addMethod) sob
// estes nomes exatos, nunca expostas por nenhum .reds deployado). Confirmado 0 colisão pra cada
// nome exato (grep redscript-src, todos zero hits) — globais soltas, não @addMethod, sem risco
// de colisão same-class do #87. `CreateNodeRef(path: String)` FECHADO 2026-08-15 (rodada 20,
// ver `BwmsCreateNodeRef` acima) — assinatura simplificada (`String` por valor, não
// `script_ref<String>` — mesma divergência já aceita noutros itens deste arquivo).
public func HashToName(value: Uint64) -> CName = BwmsHashToName(value)
public func NameToHash(value: CName) -> Uint64 = BwmsNameToHash(value)
public func ToName(value: Uint64) -> CName = BwmsHashToName(value)
public func ToName(value: String) -> CName = StringToName(value)

public func HashToCRUID(value: Uint64) -> CRUID = BwmsHashToCRUID(value)
public func CRUIDToHash(value: CRUID) -> Uint64 = BwmsCRUIDToHash(value)
public func CreateCRUID(value: Uint64) -> CRUID = BwmsHashToCRUID(value)
public func ToCRUID(value: Uint64) -> CRUID = BwmsHashToCRUID(value)

public func HashToNodeRef(value: Uint64) -> NodeRef = BwmsHashToNodeRef(value)
public func NodeRefToHash(value: NodeRef) -> Uint64 = BwmsNodeRefToHash(value)
public func ToNodeRef(value: Uint64) -> NodeRef = BwmsHashToNodeRef(value)
public func ToNodeRef(value: EntityID) -> NodeRef = BwmsEntityIDToNodeRef(value)
// `CreateNodeRef(path)`/`ToNodeRef(path)` FECHADOS 2026-08-15 (rodada 20) — completa a família
// `ToNodeRef` com a 3ª sobrecarga (Uint64/EntityID/String), fecha #63 13/13.
public func CreateNodeRef(path: String) -> NodeRef = BwmsCreateNodeRef(path)
public func ToNodeRef(value: String) -> NodeRef = BwmsCreateNodeRef(value)

// Codeware `#101` (`GameInstance.GetSystemRequestsHandler()`) — candidato do agente de pesquisa
// (cont.192) sugeria `@addMethod(GameInstance)`, mas isso já foi TENTADO e CAUSOU CRASH real
// nesta mesma sessão (2026-07-13, ver `callbacksystem-native.reds`/`scriptableservice-native.
// reds`): "GameInstance" é validado pelo motor MUITO cedo (~primeiras 100 entradas do bundle),
// ANTES de `register_all()` ter chance de rodar — qualquer `@addMethod(GameInstance)` derruba o
// boot inteiro ("Missing native function ... in native class 'GameInstance'"). Mesmo fix
// pragmático já estabelecido: GLOBAL solta (não toca a classe `GameInstance`), mesma capacidade
// (o mecanismo que `bwms-autocontinue.reds`/`bwms-settings-poc.reds` já usam internamente via
// `this.GetSystemRequestsHandler()` dentro de um `inkMenuScenario` real), sem risco de timing.
public func BwmsGetSystemRequestsHandler() -> wref<inkISystemRequestsHandler> = new inkMenuScenario().GetSystemRequestsHandler()
