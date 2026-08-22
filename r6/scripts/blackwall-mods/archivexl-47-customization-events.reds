// ArchiveXL #47 (`CustomizationExtension`) — parte "OnAppearanceApplied/OnAppearanceSwitched/
// OnOptionUpdated": os 3 eventos NATIVOS REAIS do jogo
// (`gameuiCharacterCustomizationSystem_On{AppearanceApplied,AppearanceSwitched,OptionUpdated}Event`,
// `orphans.script:54181-54213`, confirmados em uso vanilla real dentro de
// `characterCreationBodyMorphMenu.script`) — encaminhados pro CallbackSystem do BWMS via
// @wrapMethod na classe VANILLA real (`characterCreationBodyMorphMenu extends
// BaseCharacterCreationController`), zero hook C++/RTTI novo, mesma receita segura já usada em
// dezenas de wraps deste projeto.
//
// Escopo honesto: `characterCreationBodyMorphMenu` NÃO é exclusiva da tela de criação de
// personagem (pregame) — é a MESMA tela que `MenuScenario_CharacterCustomizationMirror.
// OnCCOPuppetReady` abre no espelho real do mundo (confirmado por `bwms-customization-apply.reds`,
// já wrapping 3 métodos DIFERENTES da mesma classe: OnSliderChange/OnColorSelected/OnColorChange)
// — então este dispatch cobre pregame E o espelho de customização in-game, não só criação de
// personagem.
//
// Dispatch em 2 estágios, DELIBERADAMENTE (achado de segurança, ver `register.rs::
// tramp_customization_edge`): o wrap NÃO constrói/despacha o evento na hora — chama
// `BwmsCustomizationEdge(kind)`, um native que só marca um atomic (zero risco, mesmo shape do já
// provado `BwmsCaptureEs`/`BwmsCaptureReq`). O dispatch real (`CustomizationEvent` construído +
// `CallbackSystem` notificado) acontece no PRÓXIMO tick do BWMS (`cp77_tick::
// drain_customization_edges`, Rust puro) — evita despachar OUTRO evento (RTTI/`call_func`
// aninhado) de dentro de uma call-site que já é ela mesma uma chamada de bytecode redscript
// (a categoria de risco de reentrância que motivou o `ExecDepthGuard`, 2026-07-31). `kind`:
// 0=AppearanceApplied, 1=AppearanceSwitched, 2=OptionUpdated — os 3 eventos se distinguem pelo
// NOME de despacho no lado Rust, não por campo do objeto (mesmo padrão já usado por
// "Session/Start"/"Session/End" e "Entity/Attach").
//
// A fonte real do ArchiveXL cobre um subsistema bem maior de customização em runtime
// (`gameuiICharacterCustomizationSystem`, resolução de aparência dinâmica etc, ver item #20/#21
// do mesmo catálogo) — este arquivo cobre só os 3 pontos de notificação de evento, não o resto.
//
// CODADO, prova ao vivo pendente (regra de ouro: nunca fechar sem boot real que confirme
// `CallbackSystem.RegisterCallback(n"Customization/AppearanceApplied", ...)` disparando de
// verdade contra um mod real) — 2026-08-14.

public native class CustomizationEvent extends CallbackSystemEvent {}

public static native func BwmsCustomizationEdge(kind: Int32) -> Void

@wrapMethod(characterCreationBodyMorphMenu)
protected cb func OnAppearanceAppliedEvent(evt: ref<gameuiCharacterCustomizationSystem_OnAppearanceAppliedEvent>) -> Bool {
  let result: Bool = wrappedMethod(evt);
  BwmsCustomizationEdge(0);
  return result;
}

@wrapMethod(characterCreationBodyMorphMenu)
protected cb func OnAppearanceSwitched(evt: ref<gameuiCharacterCustomizationSystem_OnAppearanceSwitchedEvent>) -> Bool {
  let result: Bool = wrappedMethod(evt);
  BwmsCustomizationEdge(1);
  return result;
}

@wrapMethod(characterCreationBodyMorphMenu)
protected cb func OnOptionUpdated(evt: ref<gameuiCharacterCustomizationSystem_OnOptionUpdatedEvent>) -> Bool {
  let result: Bool = wrappedMethod(evt);
  BwmsCustomizationEdge(2);
  return result;
}
