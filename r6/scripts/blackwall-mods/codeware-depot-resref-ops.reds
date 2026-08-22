// codeware-depot-resref-ops.reds — Codeware `Depot/ResourceReference.reds` (fragmento seguro do
// item #17, 2026-08-10/11 cont.192 + round 3 do catálogo Codeware, 2026-08-12). `ResRef`
// confirmado NATIVE REAL vanilla (orphans.script:23714, já base do trabalho de Casts.reds).
//
// `OperatorEqual`/`OperatorNotEqual(ResRef, ResRef)`: `Equals`/`NotEquals` são builtins
// genéricos já usados em toda parte do BWMS — aqui só habilitam a sintaxe `==`/`!=` pro tipo
// `ResRef` (sugar de operador, zero native novo).
//
// `GetHash()` — CORREÇÃO 2026-08-12 (round 3, retificado no mesmo dia): a nota anterior deste
// arquivo classificava `@addMethod(ResRef) GetHash` como "todo native, precisa de RE própria" —
// ERRADO, achado lendo a fonte real (`App/Depot/ResourceReference.hpp::ResourceScriptReferenceEx::
// GetHash`): `return aReference.resource.path;` — é a MESMA leitura de campo que
// `BwmsResRefToHash` (`Casts.reds`, item `#63`, PROVADO AO VIVO em 2026-08-10) já implementa:
// extrair o hash cru de dentro do `ResRef`. Bookkeeping puro — zero código Rust novo.
// **Divergência de escopo (achada no compile real, não no scratch-compile do agente — este
// achou `NO_MATCHING_OVERLOAD`, `this` dentro de `@addMethod(ResRef)` resolve pra `ref<ResRef>`,
// não `ResRef` puro, apesar de `ResRef` ser `struct`; sem receita conhecida no projeto pra
// "@addMethod em struct" até agora)**: função GLOBAL (`ResRefGetHash(self)`) em vez de método de
// instância — mesmo padrão de `OperatorEqual`/`OperatorNotEqual` acima, evita o problema de `this`
// por completo. Nome BWMS-próprio, não `ResRef.GetHash()` literal (ver regra de nome do projeto).
//
// `ToString()` continua de fora, genuinamente: chama `ResourcePathRegistry::ResolvePath(hash)`
// (lookup REVERSO hash->path), capacidade que este projeto nunca construiu (`resource.link`
// só indexa PATH->hash, nunca o sentido inverso) — RE/infra própria não feita. Resto do arquivo
// real (structs `ResourceRef`/`ResourceAsyncRef`, `OperatorAssignMultiply`) segue fora —
// forjaria classe nativa nova, categoria de risco alta já evitada neste catálogo.
// `ResRef` é tipo do próprio jogo; o redscript não traz os operadores dele prontos, então o
// mod tem de declará-los. Corpo trivial por natureza — comparar dois handles de recurso é
// delegar ao comparador do motor. O hash sai da nossa native (`BwmsResRefToHash`, Rust próprio).
public func OperatorEqual(lhs: ResRef, rhs: ResRef) -> Bool = Equals(lhs, rhs)

public func OperatorNotEqual(lhs: ResRef, rhs: ResRef) -> Bool = NotEquals(lhs, rhs)

public func ResRefGetHash(self: ResRef) -> Uint64 {
    return BwmsResRefToHash(self);
}
