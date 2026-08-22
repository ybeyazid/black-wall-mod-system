// archivexl-puppetstatesystem.reds — ArchiveXL `App::PuppetStateSystem` (item #32, 2026-08-11,
// cont.192). `extends ScriptableSystem` (mesmo padrão de forge seguro já provado 5x+ nesta
// sessão — `LocalizationSystem`/`PlayerGenderWatcher`/`CustomPopupManager`/etc). Zero native
// novo. Cópia fiel do COMPORTAMENTO real (lido em `System.cpp`+`Extension.cpp` por completo):
//
//   GetBodyTypeSuffix(...)  = PuppetStateExtension::GetBodyType(owner).ToString()
//   GetArmsStateSuffix(...) = switch(PuppetStateExtension::GetArmsState(owner)) { ... }
//   GetFeetStateSuffix(...) = switch(PuppetStateExtension::GetFeetState(owner)) { ... }
//
// `GetArmsState`/`GetFeetState` (Extension.cpp:120-140) só consultam `s_handlers`, um mapa
// populado EXCLUSIVAMENTE por `OnAttachPuppet`/`OnDetachPuppet` (hooks de
// `CharacterCustomizationGenitalsController::OnAttach`/`HairstyleController::OnDetach` +
// `PuppetStateHandler`/`RegisterSlotListener`, item #29 — RE nativa genuína, fora de escopo).
// SEM esses hooks (que este projeto não implementa), `s_handlers` é SEMPRE vazio — o C++ real
// devolve os defaults documentados: `GetArmsState`→`PuppetArmsState::BaseArms` (linha 126),
// `GetFeetState`→`PuppetFeetState::None` (linha 137). `GetBodyType` (linha 142-190) tem um fast
// path idêntico: `if (!aPuppet || s_bodyTags.empty()) return s_baseBodyType;` — `s_bodyTags` só é
// populado por `Configure()` a partir do `.xl` `player.bodyTypes` (`PuppetStateConfig`, já
// tipado em `bwms-core/src/xl.rs`, mas NUNCA wired num runtime consumido por este projeto) —
// logo SEMPRE vazio aqui, `GetBodyType` SEMPRE devolve `s_baseBodyType` = `"BaseBody"`
// (`BaseBodyName`, `Extension.hpp:18`).
//
// Com essas 2 precondições (handlers/bodyTags sempre vazios) confirmadas, os 3 métodos são
// DETERMINÍSTICOS e fielmente replicáveis sem nenhuma RE: `GetBodyTypeSuffix`→`"BaseBody"`,
// `GetArmsStateSuffix`→`"BaseArms"` (nome do enum member REAL, `RTTI_ENUM_NAME_STR`), `GetFeet
// StateSuffix`→`""` (o C++ real cai no `default: return "";` pois `None` não é um dos 4 cases
// explícitos de `PuppetFeetState`). Divergência DOCUMENTADA (não escondida): se `#29` (handlers)
// ou a fiação real do `.xl` `player.bodyTypes` forem implementados no futuro, este port precisa
// ser revisitado — hoje reflete fielmente o comportamento REAL do C++ nas precondições atuais
// deste projeto, não um chute arbitrário.

public class PuppetStateSystem extends ScriptableSystem {
    public func GetBodyTypeSuffix(itemID: ItemID, owner: wref<GameObject>, suffixRecord: ref<ItemsFactoryAppearanceSuffixBase_Record>) -> String {
        return "BaseBody";
    }

    public func GetArmsStateSuffix(itemID: ItemID, owner: wref<GameObject>, suffixRecord: ref<ItemsFactoryAppearanceSuffixBase_Record>) -> String {
        return "BaseArms";
    }

    public func GetFeetStateSuffix(itemID: ItemID, owner: wref<GameObject>, suffixRecord: ref<ItemsFactoryAppearanceSuffixBase_Record>) -> String {
        return "";
    }

    public static func GetInstance(game: GameInstance) -> ref<PuppetStateSystem> {
        return GameInstance.GetScriptableSystemsContainer(game).Get(n"PuppetStateSystem") as PuppetStateSystem;
    }
}
