// codeware-resourcehelper.reds — Codeware `App::ResourceHelper` (item #199 do catálogo,
// `PENDENCIAS-UNIFICADAS.md`) — declaração PERMANENTE das natives, composição pura de RTTI já
// provado (resolve_prop_in_class+type_kind+field_ptr), zero endereço nativo novo. Fonte real:
// `enablers/Codeware/src/App/Depot/ResourceHelper.hpp`.
//
// `BwmsGetResourceReferencePath` (2026-08-11): dado um objeto e o NOME de uma property do tipo
// `ResourceReference<T>`/`ResourceAsyncReference<T>`, devolve o hash FNV1a64 do path apontado — 0
// se a property não existir/não for do tipo certo/o valueHolder ainda não foi inicializado.
native func BwmsGetResourceReferencePath(owner: ref<IScriptable>, propName: CName) -> Uint64

// `BwmsIsResourceReferenceLoaded`/`BwmsGetResourceReferenceResource` (2026-08-12, rodada de
// composição barata, offline) — completam 2 dos 3 métodos restantes do item (só
// `LoadReferenceResource` segue de fora, genuinamente precisa de RE — dispara carregamento
// assíncrono via `ResourceLoader::Get()`, sem endereço mapeado). Só funcionam pra property do
// tipo `ResourceReference<T>` (COM token — `ResourceAsyncReference<T>` não tem esse campo,
// devolve sempre `false`/`null` nesse caso, nunca crash). `IsLoaded` = `token.finished &&
// !token.error` (campos de dado puros, `RED4ext.SDK/ResourceLoader.hpp`, zero endereço nativo).
// `GetReferenceResource` = `token.resource` (Handle<T> real, reconstruído via a rotina de handle
// do PRÓPRIO motor — `rtti::make_handle`, mesma técnica já usada em outras ~10 natives desta
// sessão). CODADO+testado offline (`resource_token_is_loaded`, 3 testes) — prova ao vivo pendente.
native func BwmsIsResourceReferenceLoaded(owner: ref<IScriptable>, propName: CName) -> Bool
native func BwmsGetResourceReferenceResource(owner: ref<IScriptable>, propName: CName) -> ref<IScriptable>
