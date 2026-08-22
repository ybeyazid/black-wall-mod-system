// codeware-inkcharacterevent.reds — Codeware `UI/Core/inkCharacterEvent.reds` (item #92,
// catálogo exaustivo Codeware, round 10, 2026-08-12). `inkCharacterEvent` é CONFIRMADO
// VANILLA REAL (`redscript-src/orphans.script:51585`, `public final importonly class
// inkCharacterEvent extends inkInputEvent`, cadeia até `IScriptable` confirmada) — não é
// forge, `@addMethod` puro sobre tipo JÁ existente na RTTI do jogo (mesma categoria de risco
// BAIXO de `IPlacedComponent`/`ComponentWrapper`/`ComponentTarget`, já usada dezenas de
// vezes neste projeto).
//
// Layout confirmado pelo header GERADO da reflection real do jogo (`RED4ext.SDK/include/
// RED4ext/Scripting/Natives/inkCharacterEvent.hpp`, `RED4EXT_ASSERT_SIZE(CharacterEvent,
// 0x98)` — mesma categoria de confiança máxima que já fechou `#132`/`#135` hoje):
// `ink::CharacterEvent extends ink::InputEvent` (parent ocupa 0x90 bytes) +
// `action: EInputAction@+0x90`(u32) + `character: char@+0x94`(u8, fora do escopo — o `.reds`
// real não declara `GetCharacter`) + `type: inkCharacterEventType@+0x95`(u8). Campo de DADO
// puro, sem shift Itanium (regra já estabelecida dezenas de vezes neste catálogo — só
// métodos de VTABLE precisam do +0x08).
//
// Achado honesto (não escondido): a fonte C++ vendorizada real do Codeware
// (`inkCharacterEventEx.hpp`) só tem `RTTI_EXPAND_CLASS(Red::inkCharacterEvent, {
// RTTI_GETTER(action); })` — expõe OFICIALMENTE só `action`. O `.reds` real declara TAMBÉM
// `GetType() -> inkCharacterEventType` como `native func`, mas não há `RTTI_GETTER(type)`
// correspondente nesta cópia vendorizada (drift de versão do próprio projeto Codeware
// open-source, não erro nosso) — como o campo `type@+0x95` é confirmado real pelo header
// GERADO (maior confiança que a fonte `-Ex.hpp`), implementamos os DOIS mesmo assim: cobre
// o contrato do `.reds` por completo, capacidade PRÓPRIA do BWMS onde a cópia oficial
// vendorizada aparenta ter uma lacuna.
//
// ⚠️ REGRA DE OURO: nada abaixo foi testado ao vivo — offsets confirmados por header oficial
// GERADO, nunca contra memória viva desta build Mac. Item `#92` continua REAL_GAP até um
// boot confirmar. `character@+0x94` NÃO exposto (fora do contrato original do `.reds` real).

native func BwmsInkCharacterEventGetAction(evt: ref<IScriptable>) -> EInputAction
native func BwmsInkCharacterEventGetType(evt: ref<IScriptable>) -> inkCharacterEventType
// Fixture de teste — bypassa a restrição `importonly` do REDSCRIPT (trava de compile-time
// do `scc`, sem efeito no RTTI/C++ real) construindo a instância pelo lado Rust
// (`rtti::new_object`, mesma receita já usada por `BwmsMakeTestInkWidgetSpawnEvent`/#135).
native func BwmsMakeTestInkCharacterEvent(action: EInputAction, type: inkCharacterEventType) -> ref<inkCharacterEvent>

@addMethod(inkCharacterEvent)
public func GetAction() -> EInputAction {
    return BwmsInkCharacterEventGetAction(this);
}

@addMethod(inkCharacterEvent)
public func GetType() -> inkCharacterEventType {
    return BwmsInkCharacterEventGetType(this);
}
