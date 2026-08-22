# BWMS — API pra quem escreve mods

**O BWMS não é o Codeware/TweakXL/ArchiveXL/CET portados pro Mac — é um runtime próprio, nativo (Rust + ARM64 + Metal, zero emulação), que resolve os MESMOS problemas que essas ferramentas resolvem no Windows, do jeito que fizer mais sentido aqui.** Duas regras diferentes, dependendo de onde a API vem:

- **Vem do jogo de verdade (CD Projekt Red)** — classes/métodos vanilla (`PlayerPuppet`, `GameInstance`, etc.), o `native func`/`@wrapMethod`/`@addMethod` do redscript em cima delas: **nome bate exatamente igual ao Windows, sem exceção.** É o contrato real do jogo, não uma escolha nossa.
- **Vem de um framework de terceiros (Codeware, TweakXL, ArchiveXL, CET)** — essas são invenções de outros modders, não da CDPR. O BWMS reimplementa o que elas fazem, com **nome e forma próprios**, sem obrigação de replicar bug/decisão estranha da versão original. Se você está migrando um mod Windows que dependia de uma dessas, esta doc tem a seção "De Codeware/TweakXL pro BWMS" abaixo — curta, só o essencial pra trocar a chamada.

## Como usar (funções utilitárias próprias do BWMS)

Nome próprio, prefixo `Bwms` (2026-08-07: renomeado do nome Codeware original — ver critério no topo desta doc). Exceção deliberada: `Print`/`ModLog` mantêm o nome original porque são usados internamente em vários `.reds` do próprio BWMS e são utilitário genérico de debug, não API distintiva de framework de terceiro.

No seu `.reds`, declare a função com o nome BWMS e chame normalmente. Não precisa de `import` nem de nenhuma configuração — o runtime do BWMS já deixa essas funções registradas antes do seu mod carregar.

```swift
// no seu mod (qualquer arquivo .reds em r6/scripts/seu-mod/)
native func BwmsBitTest32(value: Uint32, bit: Int32) -> Bool
native func BwmsFNV1a64(data: String, opt seed: Uint64) -> Uint64
native func BwmsParseInt32(str: String, opt base: Int32) -> Int32
native func BwmsUTF8StrLeft(str: String, length: Int32) -> String

public func Exemplo() -> Void {
  let ligado = BwmsBitTest32(5u, 0); // true (bit 0 de 0b101)
  let hash = BwmsFNV1a64("hello", 0xCBF29CE484222325ul); // 0xa430d84680aabd0b
  let numero = BwmsParseInt32("42", 10); // 42
  let inicio = BwmsUTF8StrLeft("café com leite", 4); // "café"
}
```

## Funções disponíveis hoje

### Bitwise em 8/16/32/64 bits
`BwmsBitTest8/16/32/64(value, bit) -> Bool` · `BwmsBitSet8/16/32/64(value, bit, state) -> UintN` · `BwmsBitShiftL8/16/32/64(value, bits) -> UintN` · `BwmsBitShiftR8/16/32/64(value, bits) -> UintN`

### Hashing
- `BwmsFNV1a64(data: String, opt seed: Uint64) -> Uint64` (seed padrão `0xCBF29CE484222325`)
- `BwmsFNV1a32(data: String, opt seed: Uint32) -> Uint32` (seed padrão `0x811C9DC5`)
- `BwmsMurmur3(data: String, opt seed: Uint32) -> Uint32` (seed padrão `0x5EEDBA5E`)

### Parsing de string pra número
`BwmsParseInt8/16/32/64(str: String, opt base: Int32) -> IntN` · `BwmsParseUint8/16/32/64(str: String, opt base: Int32) -> UintN`
Base `0` auto-detecta prefixo (`0x`=hex, `0`=octal); string com lixo sobrando (espaço, letra) retorna `0`.

### String UTF-8-aware
- `BwmsUTF8StrLen(str: String) -> Int32` — conta caracteres (não bytes; acentos/emoji contam como 1)
- `BwmsUTF8StrLeft/Right(str: String, length: Int32) -> String`
- `BwmsUTF8StrMid(str: String, offset: Int32, length: Int32) -> String`
- `BwmsUTF8StrLower/Upper(str: String) -> String`
- **Limite atual:** o texto de RETORNO precisa ter menos de 20 bytes. Pra strings maiores o mecanismo de escrita ainda não existe — evite usar essas 5 funções com textos longos por enquanto (leitura de string como ARGUMENTO não tem esse limite).

### Conversão hash↔tipo
`BwmsHashToName(value: Uint64) -> CName` · `BwmsNameToHash(value: CName) -> Uint64` · `BwmsHashToCRUID(value: Uint64) -> CRUID` · `BwmsCRUIDToHash(value: CRUID) -> Uint64` · `BwmsHashToNodeRef(value: Uint64) -> NodeRef` · `BwmsNodeRefToHash(value: NodeRef) -> Uint64`

### Logging (nome original mantido, ver nota acima)
`Print(text: String) -> Void` · `ModLog(mod: CName, text: String) -> Void`
Escrevem no log de desenvolvimento do BWMS (não na UI do CET) — útil pra depurar seu mod.

## Ainda não disponível (não declare, vai travar o bind do seu mod)
- `CreateNodeRef` (precisa do algoritmo de hash de path-de-cena do motor, ainda não mapeado)
- `GameFileExists`
- `CreateLocalizationString` / `ExtractLocalizationString`
- Módulos inteiros do Codeware sem equivalente BWMS ainda: `Depot`, `Device`, `Engine`, `Mesh`, `Physics`, `Player`, `Quest`, `Vehicle` (exceto o caso pontual documentado abaixo)
- `Codeware.UI.inkCustomController` completo (existe uma versão própria mais simples — ver abaixo)

Se seu mod declara uma dessas (ou qualquer outra `native func`/`native class` que o BWMS não registrou), o carregamento do script vai falhar — **remova a declaração** ou espere uma atualização do BWMS.

## De Codeware/TweakXL (Windows) pro BWMS

Se você está adaptando um mod que dependia de uma dessas chamadas, troque como abaixo. **Não são apelidos — são reimplementações próprias**, então o comportamento pode variar em detalhe fino (marcado onde relevante).

**`GameInstance.GetCallbackSystem(game)` → `BwmsGetCallbackSystem()`**
Sem parâmetro `game` (o BWMS resolve o singleton sozinho). `RegisterCallback`/`UnregisterCallback`/`RegisterEvent`/`DispatchEventAs` funcionam igual depois disso. Testado e estável.
```swift
// Windows: let cbs = GameInstance.GetCallbackSystem(game);
let cbs = BwmsGetCallbackSystem();
cbs.RegisterCallback(n"MeuEvento", this, n"OnMeuEvento");
```

**`GameInstance.GetScriptableServiceContainer(game)` → `BwmsGetScriptableServiceContainer()`**
Mesma ideia, sem `game`. `GetService(name)` devolve instância cacheada por nome (round-trip testado).
```swift
// Windows: let svc = GameInstance.GetScriptableServiceContainer(game).GetService(n"MeuServico");
let svc = BwmsGetScriptableServiceContainer().GetService(n"MeuServico");
```

**`Reflection.GetClass(name)` → `BwmsReflGetClass(name)`; `.GetProperty(name)`/`.GetPropertyByIndex(i)` → `BwmsReflClassGetProperty`/`BwmsReflClassGetPropertyByIndex`**
Cobertura hoje é mais estreita que o Reflection real: leitura/escrita só de campos `Float`/`Bool` tipados (`BwmsReflPropGetValueFloat`/`SetValueFloat`, idem `Bool`); `Variant`/`String`/enum genérico ainda não. `ReflectionType`/`ReflectionEnum`/`ReflectionBitfield` (navegação de tipo completa) ainda não existem — se seu mod precisa disso, ainda não dá pra migrar.
```swift
let cls = BwmsReflGetClass(n"PlayerPuppet");
let prop = BwmsReflClassGetProperty(cls, n"health");
let valor = BwmsReflPropGetValueFloat(objetoAlvo, prop);
```

**`Codeware.UI.inkCustomController` → `codeware-ui-customcontroller.reds` (versão própria, mais simples)**
Cobre criar `inkText`/`inkCompoundWidget` anexado a um painel real via `inkCompoundRef.AddChild`. **Não é testado em boot ainda** (marcado no próprio arquivo-fonte) — se seu mod precisa de popup/input de texto/múltiplos controllers por widget, espere, essa peça ainda não está pronta pra depender dela.

**`VehicleSystem.ToggleGarageVehicle`/`WardrobeSystem.ForgetItemID`** — mecanismo interno já provado (o BWMS sabe fazer isso), mas **ainda não exposto como `native func` pro seu mod chamar**. Se seu mod precisa disso hoje, ainda não dá — é próximo passo natural (só falta a "porta de entrada" redscript, o motor já funciona).

**`TweakDBManager.SetFlat`/`CreateRecord`/`TweakDBInterface.GetRecords` (ler/listar registros por tipo)** — a parte de ESCREVER (`SetFlat`/`CreateRecord`) funciona internamente mas hoje só via comando de canal de dev, não native pro seu mod. A parte de LER-EM-LOTE (`GetRecords`/`GetRecordCount`) não existe ainda de forma nenhuma. `TweakXL.Version()`/`Require(...)`/`Reload()` (a fachada de controle) **essas sim já são nativas reais, mesmo nome do Windows** — pode chamar direto.

## Adicionando um cheat na aba "Cheats" do BWMS

A aba Cheats (Mods → Cheats, 100% redscript, sem CET/Lua/ConfigVar) tem um **ponto de extensão único**: você adiciona 1 entrada com `label` + um objeto de **comportamento** (`BWMSCheatHandler`), sem precisar tocar em nenhum arquivo do BWMS nem entender o switch interno dos cheats nativos.

### O contrato

```swift
// bwms-settings-poc.reds já declara isso — não precisa redeclarar, só herdar:
public abstract class BWMSCheatHandler {
  public func BWMSOnToggle(pp: ref<PlayerPuppet>, game: GameInstance) -> Void {}
  public func BWMSOnQuery(pp: ref<PlayerPuppet>, game: GameInstance) -> Bool { return false; }
}
```

- `BWMSOnQuery` — devolve `true` se o efeito está LIGADO agora (decide o desenho do toggle na UI).
- `BWMSOnToggle` — roda quando o usuário clica; aplica ou remove o efeito.

**Importante:** o handler é um objeto SEM ESTADO PRÓPRIO — ele é reconstruído toda vez que a aba abre. Guarde o estado do SEU efeito num campo em `PlayerPuppet` (`@addField`), nunca numa variável do próprio handler.

### Exemplo completo, copiável (arquivo `.reds` próprio, fora do BWMS)

```swift
// seu-mod.reds — adiciona 1 cheat com efeito real, sem tocar em nenhum arquivo do BWMS.
native func Print(text: String) -> Void;  // se seu mod ainda não declarou

@addField(PlayerPuppet) let m_meuBoostDeVida: ref<gameStatModifierData>;

public class MeuCheatDeVida extends BWMSCheatHandler {
  public func BWMSOnQuery(pp: ref<PlayerPuppet>, game: GameInstance) -> Bool {
    return IsDefined(pp.m_meuBoostDeVida);
  }
  public func BWMSOnToggle(pp: ref<PlayerPuppet>, game: GameInstance) -> Void {
    let ss: ref<StatsSystem> = GameInstance.GetStatsSystem(game);
    let soid: StatsObjectID = Cast<StatsObjectID>(pp.GetEntityID());
    if IsDefined(pp.m_meuBoostDeVida) {
      ss.RemoveModifier(soid, pp.m_meuBoostDeVida);
      pp.m_meuBoostDeVida = null;
    } else {
      pp.m_meuBoostDeVida = RPGManager.CreateStatModifier(gamedataStatType.BonusHealth, gameStatModifierType.Additive, 500.0);
      ss.AddModifier(soid, pp.m_meuBoostDeVida);
    };
  }
}

// PONTO DE EXTENSÃO: @wrapMethod em BWMSCheats() só pra somar SEU cheat na lista — não
// precisa @wrapMethod em BWMSRun/BWMSIsOn nem tocar no switch interno dos cheats nativos.
@wrapMethod(SettingsMainGameController)
public func BWMSCheats() -> array<BWMSCheatDef> {
  let base: array<BWMSCheatDef> = wrappedMethod();
  ArrayPush(base, this.BWMSDefH("Meu cheat: +500 de vida", new MeuCheatDeVida()));
  return base;
}
```

Coloque esse arquivo em `r6/scripts/seu-mod/seu-mod.reds` (diretório PRÓPRIO, separado de `blackwall-mods/`), compile com `scc -compile r6/scripts` — seu cheat aparece na aba Cheats junto com os do BWMS, com efeito real, sem editar nada do BWMS.

`BWMSDefH(label, handler, opt kind)` — `kind` opcional: `0` (default) = toggle liga/desliga; `1` = ação de disparo único (clique executa e não fica "ligado").

### Testado

Este exato exemplo (`examples/thirdparty-cheat-handler-test.reds`) compila limpo contra o bundle do BWMS e foi validado em boot real via o mecanismo de diagnóstico (`BWMSOnQuery`/`BWMSOnToggle` chamados pela via virtual, efeito real de `StatModifier` aplicado/removido) — 2026-07-15.
