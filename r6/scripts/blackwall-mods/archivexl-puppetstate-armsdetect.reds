// archivexl-puppetstate-armsdetect.reds — ArchiveXL item #29 (`PuppetStateHandler`), catálogo
// exaustivo. Sessão 2026-08-12, ArchiveXL round 4 (agente offline dedicado, 100% offline —
// nenhum arquivo do jogo tocado, nenhum boot lançado por esta rodada).
//
// CONTEXTO: `App::PuppetStateHandler : Red::AttachmentSlotsListener` (fonte real, `PuppetState/
// Handler.cpp`) reage a `OnItemEquippedComplete`/`OnItemUnequippedComplete` (eventos de EQUIP/
// UNEQUIP normal — capacidade DISTINTA de `PuppetStateExtension::OnAttachPuppet`/`OnDetachPuppet`
// já mapeados, item #30, que precisam do espelho físico real de customização) e resolve/atualiza
// `s_handlers[owner] = {ArmsState, FeetState}` — cache consultado por `PuppetStateSystem::
// GetArmsState`/`GetFeetState` (item #32, JÁ FECHADO E PROVADO AO VIVO em 2026-08-11, via
// `archivexl-puppetstatesystem.reds`). Como este projeto nunca implementa
// `PuppetStateHandler`/`RegisterSlotListener` (RE de endereço nativo genuína, fora de escopo —
// `AttachmentSlotsListener` é interface virtual C++, não `RawFunc` com hash), `s_handlers` fica
// SEMPRE vazio e `GetArmsStateSuffix` devolve o default fiel `"BaseArms"` incondicionalmente
// (comportamento já documentado e correto em `archivexl-puppetstatesystem.reds`).
//
// Achado desta rodada (auditoria sistemática 2026-08-12, checkpoint "auditoria... 20 REAL_GAP",
// Achado negativo #29: "zero hit em qualquer rastreador antigo... capacidade genuinamente nunca
// tentada"): a via "equip normal" nunca foi tentada via um mecanismo DIVERGENTE (poll sob-demanda
// em vez de cache atualizado por evento). `PuppetArmsState` (BaseArms/MantisBlades/Monowire/
// ProjectileLauncher) reflete qual cyberware de braço está DESEMBAINHADA no slot
// `AttachmentSlots.WeaponRight` — a mesma pergunta que a native VANILLA REAL já responde, sem
// NENHUM endereço/hook novo:
//
//   - `TransactionSystem.GetItemInSlot(obj, t"AttachmentSlots.WeaponRight") -> ref<ItemObject>`
//     (orphans.script:18167, já usada extensivamente neste projeto — `scanslots`/`scanspawn`).
//   - `WeaponObject.IsOfType(itemID, type: gamedataItemType) -> Bool` (weapon.script:583, 100%
//     redscript vanilla — `TweakDBInterface.GetWeaponItemRecord(...).ItemType().Type()`).
//   - `gamedataItemType.Cyb_MantisBlades`/`Cyb_NanoWires`(Monowire)/`Cyb_Launcher`
//     (ProjectileLauncher) — os 3 tipos REAIS de cyberware de braço do jogo, confirmados em uso
//     vanilla real (`prereqs.script::PlayerHasMantisBladesEquippedPrereq` compara a mesma slot
//     `AttachmentSlots.WeaponRight`; `weapon.script:1064` usa `Cyb_NanoWires`;
//     `leftHandCyberwareTransitions.script` usa o conceito de "Launcher"/`ProjectileLauncher`).
//
// Divergência CONSCIENTE, documentada (mesma categoria já usada pra fechar outros itens deste
// catálogo por via EQUIVALENTE, ex. #8/`SelectAppearanceName`↔`ChangeAppearanceToItem`):
// mecanismo DIFERENTE do C++ real (poll sob-demanda via API 100% vanilla, não hook de evento +
// cache), mesma resposta prática na esmagadora maioria dos casos (o slot WeaponRight só contém
// uma dessas 3 armas de cyberware QUANDO ela está desembainhada — o mesmo instante em que o
// C++ real dispararia `OnItemEquippedComplete` pra ela). Zero endereço nativo, zero hook, zero
// forge de RTTI — só composição de natives já provadas neste projeto.
//
// Item `#29` continua REAL_GAP (regra de ouro — nunca testado ao vivo nesta rodada, boot fora de
// escopo). `PuppetFeetState` (`None`/`Flat`/`Lifted`/`HighHeels`/`FlatShoes`) NÃO implementado
// aqui — achado negativo honesto: as tags "HighHeels"/"FlatShoes" (já usadas no item #21/#22,
// `builtin_tag_overrides`) são tags de OVERRIDE DE GARMENT (`.xl` `overrides.tags`, aplicadas
// por MOD, não dado intrínseco do item), não uma classificação vanilla do calçado — sem um
// runtime que resolva "quais tags visuais este item TEM" (o próprio `GetVisualTags`/
// `AppearanceNameVisualTagsPreset`, já catalogado noutro item mas nunca portado ao dylib), não
// há via vanilla equivalente pra Feet como há pra Arms. Documentado, não forçado.
//
// ACHADO DE PROCESSO (esta rodada): a 1ª tentativa declarou isto como `@addMethod(ArchiveXL)`
// (mesma classe Facade dos itens #20/#21/#22/#23/#48 já fechados hoje) — `scc -compile` do
// bundle inteiro FALHOU com `constant pool error: definition not found: 202281`, reproduzido de
// forma MÍNIMA e determinística (isolado num arquivo de 6 linhas, zero relação com o resto do
// bundle). Bisectado: o erro desaparece trocando só o `@addMethod(ArchiveXL) static func` por um
// método `static func` numa classe PRÓPRIA nova (mesmo padrão de `BwmsCastsSmoke`/
// `BwmsEntityAssembleDetachSmoke`) — ArchiveXL é `public abstract native class` com TODOS os
// membros `native` (nenhum @addMethod prévio nesta classe em todo o bundle, confirmado por
// grep); aparenta ser uma limitação/bug genuína do compilador `scc` ao adicionar método NOVO
// (não-native) a uma classe `abstract native` cujos membros são 100% nativos — categoria
// DIFERENTE do bug já documentado (`@addMethod` sobre `struct` não dá `this` por valor). Fix:
// implementado como classe própria (`ArchiveXLPuppetState`), sem tocar `ArchiveXL`. Lição nova
// pro projeto: `@addMethod` numa classe `abstract native` 100%-nativa pode quebrar o compile de
// forma NÃO-óbvia (erro de constant-pool interno, não um erro de tipo claro) — preferir classe
// própria quando a classe-alvo for desse formato.

// `PuppetArmsState` — nome fiel ao enum real do ArchiveXL (`PuppetState/Extension.hpp`), mas
// ordinais são escolha PRÓPRIA do BWMS (o tipo nunca existiu na RTTI do Mac, nenhuma colisão
// possível — confirmado por grep, zero declaração prévia deste nome em blackwall-mods-dev/).
public enum PuppetArmsState {
    BaseArms = 0,
    MantisBlades = 1,
    Monowire = 2,
    ProjectileLauncher = 3
}

public class ArchiveXLPuppetState {
    public static func ComputePuppetArmsState(puppet: wref<GameObject>) -> PuppetArmsState {
        if !IsDefined(puppet) {
            return PuppetArmsState.BaseArms;
        }
        let game: GameInstance = puppet.GetGame();
        let weapon: ref<WeaponObject> = GameInstance.GetTransactionSystem(game).GetItemInSlot(puppet, t"AttachmentSlots.WeaponRight") as WeaponObject;
        if !IsDefined(weapon) {
            return PuppetArmsState.BaseArms;
        }
        let itemID: ItemID = weapon.GetItemID();
        if WeaponObject.IsOfType(itemID, gamedataItemType.Cyb_MantisBlades) {
            return PuppetArmsState.MantisBlades;
        }
        if WeaponObject.IsOfType(itemID, gamedataItemType.Cyb_NanoWires) {
            return PuppetArmsState.Monowire;
        }
        if WeaponObject.IsOfType(itemID, gamedataItemType.Cyb_Launcher) {
            return PuppetArmsState.ProjectileLauncher;
        }
        return PuppetArmsState.BaseArms;
    }
}
