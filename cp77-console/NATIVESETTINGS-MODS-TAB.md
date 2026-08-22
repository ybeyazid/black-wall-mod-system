# NativeSettings no BWMS — o que foi modificado + como a aba Mods funciona

> Referência pra NÃO reinventar o fix da aba Mods. O `init.lua` do NativeSettings é o **1.96 stock** do justarandomguy (port macOS), com **UMA** adição BWMS.

## A ÚNICA modificação BWMS vs o NativeSettings 1.96 (diff = 9 linhas)

Dentro do `onInit`, logo após os dois `Observe(... "OnMenuItemActivated")` stock:

```lua
-- Blackwall.sys compat: OnMenuItemActivated é cb func de evento ink (RegisterToCallback
-- 'OnItemActivated' -> CallCustomCallback NATIVO), não passa pelo executor de script, então o
-- hook por CName não o pega. HandleMenuItemActivate é função scripted COMUM, chamada
-- script->script logo depois (this.HandleMenuItemActivate(data)) com o MESMO data -> cai
-- no executor (igual AddMenuItem) e é interceptável. Seta fromMods do mesmo jeito.
Observe("gameuiMenuItemListGameController", "HandleMenuItemActivate", function (_, data)
    nativeSettings.fromMods = data.label == "Mods"
end)
```

**Por quê:** o runtime BWMS intercepta funções pelo executor de script (hook por CName). `OnMenuItemActivated` é **cb func** (callback de evento ink, despacho NATIVO) → **não passa pelo executor** → o hook não pega. `HandleMenuItemActivate` é função scripted comum, chamada script→script logo depois com o MESMO `data` → **passa pelo executor → interceptável**. É ela que seta `fromMods` no macOS. (Cron.lua/EventProxy/Ref/UIButton são stock, headers do psiberx intactos.)

## Flow completo da aba Mods

1. **Cria o botão "Mods":** `Observe("gameuiMenuItemListGameController", "AddMenuItem")` — quando o jogo adiciona o item com `spawnEvent.value == "OnSwitchToSettings"` (o botão Configurações), empurra um SEGUNDO item `label="Mods"`, mesmo eventName/action (`PauseMenuAction.OpenSubMenu`). (+ `SingleplayerMenuGameController.OnTooltipContainerSpawned` pro menu principal inicial.)
2. **Detecta o clique → `fromMods`:** stock usa `OnMenuItemActivated` (PauseMenu + gameui); **o BWMS usa `HandleMenuItemActivate`** (acima). `fromMods = (label == "Mods")`.
3. **Renderiza a página Mods quando `fromMods==true`** — Overrides em `SettingsMainGameController`:
   - `OnInitialize` (esconde botões + pega ref do `settingsOptionsList`)
   - `PopulateCategories` (abas) · `PopulateSettingsData` (abas dos mods) · `PopulateCategorySettingsOptions` (opções)
   - `RequestClose` (fechar) · `SettingsSelectorController{Bool,Int,Float,ListString}` (widgets)

## O que o runtime PRECISA suportar (senão a aba abre vazia/normal)

**30 hooks:** 12 `Observe` + 2 `ObserveAfter` + **16 `Override`**.
Os 16 `Override` são **wrapped** (substituem comportamento e chamam `wrapped()` só quando NÃO é mods). `PopulateSettingsData`/`PopulateCategories`/`PopulateCategorySettingsOptions` são os que DESENHAM a página — se o mecanismo `Override(wrapped)` do runtime falhar nesses, a aba não renderiza.

## Estado — onde funciona e onde quebra

- **Menu principal:** `HandleMenuItemActivate` dispara → `fromMods` seta → página Mods aparece. ✓ caminho coberto pelo fix.
- **Menu de pausa (in-game):** o clique vai por `PauseMenuGameController` e **NÃO** dispara `HandleMenuItemActivate` (descoberta via ring: clique → `IsAction`/`DoesActionMatch` → spawn das settings, sem evento de item interceptável) → `fromMods` fica falso → abre **Configurações normais**. ✗ **caminho EM ABERTO.**

**Se está apanhando pra mostrar a aba Mods, cheque nesta ordem:**
1. Está testando pela **PAUSA** (não resolvido) ou pelo **MENU PRINCIPAL** (coberto)? Use o menu principal.
2. O runtime intercepta os **16 Override wrapped**, em especial os `Populate*`? (esses desenham a página.)
3. O `HandleMenuItemActivate` está sendo interceptado (executor) e setando `fromMods`?
