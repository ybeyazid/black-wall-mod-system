// codeware-resourcetoken.reds — Codeware `Depot/ResourceToken.reds` (item #18 do catálogo
// exaustivo, `PENDENCIAS-UNIFICADAS.md`), 2026-08-12 (Codeware round 6).
//
// Fonte real (`App/Depot/ResourceToken.hpp`): `ResourceTokenWrapper` é uma classe C++ FORJADA
// (`RTTI_DEFINE_CLASS`, não `RTTI_EXPAND_CLASS`) que guarda um `SharedPtr<ResourceToken<T>>` bruto
// e o constrói via `ResourceTokenWrapper::FromResRef(const ResourceReference<>&)`. Redscript nunca
// vê um `ResourceReference<>` por valor (só o `ref<T>`/`RaRef<T>` campo que ele materializa) — o
// caminho prático real (e o único já provado nesta sessão, item #199) é resolver o token a partir
// de (objeto dono, nome da property), não de um valor passado à parte. Em vez de forjar uma nova
// classe RTTI que guarda um ponteiro cru de token (categoria de risco desnecessária — precisaria
// de storage nativo por instância nunca antes testado), este wrapper é 100% REDSCRIPT PURO
// guardando (owner, propName) e delegando pras 3 natives já provadas/codadas do item #199 +
// `BwmsGetResourceReferenceTokenStatus` (nova, ver `register.rs`). Zero forge de classe nativa,
// zero risco novo de bind RTTI.
//
// Escopo consciente, 6/8 métodos: `GetResource`/`GetPath`/`GetHash`/`IsFinished`/`IsLoaded`/
// `IsFailed` fechados. `GetJobHandle`/`RegisterCallback` ficam de fora — o 1º devolveria um
// `JobHandle` que este projeto nunca marshalled (tipo opaco do Codeware, sem consumidor real
// ainda); o 2º precisa da `Callback<>` assíncrona nativa (`ResourceToken::OnLoaded`, `RawFunc` sem
// endereço Mac mapeado) — mesma categoria dos itens deixados de fora em `#199`.

public class ResourceToken extends IScriptable {
    // ⚠️ `ref` FORTE, não `wref` (corrigido 2026-08-21). Era `wref<IScriptable>` e produzia um
    // comportamento impossível: chamando o native DIRETO com um `ref` local o status vem `0/0/0` e as
    // comparações batem (`!=0`=false, `==1`=false, `==2`=false), mas passando por estes métodos —
    // que passam `this.m_owner` — saía `IsFinished=false` + `IsLoaded=true` + `IsFailed=true`, o que
    // é aritmeticamente impossível pra um único `Int32`. A ÚNICA diferença entre os 2 caminhos é o
    // campo `wref`. Mesma classe de bug de fundação já achada em 2026-08-18 nos itens `#97`/`#106`
    // ("referência fraca sem ninguém segurando um `ref` forte"), que lá fazia `OnInitialize()` nunca
    // disparar. Aqui o dono do recurso é um objeto do MOTOR de vida longa; segurar `ref` forte é
    // seguro e não cria ciclo (o `ResourceToken` é descartável, criado por `FromReference`).
    private let m_owner: ref<IScriptable>;
    private let m_propName: CName;

    // Divergência de nome consciente (regra de API de framework de 3os): a fonte real usa
    // `ResourceTokenWrapper::FromResRef(const ResourceReference<>&)`, sem equivalente redscript
    // direto. `FromReference(owner, propName)` é a via PRÁTICA real: aponta pro MESMO objeto+
    // property já usados por `BwmsGetResourceReferencePath` (item #199).
    public static func FromReference(owner: ref<IScriptable>, propName: CName) -> ref<ResourceToken> {
        let self: ref<ResourceToken> = new ResourceToken();
        self.m_owner = owner;
        self.m_propName = propName;
        return self;
    }

    // Divergência de tipo consciente: a fonte real devolve `ref<CResource>` — `CResource` (item
    // #15 do catálogo, marcado 🔶) NÃO existe como tipo redscript-declarável nesta RTTI (grep em
    // `redscript-src` inteiro: zero hits, mesma ausência já documentada). `BwmsGetResourceReferenceResource`
    // (#199) já devolve `ref<IScriptable>` — mantido aqui sem cast pra tipo inexistente.
    public func GetResource() -> ref<IScriptable> {
        return BwmsGetResourceReferenceResource(this.m_owner, this.m_propName);
    }

    public func GetPath() -> ResRef {
        return BwmsHashToResRef(this.GetHash());
    }

    public func GetHash() -> Uint64 {
        return BwmsGetResourceReferencePath(this.m_owner, this.m_propName);
    }

    // ⚠️ LER EM LOCAL ANTES DE COMPARAR (corrigido 2026-08-21). As 3 comparavam o retorno do native
    // INLINE (`Bwms...(...) != 0`) — e essa é a forma quebrada já documentada no projeto desde
    // 2026-07-15 (`Equals(BwmsConfigGet(...), "1")` inline falha; ler em local antes). Era a causa
    // do "triplo impossível" aberto desde 2026-08-15: `IsFinished=false` + `IsLoaded=true` +
    // `IsFailed=true` ao mesmo tempo, o que nenhum `Int32` único permite. Prova: instrumentando o
    // MESMO native com `let s: Int32 = ...` e comparando o LOCAL, saiu `0/0/0` com as 3 comparações
    // corretas (`!=0`=false, `==1`=false, `==2`=false). A única diferença entre os 2 caminhos era
    // inline vs local.
    public func IsFinished() -> Bool {
        let s: Int32 = BwmsGetResourceReferenceTokenStatus(this.m_owner, this.m_propName);
        return s != 0;
    }

    public func IsLoaded() -> Bool {
        let s: Int32 = BwmsGetResourceReferenceTokenStatus(this.m_owner, this.m_propName);
        return s == 1;
    }

    public func IsFailed() -> Bool {
        let s: Int32 = BwmsGetResourceReferenceTokenStatus(this.m_owner, this.m_propName);
        return s == 2;
    }
}

native func BwmsGetResourceReferenceTokenStatus(owner: ref<IScriptable>, propName: CName) -> Int32

// Diagnóstico 2026-08-17 pro achado NEGATIVO de 2026-08-15 (`mesh`/`rig` deram `loaded=true` E
// `failed=true` simultaneamente — impossível numa leitura estável). Devolve o `type_kind` CRU
// resolvido pra (owner,propName): 11=Ref/RaRef esperado, outro valor = campo não é o tipo certo,
// -1 = property não resolveu. Ver `tramp_resource_helper_debug_typekind` (register.rs).
native func BwmsResourceTokenDebugTypeKind(owner: ref<IScriptable>, propName: CName) -> Int32

// GLOBAL DE TESTE (2026-08-18, madrugada, continuação dedicada) — fixture SINTÉTICA de
// `inkWidgetBrush`, cross-referenciando o achado do item `#199` na MESMA madrugada: `scan11`
// achou `inkWidgetBrush.textureAtlas` como a 1ª propriedade `type_kind==11` (`Ref<T>`) REAL
// confirmada nesta RTTI, e `res11probe` já validou ao vivo que `new_object("inkWidgetBrush")` é
// seguro. Devolvido como `ref<IScriptable>` genérico (NUNCA declaramos `inkWidgetBrush` em
// redscript — classe vanilla real, sem backing em `redscript-src`, mesma disciplina anti-CMesh-
// trap já aplicada em todo o resto deste catálogo). Ver `tramp_make_test_inkwidgetbrush`
// (register.rs).
native func BwmsMakeTestInkWidgetBrush() -> ref<IScriptable>
