// codeware-inklayerwrapper.reds — Codeware `#93` (`inkLayerWrapper.reds`, catálogo exaustivo),
// 2026-08-14 (mesma sessão que confirmou o singleton `Red::InkSystem::Get()` por cross-validação
// decisiva, `findinksystembss`+`inksystemvalidate`, e provou `BwmsInkSystemGetLayer(index)`
// devolvendo um HANDLE REAL/utilizável — `HISTORICO.md` 2026-08-14 topo). Item estava bloqueado
// desde 2026-08-12 ("só é útil se algo já entregar uma instância pronta ... que precisa do
// singleton") — com o singleton confirmado E um mecanismo pra obter o handle real
// (`BwmsInkSystemGetLayer`, já provado ao vivo), a via fica destravada.
//
// Divergência de escopo CONSCIENTE (documentada, não escondida): NÃO forja a classe
// `inkLayerWrapper` em si (`RTTI_DEFINE_CLASS` real na fonte C++, mas `layer: Handle<inkLayer>`
// como campo próprio exigiria bind de classe nova sem ganho real — o mesmo dado já é acessível
// via composição de campo sobre o handle já obtido). Em vez disso: 5 globais BWMS-própria que
// recebem o HANDLE DE LAYER JÁ OBTIDO (via `BwmsInkSystemGetLayer`, já provado) como PARÂMETRO
// — mesmo padrão já usado pro `#100`/clipboard e `#9`/`EntityComponentEvent.GetComponent`.
//
// `GetGameControllers()` (retorno `array<T>` na fonte real) vira count+index em vez de um
// array de verdade — evita a categoria de risco que já causou crash neste projeto antes da
// técnica do "trailer de allocator-vft" ser achada (`tweakxl-getrecordsarray`); mesmo padrão já
// usado pra `layerManagers`/`layers` (`GetLayerCount`+`GetLayer(index)`).
//
// TODOS os métodos abaixo são SÓ LEITURA — seguros pra smoke test automático.
native func BwmsInkLayerGetName(layer: ref<IScriptable>) -> CName;
native func BwmsInkLayerGetWindow(layer: ref<IScriptable>) -> wref<IScriptable>;
native func BwmsInkLayerGetGameController(layer: ref<IScriptable>) -> wref<IScriptable>;
native func BwmsInkLayerGetControllerCount(layer: ref<IScriptable>) -> Int32;
native func BwmsInkLayerGetControllerAt(layer: ref<IScriptable>, index: Int32) -> wref<IScriptable>;
