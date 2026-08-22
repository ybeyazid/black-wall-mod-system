// BWMS — CallbackSystem nativo real (2026-07-13), mesma técnica que fechou codeware-facade.reds.
// Declaração IDÊNTICA à fonte real do Codeware Windows (enablers/Codeware/scripts/Callback/
// CallbackSystem.reds) — "extends IGameSystem" explícito (diferente da Facade, que não tem
// "extends" e por isso herda IScriptable implicitamente). register.rs::register_callbacksystem
// forja a classe com o PARENT correto (IGameSystem, não IScriptable) — é o mesmo fix de
// parent-pointer que fechou a Facade, generalizado pra qualquer base explícita.
//
// Escopo desta fatia (cw-callbacksystem-rtti, ver ROADMAP-6-MODS-100.md F3/F4):
// RegisterCallback/UnregisterCallback funcionais (mesma registry global já provada em
// BwmsRegisterCallback); RegisterEvent permissivo; DispatchEventAs dispara por nome (payload
// do evento não marshallado ainda); RegisterStaticCallback/UnregisterStaticCallback/
// DispatchEvent são stubs documentados — dependem de CallbackSystemHandler/
// CallbackSystemEvent, gaps separados (cw-callback-handler/cw-event-target-classes).
// `CallbackSystemHandler` — NATIVA de verdade, PARCIALMENTE fechado (2026-07-18,
// `cw-callback-handler` AVANÇOU MUITO mas não fechou — ver evidencia_fresca_20260718 em
// gaps-revised.json pro relato completo e honesto).
// Histórico: placeholder SCRIPT (2026-07-15 a 2026-07-18) porque registrar métodos com param/
// retorno `ref<X>` de classe NATIVA FORJADA POR NÓS crashava o validador original (bisect
// completo, ver scriptableservice-native.reds pro relato paralelo da MESMA causa).
// **CAUSA RAIZ #1 ACHADA E CORRIGIDA 2026-07-18** (sessão `dynarraygrowth-probe`): não era o
// validador de tipo — era um hashmap embutido no forge da `CClass` com um vtable de alocador
// NUNCA inicializado (`register.rs::fix_embedded_allocator_vtables`, mesmo fix que fechou
// `ScriptableServiceContainer.GetService`). Forjar a classe + registrar os 6 métodos reais (+1
// BWMS-only, `HasTarget`) NÃO crasha mais — PROVADO ao vivo, 2 boots.
// **CAUSA RAIZ #2 — NOVA, ACHADA E AINDA NÃO CORRIGIDA (mesma sessão):** fazer `RegisterCallback`
// devolver uma instância REAL (via `rtti::new_object`) pra permitir o encadeamento
// `RegisterCallback(...).AddTarget(...).SetLifetime(...)` CRASHA num ponto DIFERENTE — não mais
// no forge/registro, mas quando o REDSCRIPT COMPILADO libera (refcount release) o `ref<>` local
// que guarda o retorno. RE ao vivo (crash-report, 2 tentativas de fix): um `ref<T>` em bytecode
// compilado é uma estrutura de 16 bytes (ponteiro do objeto + ponteiro pro bloco de refcount), e
// `rtti::new_object` só constrói o objeto cru, sem montar um bloco de refcount de verdade — falta
// achar a API real de construção de `Handle<T>` antes de tentar de novo. `RegisterCallback` fica
// revertido pro padrão seguro (devolve null, mesmo esquema documentado desde 2026-07-13 — quebra
// encadeamento, não crasha). Ver `register.rs::tramp_cbs_register_callback` pro relato RE completo
// + `cp77-symbols/notes/proofs/2026-07-18-cw-callback-handler-*.log`.
// Assinaturas de INTEROPERABILIDADE. Um mod escrito para o ecossistema de PC chama estes nomes;
// para ele funcionar aqui, nome e forma precisam casar — isso é interface, e a implementação por
// trás de cada uma é própria, em Rust (`register.rs::register_callbacksystem`). Agrupadas por
// ciclo de vida (montar o registro -> consultar -> desfazer).
public native class CallbackSystemHandler {
    // montar o registro (encadeável)
    public native func AddTarget(target: ref<CallbackSystemTarget>) -> ref<CallbackSystemHandler>
    public native func SetRunMode(runMode: CallbackRunMode) -> ref<CallbackSystemHandler>
    public native func SetLifetime(lifetime: CallbackLifetime) -> ref<CallbackSystemHandler>

    // consultar
    public native func IsRegistered() -> Bool

    // desfazer
    public native func RemoveTarget(target: ref<CallbackSystemTarget>) -> ref<CallbackSystemHandler>
    public native func Unregister()

    // BWMS-ONLY — NÃO existe na fonte real do Codeware. Expõe o filtro de alvos (a lista que
    // `AddTarget`/`RemoveTarget` mutam) como predicado testável por chamada de método real, prova
    // que o "filtro de target" do proof_needed é estado genuíno por-instância, não decorativo.
    public native func HasTarget(target: ref<CallbackSystemTarget>) -> Bool
}

// `CallbackRunMode`/`CallbackLifetime` — enums reais do Codeware (fonte:
// enablers/Codeware/scripts/Callback/CallbackRunMode.reds e CallbackLifetime.reds), plain
// redscript (sem native, sem risco de crash — são só valores inteiros).
public enum CallbackRunMode {
    Default = 0,
    Once = 1,
    OncePerTarget = 2,
}
public enum CallbackLifetime {
    Session = 0,
    Forever = 1,
}

// `CallbackSystemEvent` — NATIVA de verdade (2026-07-18, `cw-event-target-classes`, RETRY):
// antes era placeholder SCRIPT (`extends IScriptable`, sem native) porque a Tentativa 1 de 2026-
// 07-13 (ver nota abaixo) mostrou que um parent NÃO-nativo quebra o timing do validador pra
// `KeyInputEvent`. Agora nativa de verdade (SEM `extends` explícito = parent `IScriptable`
// implícito, mesma receita já provada 4x hoje), com `GetEventName()` real — vira o parent
// LEGÍTIMO de `KeyInputEvent` (ver classe abaixo, depois do histórico de 2026-07-13/15).
public abstract native class CallbackSystemEvent {
    public native func GetEventName() -> CName
}

// `CallbackSystemTarget` — NATIVA de verdade (2026-07-15, `cw-callback-handler`, PROVADA por boot
// real): classe abstrata SEM métodos, mesma receita segura da Facade/ScriptableService. Bisect
// confirmou classes forjadas SEM métodos são sempre seguras — o risco é específico de MÉTODOS com
// tipo ref<classe-forjada>.
public abstract native class CallbackSystemTarget {}

// `ComponentTarget extends CallbackSystemTarget` (2026-07-24, `cw-event-target-classes`) — as 2
// fábricas estáticas (`ID`/`Name`) usam `is_static=true` NUMA CLASSE FORJADA por nós, combinação
// nunca testada ao vivo antes (ver nota grande em `register.rs::register_componenttarget`) — os
// dois pilares (is_static=true em geral; new_object+make_handle sem self) já foram provados
// separadamente, mas não juntos. NÃO marcar como provado até um boot confirmar.
public native class ComponentTarget extends CallbackSystemTarget {
    public static native func ID(id: CRUID) -> ref<ComponentTarget>
    public static native func Name(name: CName) -> ref<ComponentTarget>
}

// 5 subclasses NOVAS `extends CallbackSystemTarget` (2026-07-24, rodada 2 — mesma sessão do
// `ComponentTarget` acima, MESMO veredito de risco: `is_static=true` + `new_object`+`make_handle`
// numa classe FORJADA por nós, combinação ainda NÃO testada ao vivo — ver nota grande em
// `register.rs::register_componenttarget`). Fontes reais confirmadas por leitura direta em
// `enablers/Codeware/scripts/Callback/Targets/*.reds`: todas `extends CallbackSystemTarget`
// DIRETO (nenhuma via parent intermediário). `array<T>` (`Tags`) e `EntityID` (`ID`) seguem de
// fora (marshalling separado, não coberto). `InputTarget` (usa `EInputKey`/`EInputAction`) fica
// declarada mais abaixo, perto do enum `EInputAction` (ver lá).
//
// RODADA 3 (2026-07-24, mesma sessão): `"ResRef"` entrou no whitelist `escalar` de `rtti.rs` (lê
// como o hash `ResourcePath`/u64, mesma representação canônica de `resource.link`) — destrava
// `Path`/`Library`/`Template`/`Definition`, adicionados agora. `Appearance` também adicionado:
// achado que sua assinatura real usa `CName`, não `ResRef` (tinha ficado de fora por engano de
// escopo da rodada 2, não por dependência real).
public native class ResourceTarget extends CallbackSystemTarget {
    public static native func Type(resourceType: CName) -> ref<ResourceTarget>
    public static native func Path(resourcePath: ResRef) -> ref<ResourceTarget>
}

public native class inkWidgetTarget extends CallbackSystemTarget {
    public static native func Controller(type: CName) -> ref<inkWidgetTarget>
    public static native func Library(library: ResRef, opt item: CName) -> ref<inkWidgetTarget>
}

public native class DynamicEntityTarget extends CallbackSystemTarget {
    public static native func Tag(tag: CName) -> ref<DynamicEntityTarget>
    // Codeware `#133`, 2026-08-18: fábrica PLURAL, insere TODAS as tags do array no mesmo
    // Set<CName> que `.Tag(x)` usa (mesma semântica OR do `Matches()` real, ver register.rs
    // `tramp_dyt_tags`). 1ª native deste projeto a receber `array<CName>` como parâmetro.
    public static native func Tags(tags: array<CName>) -> ref<DynamicEntityTarget>
}

public native class StaticEntityTarget extends CallbackSystemTarget {
    public static native func Tag(tag: CName) -> ref<StaticEntityTarget>
}

public native class EntityTarget extends CallbackSystemTarget {
    public static native func Type(entityType: CName) -> ref<EntityTarget>
    public static native func RecordID(recordID: TweakDBID) -> ref<EntityTarget>
    public static native func Template(templatePath: ResRef) -> ref<EntityTarget>
    public static native func Appearance(appearanceName: CName) -> ref<EntityTarget>
    public static native func Definition(appearancePath: ResRef, opt definitionName: CName) -> ref<EntityTarget>
}

// `cw-rawinput-realname` — TENTADO e REVERTIDO (2026-07-13): `KeyInputEvent extends
// CallbackSystemEvent` (native) CRASHOU o boot (EXC_BREAKPOINT/SIGTRAP dentro do assert-handler
// do próprio motor, `TCrashData<...>::Print` na stack — mesma categoria do crash de
// `GameInstance`/`@addMethod` cedo demais). Causa: `CallbackSystemEvent` é classe de SCRIPT PURA
// (não-native) — seu CClass é criado pelo pipeline de bind NORMAL, que não passa pelo
// `class_validate_probe_hook` (esse hook só intercepta classes `native`). Mesmo declarando
// `KeyInputEvent` NO MESMO ARQUIVO logo após `CallbackSystemEvent` (aposta de que ordem de
// bind = ordem de declaração), no momento em que o validador nativo processa `KeyInputEvent`,
// nem `reg.class_by_name("CallbackSystemEvent")` nem `resolve_class_via_validator_getclass`
// conseguem resolvê-la — ainda não existe. Ou seja: a hipótese "mesmo arquivo garante ordem"
// NÃO vale quando o parent é uma classe NÃO-nativa (só vale entre 2 classes nativas, como
// `CallbackSystemHandler/Event`(placeholder script)→`CallbackSystem`(native) funcionou por
// serem processadas por pipelines DIFERENTES sem essa dependência direta). Não há retry
// possível dentro do mesmo hook (retorno síncrono). Caminho pra reabrir: (a) declarar
// `KeyInputEvent extends IScriptable` direto (perde a hierarquia real, mas registra) ou (b)
// achar hook que rode DEPOIS do bind normal de scripts mas ainda a tempo do validador nativo
// (nenhum mapeado nesta sessão). Revertido pra restaurar boot estável; ver register.rs
// (função `register_keyinputevent` também revertida) e memória `cp77-codeware-port`.

// `cw-rawinput-realname` — RETRY 2 (2026-07-13, caminho (a) da nota acima) TAMBÉM CRASHOU.
// `KeyInputEvent extends IScriptable` via `register_type_min` (parent-pointer + fullName bare,
// MESMA receita exata que fechou a Facade) — desta vez `register_type_min`+os 5 `register_method`
// reportaram SUCESSO completo (classe forjada, todos os métodos registrados), mas o VALIDADOR
// ORIGINAL do motor ainda retornou 0 pra 'KeyInputEvent' (log confirmado), e o boot crashou mais
// tarde (durante `bind orch CHAMADO #2`, mesmo padrão de erro-agregado-que-só-explode-depois já
// documentado na saga da Facade). Ou seja: **existe um 3º fator, além de parent-pointer e
// fullName-bare, que ainda não foi isolado** — candidatos não descartados: (1) o retorno
// `EInputAction`/`EInputKey` (enums de SCRIPT, nunca usados como tipo de retorno de native antes
// nesta sessão) pode não bater no descritor de método que o compilador espera, fazendo o check
// "Missing native function X in native class Y" disparar por MISMATCH de assinatura, não de
// nome; (2) alguma divergência ainda não identificada entre a resolução de `IScriptable` via
// `class_by_name` (usada por `register_type_min`) e a que o VALIDADOR usa internamente pra esta
// classe específica (mesma categoria de bug já visto com `IGameSystem`, mas seria estranho
// afetar só esta classe e não a Facade). **2 tentativas, 2 recipes de parent diferentes, ambas
// crasharam — isto NÃO é mais um caso de "só faltava achar o parent certo".** Precisa de sessão
// de RE dedicada (nível dos "Tentativa 1-11" da Facade) pra isolar o 3º fator antes de tentar de
// novo — não é caso de "difícil = impossível", só não é mais um retry rápido e seguro. Revertido
// de novo (register_keyinputevent + hook removidos), boot reconfirmado limpo.

// TENTATIVA 3 (2026-07-18, sessão `handle-ctor-re`) — retomada depois de: (1) `fix_embedded_
// allocator_vtables` (fechou o crash de REGISTRO de método com `ref<classe-forjada>`, achado
// 2026-07-18 cedo); (2) `make_handle`/`ADDR_HANDLE_CTOR` (fechou o crash de RETORNO de `ref<>`
// real, mesma sessão); (3) um teste isolado de "retorno enum quebra o bind" rodado 4x nesta MESMA
// sessão NÃO reproduziu de forma confiável (3 de 4 boots limpos — ver nota em `GetTestEnum`
// removida, o achado revisado está em `cp77-symbols/notes/proofs/2026-07-18-cw-event-target-
// classes-enum-return-INCONCLUSIVO.log`). Diferença chave desta tentativa vs. as 2 de 2026-07-13:
// `CallbackSystemEvent` agora É nativa de verdade (acima) — corrige a causa raiz EXATA da
// Tentativa 1 (parent não-nativo). `KeyInputEvent` forjada com `register_type_instantiable_with_
// parent` (o forge robusto, não `register_type_min`) + parent = `CallbackSystemEvent` REAL.
// `GetAction() -> EInputAction` FORA por ora (2026-07-18): `EInputAction` é `[UNRESOLVED_TYPE]`
// no compilador — 0 erro de boot, erro de COMPILAÇÃO (pego ANTES de qualquer risco). Diferente de
// `EInputKey` (usado de verdade no bundle vanilla, `orphans.script:3223` + `SetInputKey(...,
// inputKey: EInputKey)`, tipo genuinamente pré-conhecido/primado), `EInputAction` NÃO aparece em
// NENHUM `.script` decompilado — é provavelmente um tipo C++-only que o Codeware real registra
// via SEU PRÓPRIO plugin RTTI no Windows (nunca rodou aqui, então nunca foi registrado). Forjar um
// ENUM nativo (não só classe) é RE/implementação nova, fora de escopo desta rodada — gap residual
// separado, documentado. `GetKey()->EInputKey` (tipo REAL pré-primado) fica dentro do teste — é o
// teste MAIS LIMPO possível da hipótese "enum-return quebra o bind" com um enum genuinamente já
// conhecido pelo sistema, ao contrário do `BwmsTestEnum` sintético do round anterior.

// RETOMADA (2026-07-24): `GetAction()` fechado. A nota acima assumia que `EInputAction` precisava
// ser um ENUM NATIVO (RE nova, forjar CEnum no RTTI) porque o tipo não existe em nenhum .script
// vanilla — mas isso só é verdade se algo NATIVO fora do nosso próprio bytecode precisasse
// consumir esse enum. Aqui não precisa: `GetAction()` só é chamado por `.reds` que NÓS compilamos
// (o scc resolve `EInputAction` contra QUALQUER declaração de enum no MESMO bundle, nativa ou
// não). Declarando um enum de SCRIPT PURO (sem `native`) com esse nome, `scc -compile` resolve
// limpo — testado offline numa cópia scratch de `r6/scripts` antes de tocar o deploy real, 0 erro
// de compilação. `tramp_kie_get_action` (register.rs) já existia e lê o valor de `KEYINPUT_STATES`
// (fixture action=2) — só faltava a declaração do tipo aqui + o `register_method` no Rust.
//
// RETOMADA (2026-08-05, blind-spot audit): valores/nomes ORIGINAIS (Press/Release/Hold/
// HoldComplete/Repeat) eram invenção nossa, nunca conferida contra a fonte real do Codeware
// (`enablers/Codeware/scripts/Base/Imports/EInputAction.reds`) — testei GetAction() AO VIVO
// (`bwms-blindspot-tests.reds`) e confirmou o mecanismo 100% funcional (fixture action=2 →
// GetAction()=2, zero crash), então o único problema era semântico: mods reais de Windows que
// checam `action == EInputAction.IACT_Press` receberiam o valor errado com os nomes antigos.
// Corrigido pros valores REAIS (IACT_None=0/IACT_Press=1/IACT_Release=2/IACT_Axis=3) — compatível
// byte-a-byte com o import real do Codeware, sem precisar de CreateScriptedEnum (é enum de script
// puro, resolvido só contra o bytecode que NÓS compilamos).
enum EInputAction {
    IACT_None = 0,
    IACT_Press = 1,
    IACT_Release = 2,
    IACT_Axis = 3,
}

public native class KeyInputEvent extends CallbackSystemEvent {
    public native func GetAction() -> EInputAction
    public native func GetKey() -> EInputKey
    public native func IsShiftDown() -> Bool
    public native func IsControlDown() -> Bool
    public native func IsAltDown() -> Bool
}

// Global de TESTE (2026-07-18) — constrói um `KeyInputEvent` com valores fixos de fixture (ver
// `register.rs::tramp_make_test_keyinputevent`), simula o que um `CallbackSystem.DispatchEvent`
// real entregaria a um listener. Prova a mecânica (forja+registro+construção+GetKey/GetAction)
// sem depender do wiring de teclado real (RawInput controller, gap separado, maior).
public static native func BwmsMakeTestKeyInputEvent() -> ref<KeyInputEvent>

// `InputTarget extends CallbackSystemTarget` (2026-07-24, rodada 2 do `cw-event-target-classes` —
// ver nota grande acima de `ResourceTarget` pro veredito de risco compartilhado). Declarada aqui
// (não junto dos outros 5 `Target`s acima) porque usa `EInputKey`/`EInputAction`, ambos já
// declarados/primados por este ponto do arquivo. `Key`/`Axis` são as 2 fábricas estáticas reais
// (`Callback/Targets/InputTarget.reds`); nenhum tipo `ResRef`/`array<T>` envolvido — os 2 métodos
// ficam DENTRO de escopo (categoria já provada: enum-como-param + Float-como-param).
public native class InputTarget extends CallbackSystemTarget {
    public static native func Key(key: EInputKey, opt action: EInputAction) -> ref<InputTarget>
    public static native func Axis(axis: EInputKey, opt threshold: Float) -> ref<InputTarget>
}

// `cw-event-target-classes` (2026-07-24): `AxisInputEvent extends KeyInputEvent` — fonte real
// (Codeware `Events/AxisInputEvent.reds`), zero tipo novo (Float/Uint32 já usados em várias
// natives deste projeto). Ver `register.rs::register_axisinputevent`.
public native class AxisInputEvent extends KeyInputEvent {
    public native func GetValue() -> Float
    public native func GetMouseX() -> Uint32
    public native func GetMouseY() -> Uint32
}
public static native func BwmsMakeTestAxisInputEvent() -> ref<AxisInputEvent>

// `cw-event-target-classes` (2026-07-24): `VehicleLightControlEvent extends
// EntityLifecycleEvent` — `vehicleELightType` é enum REAL vanilla (`orphans.script:401`,
// pré-primado, mesma categoria de `EInputKey`). `IsLightType` lê o param como enum — mesmo
// padrão já provado em `CallbackSystemHandler.SetRunMode`/`SetLifetime`.
public native class VehicleLightControlEvent extends EntityLifecycleEvent {
    public native func IsEnabled() -> Bool
    public native func IsLightType(lightType: vehicleELightType) -> Bool
}
public static native func BwmsMakeTestVehicleLightControlEvent() -> ref<VehicleLightControlEvent>

// `cw-event-target-classes` (2026-07-24, item Codeware #9): `EntityComponentEvent extends
// EntityLifecycleEvent`. Divergência FECHADA 2026-08-11: `GetComponent` volta a devolver
// `wref<IComponent>` (assinatura real), via `rtti::make_weak_handle` — mesma técnica que fechou
// RED4ext.SDK #387 pra `EntityLifecycleEvent.GetEntity`.
public native class EntityComponentEvent extends EntityLifecycleEvent {
    public native func GetComponent() -> wref<IComponent>
}
public static native func BwmsMakeTestEntityComponentEvent() -> ref<EntityComponentEvent>

// Codeware `#11` (`inkWidgetSpawnEvent`, candidato barato 2026-08-11, 6ª rodada de mineração):
// `inkWidgetSpawnEvent extends CallbackSystemEvent` — fonte real (`App/Callback/Events/
// InkWidgetSpawnEvent.hpp`) declara 3 campos (`itemInstance`/`libraryPath`/`itemName`).
// `GetLibraryPath() -> Uint64` (hash, mesma via já usada em `ResourceEvent.GetPath` pra evitar
// reabrir RE de marshalling `ResRef`-como-retorno) e `GetItemName() -> CName`.
//
// `GetItemInstance()` — CORREÇÃO DE RISCO 2026-08-18 (recheck contra o header Generated/, mesma
// técnica que destravou `#107`/`#33`): `inkWidgetLibraryItemInstance` É um tipo RTTI GENUÍNO desta
// build (`RED4ext.SDK/.../Generated/ink/WidgetLibraryItemInstance.hpp`, "gerado da Reflection
// real do jogo" + confirmado por 2ª via independente em
// `enablers/Codeware/scripts/Base/Imports/inkWidgetLibraryItemInstance.reds`, 1 dos 374 arquivos
// auto-descobertos por scraper de reflection RTTI) — a nota antiga ("ZERO hits em redscript-src")
// checava só o corpus decompilado do JOGO, não os headers/`.reds` do próprio framework. MAS
// `inkWidgetLibraryItemInstance extends ISerializable` (raiz, sem `IScriptable` no meio) é a MESMA
// categoria que crashou o bind RTTI 2x nesta sessão (`#48`/`#49`) — NUNCA declarar essa classe.
// `GetItemInstance() -> ref<IScriptable>` (tipo widened, mesma disciplina de `#160`/`#18`) resolve
// via side-table, sem nunca tipar/registrar `inkWidgetLibraryItemInstance`.
public native class inkWidgetSpawnEvent extends CallbackSystemEvent {
    public native func GetLibraryPath() -> Uint64
    public native func GetItemName() -> CName
    public native func GetItemInstance() -> ref<IScriptable>
}
public static native func BwmsMakeTestInkWidgetSpawnEvent() -> ref<inkWidgetSpawnEvent>

// Codeware `#123` (`WidgetSpawningService::ToggleWidgetSpawnEvent`, investigação dedicada
// 2026-08-19): a fonte real (`App/UI/WidgetSpawningService.hpp/.cpp`) é 1 linha —
// `s_widgetSpawnEventEnabled = aState` — sem endereço nativo nenhum. Liga/desliga o despacho
// REAL de "InkWidget/Spawn" a partir dos hooks já existentes de `axl-inkspawner-apply`
// (`SpawnFromLocal`/`SpawnFromExternal`, `cp77-console/src/selftest.rs`, RE offline 2026-07-28/29,
// nunca testados ao vivo). CODADO+COMPILA (2026-08-19), zero registro/hook novo além do já
// existente — só religa 2 mecanismos já provados separadamente (o hook do spawn + o forge da
// classe `inkWidgetSpawnEvent` acima, item `#11` já FECHADO). Gate opt-in por marcador
// `~/.bwms-inkspawner-fireevent-confirm` (nunca ligado por padrão, mesma disciplina do
// `#461`/`cw-ctrl-misc`) — a 1ª ativação ao vivo fica pra sessão futura com monitoramento de
// crash dedicado.
public static native func BwmsToggleWidgetSpawnEvent(state: Bool) -> Bool

// `cw-controller-session` (2026-07-18): `GameSessionEvent extends CallbackSystemEvent`, mesma
// receita robusta de `KeyInputEvent`. Despachado via "Session/Start"/"Session/End" (nomes REAIS
// do Codeware) na transição de presença do player — ver `register.rs::register_gamesessionevent`
// + `lib.rs::cp77_tick` pro mecanismo completo (NÃO usa hook de world-attach/detach).
public native class GameSessionEvent extends CallbackSystemEvent {
    public native func IsRestored() -> Bool
    public native func IsPreGame() -> Bool
}

// `cw-controller-entity` (2026-07-18): `EntityLifecycleEvent extends CallbackSystemEvent`, mesma
// receita robusta. Nome REAL "Entity/Attach" (`EntityAttachHook.hpp`), despachado na MESMA
// transição de presença do player (escopo = player), NÃO via hook de `Raw::Entity::Attach`
// (função nativa de alta frequência, dispara pra toda entidade do mundo — fora de escopo desta
// rodada). Fonte real declara `GetEntity() -> wref<Entity>` — TESTADO AO VIVO com
// `write_handle_ret {ptr,0}` (mesmo padrão 100% provado pra `ref<T>`, ex.: `GetCallbackSystem()`
// singleton): o campo leu como `IsDefined()==false` mesmo com ponteiro válido (1/1 boot,
// `EntityAttachEntityNullFAIL`). RE em `redscript/compiler` confirmou que o tipo de retorno
// (ref vs wref) é resolvido 100% em COMPILE-TIME pelo `scc` a partir do `.reds` — `build_native_
// func` (register.rs) NÃO popula descritor de tipo nenhum na CBaseFunction forjada (só vtable
// clonada+nome+flags), então o runtime nativo não "sabe" ref vs wref; só o bytecode compilado
// contra ESTE arquivo decide. Como `wref` exige um WeakRefCount block real (layout não
// RE-ado/não-provado neste projeto) pra `IsDefined` resolver `true`, e não temos orçamento pra
// essa RE nesta rodada, DECISÃO PRAGMÁTICA: declarar `ref<Entity>` (mesmo padrão 100% provado)
// em vez de `wref<Entity>`. Isso é uma DIVERGÊNCIA documentada do Codeware real: o handle
// devolvido é "raw"/sem dono (refcount=0, release=no-op, igual todo outro `ref<>` forjado deste
// projeto) — NÃO decrementa nem incrementa o refcount real do entity, e NÃO detecta destruição
// (diferente de uma weak-ref de verdade). Seguro pra uso SÍNCRONO no mesmo dispatch (o caso de
// uso normal); mods que guardarem o handle entre frames podem ficar com ponteiro pendurado se o
// entity for destruído — risco documentado, não testado neste round (fora de escopo).
// ATUALIZAÇÃO 2026-08-11 (RED4ext.SDK #387, `rtti::make_weak_handle`): `GetEntity()` agora
// devolve `wref<Entity>` REAL (não mais o `ref<Entity>` "raw" da divergência documentada acima,
// que ficou obsoleta) — testado ao vivo, `IsDefined()==true` confirmado. Ver
// proofs/2026-08-11-red4ext-387-weakhandle-PROVADO.log.
public native class EntityLifecycleEvent extends CallbackSystemEvent {
    public native func GetEntity() -> wref<Entity>
}

// `cw-controller-misc` (2026-07-19): `ResourceEvent extends CallbackSystemEvent`, mesma receita
// robusta. Nome REAL "Resource/Load" (`ResourceLoadHook.hpp`), despachado quando o hook
// resource.link (já instalado, já provado, ver `selftest.rs::reslink_lookup`) observa a
// construção REAL do `ResourcePath` armado por `watchres <path>` — NÃO hookamos
// `ResourceSerializer::SchedulePostLoadJobs` (offset Mac desconhecido, RE nova fora de escopo).
// Fonte real declara `GetPath() -> ResRef`; DIVERGÊNCIA documentada (mesma categoria do `ref` vs
// `wref` de `EntityLifecycleEvent` acima): `GetPath() -> Uint64` devolve o hash FNV-1a64 do
// ResourcePath — a representação canônica que TODO o mecanismo resource.link/reslinkdump deste
// projeto já usa pra "path", em vez de re-abrir RE de marshalling de retorno String/ResRef (gap
// residual documentado, mesma categoria do `array:String` do TweakDB). `GetResource`/`GetJobGroup`
// fora do escopo (não fazem parte do `proof_needed` literal, que só pede "path correto").
public native class ResourceEvent extends CallbackSystemEvent {
    public native func GetPath() -> Uint64
}

// Superfície de INTEROPERABILIDADE do sistema de callback. São só assinaturas: nenhum corpo mora
// aqui, e cada uma é atendida por implementação própria em Rust
// (`register.rs::register_callbacksystem`, registry própria, marshalling próprio). Os nomes e os
// tipos precisam casar exatamente com o que um mod escrito para o ecossistema de PC chama — é o
// formato do plugue, não a máquina por trás dele. Agrupadas por operação (assinar / cancelar /
// declarar / disparar), com o par instância+estático junto em cada grupo.
public native class CallbackSystem extends IGameSystem {
    // assinar
    public native func RegisterCallback(eventName: CName, target: ref<IScriptable>, function: CName, opt sticky: Bool) -> ref<CallbackSystemHandler>
    public native func RegisterStaticCallback(eventName: CName, target: CName, function: CName, opt sticky: Bool) -> ref<CallbackSystemHandler>

    // declarar um evento próprio antes de usá-lo
    public native func RegisterEvent(eventName: CName, opt eventType: CName) -> Bool

    // disparar
    public native func DispatchEvent(eventObject: ref<CallbackSystemEvent>)
    public native func DispatchEventAs(eventName: CName, eventObject: ref<CallbackSystemEvent>)

    // cancelar
    public native func UnregisterCallback(eventName: CName, target: ref<IScriptable>, opt function: CName)
    public native func UnregisterStaticCallback(eventName: CName, target: CName, opt function: CName)
}

// `cw-event-target-classes` — TESTE ISOLADO enum-return, 2026-07-18 (sessão `handle-ctor-re`),
// 4 RODADAS, TESTADO E REVERTIDO — CONCLUSÃO FINAL: NÃO CONFIRMADO/PROVAVELMENTE FLAKE AMBIENTAL.
// `native func GetTestEnum() -> BwmsTestEnum` (enum novo mínimo, numa classe JÁ forjada+provada,
// `CallbackSystem`) foi testado 4x com a MESMA config essencial:
//   ROUND 1: registro + mod caller real (`EnumInt(v)==1`) — CRASHOU (`EXC_BREAKPOINT`/SIGTRAP,
//     GameThread, link addr `0x103da2a60`, perto do assert conhecido em `0x103da2a34`).
//   ROUND 2: só registro, SEM caller, COM `~/.bwms-kind0-probe` (hook observe-only do validador
//     de enum, kind==0) — boot LIMPO até gameplay, `GetTestEnum=true`.
//   ROUND 3: registro + MESMO caller do Round 1, COM kind0-probe — boot LIMPO até gameplay,
//     `GetTestEnum()` chamado 2x, `EnumInt(v)==1` avaliou TRUE (branch PASS), zero crash.
//   ROUND 4 (decisivo): registro + MESMO caller do Round 1, SEM kind0-probe (config quase
//     IDÊNTICA ao Round 1) — boot LIMPO até gameplay, zero crash.
// **3 de 4 rodadas com a mesma classe de configuração NÃO crasharam** (incluindo uma repetição
// quase exata do Round 1) — a hipótese "retorno enum quebra o bind-pass" NÃO é reprodutível de
// forma confiável. Conclusão revisada (corrige a conclusão ROUND-1 anterior, que dizia
// "CONFIRMADO"): o crash do Round 1 é provavelmente um FLAKE AMBIENTAL (mesma categoria de
// crashes intermitentes já documentados neste projeto — ex. o crash `redDispatcher4` isolado
// como não-relacionado em `cw-callback-handler`, minutos antes nesta mesma sessão), NÃO uma
// consequência determinística de retorno-enum. Ver
// `cp77-symbols/notes/proofs/2026-07-18-cw-event-target-classes-enum-return-INCONCLUSIVO.log`
// pro relato completo revisado.

// `@addMethod(GameInstance) GetCallbackSystem()` FOI TENTADO e CAUSOU CRASH (2026-07-13): a
// classe "GameInstance" é validada pelo motor MUITO cedo (uma das primeiras ~100 entradas do
// bundle, ANTES de qualquer hook nosso ter chance de registrar o método nela — timing mais cedo
// que o próprio `register_all()`, que só roda no 1º kind=5/função-global, MUITO depois de todas
// as classes já terem sido validadas). Sem o método presente a tempo, o validador rejeita
// "GameInstance" (Missing native function 'GetCallbackSystem' in native class 'GameInstance'),
// derrubando o boot inteiro. Fix pragmático: expor o singleton via GLOBAL (não addMethod), mesmo
// padrão já provado de todas as Bwms* functions (registro cedo em register_all(), sem depender
// de nenhuma classe pré-existente). Perde compatibilidade LITERAL com mods que chamem
// `GameInstance.GetCallbackSystem()` (não implementado ainda — precisa achar um hook que rode
// ANTES da validação de classes começar, mais cedo que tudo já instrumentado nesta sessão), mas
// entrega a MESMA funcionalidade via um nome alternativo.
public static native func BwmsGetCallbackSystem() -> ref<CallbackSystem>
