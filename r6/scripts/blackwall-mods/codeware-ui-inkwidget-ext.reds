// -----------------------------------------------------------------------------
// inkWidget.BwmsAttachController/BwmsGetController — extensão GLOBAL, 2026-08-10
// -----------------------------------------------------------------------------
//
// FECHA `cw-real-mod-e2e` (o gap mais antigo do projeto, aberto 2026-06-21). Achado real via
// bisecção de 7 boots (cont.178/179, ver `HISTORICO.md`): um round-trip manual (attach
// direto num `new inkCanvas()`, sem navegação de menu) dava sempre achou=false. Isolei
// variável por variável:
//   1. Campo escalar (Int32) via @addField(inkWidget) em objeto new-construído: OK
//      (refuta "new quebra @addField em geral").
//   2. Campo ref<T> (strong/refcounted): SEMPRE achou=false, com ou sem `module Codeware.UI`
//      em volta, com ou sem conversão via `let` explícito.
//   3. Campo wref<T> (weak, sem refcount) + método/campo com nome ÚNICO
//      (`BwmsAttachController`/`BwmsGetController`, não `AttachController`/`GetController`):
//      achou=true same=true, 2/2, PRE e PÓS `Mount()`.
//   4. Teste decisivo: os nomes ORIGINAIS (`AttachController`/`GetController`, iguais ao
//      Codeware real) SEMPRE falharam — module-scope (dentro/fora de `module Codeware.UI`)
//      NÃO mudou o resultado; só RENOMEAR pra `BwmsAttachController`/`BwmsGetController`
//      resolveu, com os MESMOS tipos (wref internamente, ref<T> nas assinaturas públicas).
// Causa raiz CONFIRMADA por leitura de fonte (não mais só empírica): `inkWidget.GetController()
// -> wref<inkLogicController>` é uma NATIVE VANILLA REAL do próprio jogo (`redscript-src/core/
// ui/baseWidgets/abstractWidgets.script:8`, zero relação com Codeware) — nosso `@addMethod
// GetController()` colidia com ela. `AttachController` NUNCA aparece no `redscript-src` (é
// C++-only do Codeware, não existe stub vanilla) — plausível que só `GetController` precisasse
// do rename, mas não retestado separadamente (o fix combinado já está provado e implantado,
// não vale o custo de outro boot só pra reduzir divergência). O `scc` NÃO recusa a colisão em
// compile-time (o header antigo deste arquivo assumia que recusaria) — falha SILENCIOSA em
// runtime (nossa leitura resolve pro native vanilla, que devolve o link real do motor — sempre
// null/diferente, já que nunca populamos via o `AttachController` C++ que falta). Nome
// BWMS-próprio é 100% dentro da política do projeto (API de framework de 3os, sem obrigação de
// bater nome — ver CLAUDE.md "REGRA DE NOME/API").
//
// Achado lateral útil: `GetControllerByType(CName)`/`GetControllers()` (retorna array) TAMBÉM
// são natives vanilla reais em `inkWidget` (mesmo arquivo, linhas 10/14) — já usáveis DIRETO
// sem forge nenhum, se o projeto precisar de multi-controller-por-widget no futuro.
//
// Fica registrado como divergência: mods que esperassem `widget.AttachController(...)` (nome
// Codeware original) precisam chamar `widget.BwmsAttachController(...)` no BWMS.
//
// `module` só pode ser o 1º statement do arquivo (scc recusa no meio, testado ao vivo) — por
// isso este bloco (extensão de `inkWidget`, classe VANILLA global) mora num arquivo PRÓPRIO,
// fora de `module Codeware.UI` (onde `inkCustomController`, cont.179, continua).

@addField(inkWidget) let m_bwmsLogicController: wref<inkLogicController>;

@addMethod(inkWidget)
public func BwmsAttachController(controller: ref<inkLogicController>) -> Void {
    this.m_bwmsLogicController = controller;
}

@addMethod(inkWidget)
public func BwmsGetController() -> ref<inkLogicController> {
    let result: ref<inkLogicController> = this.m_bwmsLogicController;
    return result;
}
