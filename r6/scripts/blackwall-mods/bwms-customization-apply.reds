// axl-puppet-state-apply (2026-08-05, via RE): 1ª tentativa era @wrapMethod direto na interface
// nativa `gameuiICharacterCustomizationSystem::ApplyChangeToOption` — o scc recusa com
// "signature does not match any existing method" mesmo com a assinatura byte-a-byte idêntica ao
// bundle real (confirmado via redscript-cli decompile do final.redscripts atual); a classe é
// `abstract native` sem NENHUMA subclasse redscript-visível no bundle inteiro, e @wrapMethod não
// resolve nesse caso. Substituído por wrap direto nos 3 CALLERS reais (100% redscript, não
// abstract, não native) — são os ÚNICOS 3 call-sites de ApplyChangeToOption em todo o bundle
// (confirmado por grep no decompile completo): slider (estilo/hairstyle costuma usar esse
// controle) e os 2 handlers de color-picker. `characterCreationBodyMorphMenu` é a MESMA tela
// (`character_customization`) que `MenuScenario_CharacterCustomizationMirror.OnCCOPuppetReady`
// abre no espelho real do mundo — não é exclusiva da criação de personagem pregame.
@wrapMethod(characterCreationBodyMorphMenu)
protected cb func OnSliderChange(widget: wref<inkWidget>) -> Bool {
  Print("[axl-customization] OnSliderChange disparou (troca de opção via slider — cabelo/estilo costuma vir por aqui)");
  return wrappedMethod(widget);
}

@wrapMethod(characterCreationBodyMorphMenu)
protected cb func OnColorSelected(widget: wref<inkWidget>) -> Bool {
  Print("[axl-customization] OnColorSelected disparou (color-picker)");
  return wrappedMethod(widget);
}

@wrapMethod(characterCreationBodyMorphMenu)
protected cb func OnColorChange(widget: wref<inkWidget>) -> Bool {
  Print("[axl-customization] OnColorChange disparou (color-picker)");
  return wrappedMethod(widget);
}
