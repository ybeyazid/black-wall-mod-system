// axl-transmog-apply (2026-08-05, via RE): EquipmentSystemPlayerData::ChangeAppearanceToItem é o
// orquestrador REAL do fluxo "Wear" do Wardrobe (wardrobeUIController.script -> EquipWardrobeSetRequest
// -> EquipmentSystemPlayerData.EquipWardrobeSet -> EquipVisuals -> ChangeAppearanceToItem ->
// TransactionSystem.ChangeItemAppearanceByItemID, essa última já confirmada segura em 2026-08-03 mas
// nunca confirmada como o mecanismo real disparado pela UI). 100% redscript (não native, private final
// func comum) — zero vmaddr C++ necessário, @wrapMethod funciona mesmo em método private.
// Substitui a hipótese antiga AppearanceChanger::SelectAppearanceName (0x10370e5b0): RE nova confirmou
// que o único chamador dela no binário inteiro é GetSuffixes (família GetVisualTags, preset de UI),
// nunca alcançável a partir do fluxo real de troca de aparência — por isso nunca disparou no teste
// ao vivo de 2026-08-05 mesmo com o usuário trocando a aparência de uma peça de verdade.
@wrapMethod(EquipmentSystemPlayerData)
private final func ChangeAppearanceToItem(item: ItemID) -> Void {
  Print("[axl-transmog] ChangeAppearanceToItem item=" + TDBID.ToStringDEBUG(ItemID.GetTDBID(item)));
  wrappedMethod(item);
}
