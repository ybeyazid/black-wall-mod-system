// bwms-ui-widgetfind.reds — busca na árvore de widgets (catálogo Codeware #89), REESCRITO.
//
// POR QUE ESTE ARQUIVO EXISTE: a versão anterior saiu em 2026-08-22 na auditoria de originalidade.
//
// UMA DIVERGÊNCIA QUE NÃO É ESCOLHA — é o que a engine permite:
// a implementação de terceiro subia a árvore com `widget.GetParentWidget()`. Esse método **não
// existe no redscript do jogo** (grep em `cp77-symbols/redscript-all.reds`: zero ocorrência como
// método de `inkWidget`) — quem o fornecia era o C++ do framework, que ESTENDE a classe vanilla
// via `RTTI_EXPAND_CLASS`. O BWMS não tem esse mecanismo (é o mesmo bloqueio já registrado no
// item `#91`), então **subir a árvore é impossível aqui**, e prometer `InWindowTree(widget)` com
// um argumento só seria vender o que não se entrega.
//
// A engine deixa DESCER (`GetNumChildren`/`GetWidgetByIndex`). Toda a API abaixo é escrita em
// cima disso: quem chama informa a raiz. Na prática é o que um mod tem de qualquer jeito — ele
// acabou de criar ou receber o container onde está montando a UI.
//
// TRAVESSIA POR PILHA EXPLÍCITA, não recursão: é o estilo do projeto (procedural/data-oriented),
// e aqui tem razão técnica — árvore de UI vinda de mod é de profundidade desconhecida, e o teto
// de passos abaixo é uma proteção real contra árvore cíclica ou monstruosa travando um frame.
//
// DEPENDÊNCIAS confirmadas por grep ANTES de escrever (`cp77-symbols/redscript-all.reds`):
//   - `inkCompoundWidget.GetNumChildren() -> Int32`                    (`:55017`)
//   - `inkCompoundWidget.GetWidgetByIndex(index) -> wref<inkWidget>`   (`:55025`)
//   - `inkWidget.GetControllers() -> [wref<inkLogicController>]`       (`:54597`)
//   - `IScriptable.IsA(className: CName) -> Bool`                      (`:11260`)
//   - `inkWidget.BwmsGetController()` — contorno do item `#87`, já provado ao vivo, mora em
//     `codeware-ui-inkwidget-ext.reds` (sobreviveu à quarentena).

public abstract class BwmsWidgetFind {

    // Teto de nós visitados. Árvore de UI real tem dezenas de nós; milhares indica ciclo ou
    // engano do chamador, e travar um frame de UI é pior do que devolver busca incompleta.
    public static func MaxNodes() -> Int32 = 4096

    // Todos os controllers de um tipo, na subárvore de `root` (inclusive o próprio `root`).
    // Devolve quantos achou. Considera tanto os controllers VANILLA do widget quanto o que foi
    // anexado pelo contorno do `#87` — um mod não deveria precisar saber por qual via o
    // controller chegou ali.
    public static func CollectByType(root: ref<inkWidget>, typeName: CName,
                                     out found: array<wref<inkLogicController>>) -> Int32 {
        ArrayClear(found);
        if !IsDefined(root) {
            return 0;
        }
        let pending: array<wref<inkWidget>>;
        ArrayPush(pending, root);
        let visited: Int32 = 0;

        while ArraySize(pending) > 0 && visited < BwmsWidgetFind.MaxNodes() {
            let last: Int32 = ArraySize(pending) - 1;
            let current: wref<inkWidget> = pending[last];
            ArrayErase(pending, last);
            visited += 1;
            // redscript não tem `continue` (achado registrado 2026-08-21) — guarda invertida.
            if IsDefined(current) {
                BwmsWidgetFind.CollectFromWidget(current, typeName, found);

                // Empilha os filhos, se for container.
                let container: ref<inkCompoundWidget> = current as inkCompoundWidget;
                if IsDefined(container) {
                    let count: Int32 = container.GetNumChildren();
                    let i: Int32 = 0;
                    while i < count {
                        let child: wref<inkWidget> = container.GetWidgetByIndex(i);
                        if IsDefined(child) {
                            ArrayPush(pending, child);
                        }
                        i += 1;
                    }
                }
            }
        }
        return ArraySize(found);
    }

    // O primeiro que casar, sem montar a lista inteira — o caso comum ("me dá o controller X").
    public static func FirstByType(root: ref<inkWidget>, typeName: CName) -> wref<inkLogicController> {
        let all: array<wref<inkLogicController>>;
        if BwmsWidgetFind.CollectByType(root, typeName, all) > 0 {
            return all[0];
        }
        let empty: wref<inkLogicController>;
        return empty;
    }

    // `target` está na subárvore de `root`? Substitui honestamente o antigo `InWindowTree`:
    // mesma pergunta ("este widget está montado dentro daquilo?"), só que respondida de cima
    // para baixo, que é o que a engine permite.
    public static func IsUnder(root: ref<inkWidget>, target: ref<inkWidget>) -> Bool {
        if !IsDefined(root) || !IsDefined(target) {
            return false;
        }
        let pending: array<wref<inkWidget>>;
        ArrayPush(pending, root);
        let visited: Int32 = 0;

        while ArraySize(pending) > 0 && visited < BwmsWidgetFind.MaxNodes() {
            let last: Int32 = ArraySize(pending) - 1;
            let current: wref<inkWidget> = pending[last];
            ArrayErase(pending, last);
            visited += 1;
            if IsDefined(current) {
                if current == target {
                    return true;
                }
                let container: ref<inkCompoundWidget> = current as inkCompoundWidget;
                if IsDefined(container) {
                    let count: Int32 = container.GetNumChildren();
                    let i: Int32 = 0;
                    while i < count {
                        let child: wref<inkWidget> = container.GetWidgetByIndex(i);
                        if IsDefined(child) {
                            ArrayPush(pending, child);
                        }
                        i += 1;
                    }
                }
            }
        }
        return false;
    }

    // Quantos nós a subárvore tem — diagnóstico barato pra mod que suspeita ter montado UI demais.
    public static func CountNodes(root: ref<inkWidget>) -> Int32 {
        if !IsDefined(root) {
            return 0;
        }
        let pending: array<wref<inkWidget>>;
        ArrayPush(pending, root);
        let visited: Int32 = 0;
        while ArraySize(pending) > 0 && visited < BwmsWidgetFind.MaxNodes() {
            let last: Int32 = ArraySize(pending) - 1;
            let current: wref<inkWidget> = pending[last];
            ArrayErase(pending, last);
            visited += 1;
            let container: ref<inkCompoundWidget> = current as inkCompoundWidget;
            if IsDefined(container) {
                let count: Int32 = container.GetNumChildren();
                let i: Int32 = 0;
                while i < count {
                    let child: wref<inkWidget> = container.GetWidgetByIndex(i);
                    if IsDefined(child) {
                        ArrayPush(pending, child);
                    }
                    i += 1;
                }
            }
        }
        return visited;
    }

    // Junta, para UM widget, os controllers vanilla e o anexado pelo contorno do `#87`.
    private static func CollectFromWidget(widget: wref<inkWidget>, typeName: CName,
                                          out found: array<wref<inkLogicController>>) -> Void {
        let vanilla: array<wref<inkLogicController>> = widget.GetControllers();
        for ctrl in vanilla {
            if IsDefined(ctrl) && ctrl.IsA(typeName) && !ArrayContains(found, ctrl) {
                ArrayPush(found, ctrl);
            }
        }
        let attached: ref<inkLogicController> = widget.BwmsGetController();
        if IsDefined(attached) && attached.IsA(typeName) {
            let asWeak: wref<inkLogicController> = attached;
            if !ArrayContains(found, asWeak) {
                ArrayPush(found, asWeak);
            }
        }
    }
}
