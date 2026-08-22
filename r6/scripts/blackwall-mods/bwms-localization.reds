// bwms-localization.reds — localização de mod com variante por gênero (catálogo Codeware
//
// CORREÇÃO 2026-08-22 (crash real, 2 boots): guardar `array<ref<T>>` num `ScriptableSystem` faz o
// motor tratar esses objetos como posse DELE. No world-swap do autocontinue (`phase=-1`) ele
// destrói o sistema e tenta liberar o que está dentro — e crashou em
// `red::memory::Free<PoolEngine>` com null deref, na GameThread, 2 vezes seguidas.
// Referência FRACA (`wref`) resolve na raiz: o sistema passa a observar, nunca a possuir. Quem
// criou o objeto continua dono dele — que é a semântica correta de qualquer jeito.
// #34/#35/#36/#38/#40), REESCRITO em 2026-08-22 depois que a versão anterior saiu na auditoria
// de originalidade. Cinco itens do catálogo dependem só deste arquivo.
//
// O PROBLEMA REAL: português, espanhol, francês, polonês e russo flexionam adjetivo e particípio
// por gênero. "Você está pronto/pronta" não é detalhe cosmético — é a diferença entre um mod que
// parece parte do jogo e um que parece tradução automática. O jogo resolve isso para o texto
// dele; um mod não tinha como.
//
// COMO ESTE ARQUIVO RESOLVE: o mod registra um provider (uma classe que ele escreve, com as
// traduções dele) e pede texto por chave. Quem decide a variante é o SISTEMA, consultando o
// gênero real do jogador pela via vanilla (`GameObject.GetResolvedGenderName()`,
// `redscript-all.reds:19373`) — o mod nunca precisa saber que gênero é, nem checar.
//
// DECISÕES QUE SÃO NOSSAS:
//   1. **Fallback em cascata, nunca string vazia.** Pede-se a variante de gênero; não havendo,
//      cai no texto neutro; não havendo, devolve a PRÓPRIA CHAVE. Um rótulo escrito
//      "mod_menu_title" na tela é feio, mas é diagnosticável em um segundo. Rótulo vazio manda
//      o autor do mod caçar um widget invisível.
//   2. **Vários providers coexistem**, e o último registrado para uma chave vence. Dois mods
//      traduzindo coisas diferentes não brigam; um mod que quer sobrescrever outro consegue.
//   3. **`ScriptableSystem`**, não singleton improvisado: o motor instancia e gerencia sozinho,
//      padrão já provado neste projeto várias vezes.
//
// DEPENDÊNCIAS confirmadas por grep ANTES de escrever (`cp77-symbols/redscript-all.reds`):
//   - `class ScriptableSystem extends IScriptableSystem`                       (`:46413`)
//   - `GameInstance.GetScriptableSystemsContainer(self)`                       (`:14122`)
//   - `ScriptableSystemsContainer.Get(name) -> ref<ScriptableSystem>`          (`:238244`)
//   - `GameObject.GetResolvedGenderName() -> CName`                            (`:19373`)
//   - `GameInstance.GetPlayerSystem(...).GetLocalPlayerControlledGameObject()` (uso vanilla real)

// O mod estende esta classe e devolve as traduções dele.
//
// Só `GetText` é obrigatório. Quem não flexiona por gênero (inglês, na maior parte) não escreve
// `GetTextForGender` e nada quebra — o sistema cai no neutro sozinho.
public abstract class BwmsLocProvider extends IScriptable {

    // Identifica o provider nos logs e no desempate. Sobrescreva com algo estável.
    public func GetProviderName() -> CName {
        return n"unnamed";
    }

    // Texto neutro. Devolver "" significa "não tenho essa chave" — o sistema tenta o próximo.
    public func GetText(key: CName) -> String {
        return "";
    }

    // Variante por gênero. `gender` chega como o CName que o jogo usa (`Male`/`Female`).
    // O padrão delega ao neutro, então implementar isto é opcional.
    public func GetTextForGender(key: CName, gender: CName) -> String {
        return this.GetText(key);
    }
}

public class BwmsLocalization extends ScriptableSystem {

    private let m_providers: array<wref<BwmsLocProvider>>;

    // --- Acesso ------------------------------------------------------------------------------

    public static func Get(game: GameInstance) -> ref<BwmsLocalization> {
        let container: ref<ScriptableSystemsContainer> = GameInstance.GetScriptableSystemsContainer(game);
        if !IsDefined(container) {
            return null;
        }
        return container.Get(n"BwmsLocalization") as BwmsLocalization;
    }

    // --- Registro ----------------------------------------------------------------------------

    // Último a registrar tem precedência — ver decisão 2 no topo.
    public func Register(provider: wref<BwmsLocProvider>) -> Bool {
        if !IsDefined(provider) {
            return false;
        }
        if ArrayContains(this.m_providers, provider) {
            return false;
        }
        ArrayPush(this.m_providers, provider);
        return true;
    }

    public func Unregister(provider: wref<BwmsLocProvider>) -> Bool {
        if !IsDefined(provider) {
            return false;
        }
        let i: Int32 = ArraySize(this.m_providers) - 1;
        while i >= 0 {
            if this.m_providers[i] == provider {
                ArrayErase(this.m_providers, i);
                return true;
            }
            i -= 1;
        }
        return false;
    }

    public func ProviderCount() -> Int32 {
        return ArraySize(this.m_providers);
    }

    // --- Resolução ---------------------------------------------------------------------------

    // O caminho que um mod usa 99% do tempo: pede a chave, recebe o texto certo para o gênero
    // do jogador desta partida.
    public func GetText(key: CName) -> String {
        return this.GetTextForGender(key, this.CurrentGender());
    }

    // Cascata explícita: variante de gênero -> neutro -> a própria chave. Ver decisão 1.
    public func GetTextForGender(key: CName, gender: CName) -> String {
        let i: Int32 = ArraySize(this.m_providers) - 1;
        while i >= 0 {
            let provider: wref<BwmsLocProvider> = this.m_providers[i];
            if IsDefined(provider) {
                let gendered: String = provider.GetTextForGender(key, gender);
                if StrLen(gendered) > 0 {
                    return gendered;
                }
                let neutral: String = provider.GetText(key);
                if StrLen(neutral) > 0 {
                    return neutral;
                }
            }
            i -= 1;
        }
        return NameToString(key);
    }

    // `true` se alguém sabe traduzir a chave — deixa o mod escolher outro caminho sem depender
    // de comparar contra o texto de fallback.
    public func HasText(key: CName) -> Bool {
        let gender: CName = this.CurrentGender();
        let i: Int32 = ArraySize(this.m_providers) - 1;
        while i >= 0 {
            let provider: wref<BwmsLocProvider> = this.m_providers[i];
            if IsDefined(provider) {
                if StrLen(provider.GetTextForGender(key, gender)) > 0 {
                    return true;
                }
                if StrLen(provider.GetText(key)) > 0 {
                    return true;
                }
            }
            i -= 1;
        }
        return false;
    }

    // O gênero real do V desta partida, pela via vanilla. `n"None"` quando o player ainda não
    // existe (menu, boot) — os providers tratam isso como "use o neutro".
    public func CurrentGender() -> CName {
        // `GetGameInstance()` é `protected const` no `ScriptableSystem` — chamada sem `this.`,
        // que é a forma que o próprio código do jogo usa.
        let game: GameInstance = GetGameInstance();
        let player: ref<GameObject> = GameInstance.GetPlayerSystem(game)
            .GetLocalPlayerControlledGameObject();
        // `GetResolvedGenderName` mora em `gamePuppet` (`redscript-all.reds:19373`), não em
        // `GameObject` — o próprio jogo faz este cast antes de perguntar (`:64000`).
        let puppet: ref<PlayerPuppet> = player as PlayerPuppet;
        if !IsDefined(puppet) {
            return n"None";
        }
        return puppet.GetResolvedGenderName();
    }
}
