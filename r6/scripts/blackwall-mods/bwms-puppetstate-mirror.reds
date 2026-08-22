// axl-puppet-state-apply: dispara MenuScenario_CharacterCustomizationMirror SEM depender do
// usuário caminhar até um espelho no mundo (world-nav) — tenta exercitar
// GenitalsController::OnAttach/HairstyleController::OnDetach (hooks já instalados em
// selftest.rs::install_postload_hooks, vmaddrs GENITAL_ONATTACH_VM/HAIRSTYLE_ONDETACH_VM).
//
// Achado (RE offline, redscript-src): SwitchToScenario é `public final native func` em
// inkMenuScenario (orphans.script:45541-45551) — chamável de QUALQUER instância viva de
// qualquer subclasse (não só a que está trocando). MenuScenario_Idle é a scenario ativa
// durante gameplay normal (fecha todos os menus/mostra HUD no OnEnterScenario dela,
// inGameScenarios.script:14-24) — sempre viva na autocontinue.
//
// EXPERIMENTAL: nenhum caller de "MenuScenario_CharacterCustomizationMirror" existe em
// redscript-src (o disparo normal vem de um device de espelho no mundo, C++/scene puro) —
// esse device provavelmente prepara estado de cena (scene resource do "character_customization_scenes")
// ANTES de despachar o switch. Pular essa preparação pode: (a) não fazer nada visível (scenario
// muda mas OnCCOPuppetReady nunca dispara por falta de puppet), ou (b) crashar. Ambos os
// resultados são informativos. Gate = marcador ~/.bwms-puppetstate-fire (opt-in, dev-only,
// consumido 1x por processo pela native).
native func BwmsPuppetStateFireOnce() -> Bool;

// FIX (2026-08-03, achado de RE dedicada nova): a via `SwitchToScenario(...Mirror)` acima é
// beco-sem-saída CONFIRMADO (soft-lock, `OnCCOPuppetReady` nunca chega sem o device físico real
// do espelho). Achado novo: `preGameScenarios.script:542-552` (`MenuScenario_CharacterCustomization`,
// usado na criação de personagem) NUNCA espera esse evento — só chama
// `this.GetMenusState().OpenMenu(n"player_puppet")` + `OpenMenu(n"character_customization", ...)`
// em sequência síncrona. `GetMenusState()`/`OpenMenu` são `native func` de `inkMenuScenario`
// (classe-base) — chamáveis de QUALQUER scenario vivo, incluindo `MenuScenario_Idle` (scenario
// normal de gameplay). Testando se isso contorna o device físico/`OnCCOPuppetReady` por completo.
@wrapMethod(MenuScenario_Idle)
protected cb func OnEnterScenario(prevScenario: CName, userData: ref<IScriptable>) -> Bool {
  let r: Bool = wrappedMethod(prevScenario, userData);
  if BwmsPuppetStateFireOnce() {
    Print("[axl-puppet] tentando OpenMenu(player_puppet)+OpenMenu(character_customization) DIRETO, sem Mirror/scene/world-nav");
    let menuState: wref<inkMenusState> = this.GetMenusState();
    menuState.OpenMenu(n"player_puppet");
    menuState.OpenMenu(n"character_customization", new MorphMenuUserData());
    Print("[axl-puppet] OpenMenu x2 retornou — zero crash até aqui");
  };
  return r;
}

// 2026-08-02: achado no código-fonte real (inGameScenarios.script) — `OnEnterScenario` do espelho
// abre `character_customization_scenes` (`OpenMenu`, que pausa o mundo por design, igual qualquer
// menu) e espera `OnCCOPuppetReady` — que só dispara se o DEVICE FÍSICO real do espelho tiver
// preparado a cena antes (SkipIntro pulou isso de propósito). Sem esse preparo, `OnCCOPuppetReady`
// nunca dispara → soft-lock permanente (achado ao vivo 2026-08-02, 5min sem tick nenhum, zero
// crash). Saída real do jogador (`OnAccept`/`OnCancel`) chama `this.SwitchToScenario(n"MenuScenario_Idle")`.
//
// 2026-08-02 (cont.): tentativa 1 (saída IMEDIATA, mesma call-chain síncrona) PROVOU escapar o
// soft-lock (zero crash, simulação volta a tickar) mas é rápida demais — zero disparo real
// confirmado de GenitalsController::OnAttach/HairstyleController::OnDetach (sem janela pro
// puppet-setup assíncrono do OpenMenu rodar). Tentativa 2 (esta): em vez de chamar
// SwitchToScenario aqui mesmo, arma a recuperação do lado Rust (`BwmsPuppetStateArmRecovery`,
// captura `this` + agenda ~180 ticks/3-6s depois via `cp77_tick`/`rtti::call_func` direto na
// instância) — dá uma janela de verdade antes da saída forçada.
native func BwmsPuppetStateArmRecovery(self: ref<IScriptable>) -> Bool;

// 2026-08-05: a recuperação atrasada só existe pra escapar o soft-lock do disparo SINTÉTICO
// (BwmsPuppetStateFireOnce pula o preparo do device físico, então OnCCOPuppetReady nunca chega).
// Um espelho REAL do mundo prepara o puppet de verdade — forçar a saída em ~3-6s ali só atrapalha
// (interrompe a cena antes do jogo terminar sozinho), então só arma quando o disparo foi o nosso.
native func BwmsPuppetStateShouldRecover() -> Bool;

@wrapMethod(MenuScenario_CharacterCustomizationMirror)
protected cb func OnEnterScenario(prevScenario: CName, userData: ref<IScriptable>) -> Bool {
  let r: Bool = wrappedMethod(prevScenario, userData);
  if BwmsPuppetStateShouldRecover() {
    Print("[axl-puppet] MenuScenario_CharacterCustomizationMirror.OnEnterScenario (disparo sintético) — armando recuperação ATRASADA (não sai na hora)");
    BwmsPuppetStateArmRecovery(this);
  } else {
    Print("[axl-puppet] MenuScenario_CharacterCustomizationMirror.OnEnterScenario (espelho real do mundo) — deixando o fluxo vanilla rodar, sem recuperação forçada");
  };
  return r;
}
