// codeware-inksystem-layers.reds — Codeware `#100`/`#120` (`InkSystem.hpp`,
// `PENDENCIAS-UNIFICADAS.md`), 2026-08-14 (mesma sessão que CONFIRMOU `Red::InkSystem::Get()`
// por cross-validação decisiva — `findinksystembss`+`inksystemvalidate`, ver `HISTORICO.md`
// 2026-08-14 topo). Implementa os métodos que dependiam do singleton nunca resolvido antes,
// reusando o mecanismo de descoberta agora como rotina chamável+cacheada
// (`get_inksystem_singleton()`, `lib.rs`) em vez de exigir varredura manual toda vez.
//
// Navegação até os layers de verdade (offsets CONFIRMADOS no header vendorizado real,
// `enablers/Codeware/src/Red/InkSystem.hpp`):
//   InkSystem+0x380 = layerManagers: DynArray<SharedPtr<InkLayerManager>>
//   layerManagers[0].instance = InkLayerManager* (mesmo padrão de `GetLayerManager()` real)
//   InkLayerManager+0x38 = layers: DynArray<Handle<inkLayer>> (o MESMO array que `GetLayers()`
//   real devolve — `layerManagers[0]->layers`)
//
// Divergência de escopo CONSCIENTE (documentada, não escondida): `inkLayer`/`inkWorldLayer` têm
// ZERO ocorrência em `redscript-src` (1764 arquivos) E zero símbolo em `symbols-demangled.txt`
// (68k símbolos) — diferente de `CMesh` (que TINHA símbolo RTTI real e mesmo assim CROU o bind
// ao ser declarado `native class` sem backing confirmado, 2026-08-10, item `#30` do catálogo
// ArchiveXL). Forjar `native class inkLayer extends IScriptable {}` às cegas repetiria essa
// categoria de risco já provada real neste projeto — as natives abaixo devolvem
// `CName`/`Int32` (identidade/contagem/índice) em vez de um `ref<inkLayer>` forjado. Não é a
// assinatura LITERAL do Codeware (`GetLayers()->array<ref<inkLayerWrapper>>`), mas entrega a
// MESMA capacidade PRÁTICA (enumerar layers reais do `InkSystem` vivo + achar um por classe)
// com risco zero de forge de tipo sem backing confirmado.
//
// `GetWorldWidgets()` fica só como GROUNDWORK (`FindWorldLayerIndex`) — a parte que falta
// (`worldLayer->components` + `GetWindow()`/`GetGameController()` por componente) precisa de
// offsets nunca confirmados, fora do escopo desta rodada.
//
// TODOS os métodos abaixo são SÓ LEITURA — seguros pra smoke test automático.
native func BwmsInkSystemGetLayerManagerCount() -> Int32;
native func BwmsInkSystemGetLayerCount() -> Int32;
native func BwmsInkSystemGetLayerClassNameAt(index: Int32) -> CName;
native func BwmsInkSystemFindLayerIndexByClassName(name: CName) -> Int32;
native func BwmsInkSystemFindWorldLayerIndex() -> Int32;

// BwmsInkSystemGetLayer(index) — 2026-08-14 (continuação): fecha a divergência das 5 natives
// acima (contagem/hash/índice, nunca um HANDLE de verdade) — devolve um HANDLE REAL pro layer
// C++ no índice dado, via `rtti::make_handle`/`ADDR_HANDLE_CTOR` (a rotina do PRÓPRIO MOTOR;
// acha o bloco de refcount JÁ existente do objeto — ele já vive dentro de
// `layerManagers[0]->layers`, um `DynArray<Handle<inkLayer>>` real — e só incrementa, não
// duplica). Retorno `ref<IScriptable>` (base vanilla real, sempre segura) em vez do tipo
// concreto do layer (sem backing redscript confirmado nesta RTTI, ver nota grande acima) — a
// identidade REAL (classe concreta) continua acessível via `rttidump`/`class_of`/`IsA` do canal
// de debug sobre o handle devolvido; só o tipo ESTÁTICO declarado aqui é genérico. Índice
// fora do alcance/ponteiro inválido devolve um handle NULO (nunca crasha).
native func BwmsInkSystemGetLayer(index: Int32) -> ref<IScriptable>;

// BwmsInkSystemWorldWidgetsDiag() — 2026-08-16 (rodada dedicada de RE ao vivo): diagnóstico
// OBSERVE-ONLY pra `GetWorldWidgets()` (a última peça do item #100/#93). Testa 2 candidatos de
// offset pra `worldLayer->components` (Windows +0x170 puro E +0x178, o shift Itanium+0x08 já
// confirmado noutros structs C++ deste projeto) e, se achar um DynArray bem-formado, loga o
// ponteiro de vtable cru + o endereço estático (vmaddr) de cada componente achado — pronto pra
// disassembly OFFLINE identificar `GetWindow()`/`GetGameController()` sem chamar nada às cegas.
// ZERO escrita, ZERO chamada de função nativa — só leitura guardada. Devolve `true` se achou pelo
// menos 1 candidato de `components` bem-formado (não confirma GetWindow/GetGameController — só a
// existência do array).
native func BwmsInkSystemWorldWidgetsDiag() -> Bool;

// BwmsInkSystemGetLayers() — 2026-08-18: fecha a peça `GetLayers()`-array do item `#100`, ZERO
// RE/native novo. Composição pura em redscript sobre 2 primitivas JÁ PROVADAS AO VIVO
// (`BwmsInkSystemGetLayerCount`, 2026-06-25; `BwmsInkSystemGetLayer(index)`, handle real via
// `rtti::make_handle`, 2026-08-14): `GetLayerCount()` é literalmente `layerManagers[0]->layers.size`
// — o MESMO espaço de índice que `GetLayer(index)` navega (`register.rs` comment confirma:
// "layerManagers[0]->layers.size"). Laço 0..count-1 devolvendo cada handle é o `GetLayers()->
// array<ref<inkLayerWrapper>>` real do Codeware, na mesma divergência de tipo já documentada
// pro `GetLayer` (retorno genérico `ref<IScriptable>`, sem forjar `inkLayer`/backing ausente da
// RTTI — categoria de risco CMesh-trap, evitada de propósito).
public func BwmsInkSystemGetLayers() -> array<ref<IScriptable>> {
    let result: array<ref<IScriptable>>;
    let count: Int32 = BwmsInkSystemGetLayerCount();
    let i: Int32 = 0;
    while i < count {
        ArrayPush(result, BwmsInkSystemGetLayer(i));
        i += 1;
    }
    return result;
}
