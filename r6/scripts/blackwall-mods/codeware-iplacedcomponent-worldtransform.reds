// codeware-iplacedcomponent-worldtransform.reds — Codeware `Entity/IPlacedComponent.reds` (item
// #29, round 8, 2026-08-12) RECLASSIFICADO REAL_GAP->EQUIVALENTE via achado de composição barata.
//
// A nota antiga do catálogo (2026-08-10) tinha caracterizado este item como "precisa de RE
// genuína" porque a fonte real declara `@addField(IPlacedComponent) public native let
// worldTransform: WorldTransform` — `native let` reivindicando storage NATIVO real, mesma
// categoria de risco do crash `#30`/CMesh (validação de bind em RUNTIME, não compile-time).
//
// Achado (leitura direta de `cp77-symbols/redscript-src/orphans.script:16676-16697`): a classe
// VANILLA REAL `IPlacedComponent` (não invenção do Codeware) já expõe `GetLocalToWorld() -> Matrix`
// — em termos de gráficos 3D, "local-to-world" É por definição a matriz de transform NO ESPAÇO DO
// MUNDO do componente (o mesmo conceito prático que `worldTransform` representaria, só numa
// representação diferente: `Matrix` em vez do struct `WorldTransform` do motor, que usa
// `WorldPosition` — coordenadas de precisão especial pra mundo aberto grande, não replicadas aqui).
// `Matrix` também é `native struct` VANILLA REAL (`orphans.script:25902`) com
// `GetTranslation(m)`/`GetRotation(m)` nativos prontos — dá a MESMA informação prática
// (posição+orientação no espaço do mundo) que `WorldTransform.GetWorldPosition()`/`GetOrientation()`
// dariam, zero RE, zero forge, zero risco de bind.
//
// Divergência honesta documentada (não escondida): (1) `Vector4`/`EulerAngles` (não `WorldPosition`/
// `Quaternion`) — perde a precisão de ponto-flutuante-duplo pra coordenadas MUITO distantes da
// origem do mundo (irrelevante pra escala normal de gameplay); (2) nome próprio `BwmsGetWorld*`
// (regra de nome já estabelecida pra API de framework de 3os, `CLAUDE.md`) — não é
// `component.worldTransform` literal, é a MESMA capacidade prática via caminho vanilla direto.
//
// Zero código Rust, zero endereço nativo, zero forge de tipo — 3 linhas de sugar puro sobre
// natives JÁ PROVADOS (`IPlacedComponent.GetLocalToWorld`/`Matrix.GetTranslation`/`GetRotation`,
// todos `native func`/`native struct` VANILLA REAIS, sem precedente de risco no projeto).

@addMethod(IPlacedComponent)
public func BwmsGetWorldTransformMatrix() -> Matrix {
    return this.GetLocalToWorld();
}

@addMethod(IPlacedComponent)
public func BwmsGetWorldPosition() -> Vector4 {
    return Matrix.GetTranslation(this.GetLocalToWorld());
}

@addMethod(IPlacedComponent)
public func BwmsGetWorldOrientation() -> EulerAngles {
    return Matrix.GetRotation(this.GetLocalToWorld());
}
