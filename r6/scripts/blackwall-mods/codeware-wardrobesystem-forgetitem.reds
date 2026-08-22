// codeware-wardrobesystem-forgetitem.reds — Codeware `Player/WardrobeSystem.reds`:
// `@addMethod(WardrobeSystem) public native func ForgetItemID(itemID: ItemID) -> Bool`
// (catálogo item #46/#208, candidato barato, 2026-08-11, 3ª rodada de mineração da sessão).
//
// A fonte real (`WardrobeSystemEx::ForgetItemID`, `App/Player/WardrobeSystemEx.hpp`) faz 2
// coisas: (1) deriva `appearanceName` via `GetFlatValue<CName>({itemID.tdbid,
// ".appearanceName"})` e tenta remover por essa chave direto; (2) se isso falhar, varre TODO o
// `HashMap<CName,ItemID>` (`store.ForEach`) comparando `tdbid` e remove TODAS as chaves que
// baterem.
//
// DIVERGÊNCIA DE ESCOPO CONSCIENTE: em vez de replicar as 2 etapas (a 1ª exige compor uma
// derivação de TweakDBID + leitura de flat tipado CName, mais uma superfície de risco pra pouco
// ganho), uso só a 2ª — o SCAN por `tdbid` — como via ÚNICA e PRIMÁRIA. É estritamente mais
// ROBUSTO que a via 1 (não depende do item ter um flat `.appearanceName` populado no TweakDB) e
// reusa 100% a mecânica já provada ao vivo contra o save real do usuário (`wardrobe_forget_item`,
// 2026-07-31, HashMap layout confirmado por RE dedicada + leitura ao vivo batendo exata, 246
// itens). Ver `cp77-console/src/selftest.rs::wardrobe_forget_by_tdbid` pro algoritmo completo
// (offset `ItemID.tdbid@node+0x10`, ground-truth `RED4ext.SDK/include/RED4ext/NativeTypes.hpp`,
// `RED4EXT_ASSERT_SIZE(ItemID, 0x10)`).
//
// `ItemID.GetTDBID(itemID) -> TweakDBID` e `TDBID.ToNumber(tweakDBID) -> Uint64` são NATIVAS
// VANILLA REAIS já usadas em outros pontos do BWMS (`orphans.script:18613`, item Codeware `#69`
// já fechado) — zero declaração nova necessária pra elas.
native func BwmsWardrobeForgetByTDBID(sys: ref<IScriptable>, tdbid: Uint64) -> Int32

@addMethod(WardrobeSystem)
public func ForgetItemID(itemID: ItemID) -> Bool {
    let tdbid: TweakDBID = ItemID.GetTDBID(itemID);
    // Lido em LOCAL antes de comparar (2026-08-21): a comparação INLINE do retorno de um native é
    // a forma quebrada documentada desde 2026-07-15. Este caminho JÁ foi provado ao vivo em
    // 2026-08-01 (`removido=true`), então aqui é correção PREVENTIVA — comportamento idêntico,
    // sem depender de a forma frágil continuar funcionando por acaso.
    let n: Int32 = BwmsWardrobeForgetByTDBID(this, TDBID.ToNumber(tdbid));
    return n > 0;
}

// Sugar de conveniência: a via de scan não precisa de um ItemID completo, só do TweakDBID — útil
// pra quem já tem o TDBID e quer evitar montar um ItemID sintético só pra chamar ForgetItemID.
@addMethod(WardrobeSystem)
public func BwmsForgetByTDBID(tdbid: TweakDBID) -> Int32 {
    return BwmsWardrobeForgetByTDBID(this, TDBID.ToNumber(tdbid));
}
