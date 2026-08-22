// BWMS — TweakDBBatch (item TweakXL #45, 2026-08-08).
//
// Devolvida por `TweakDBManager.StartBatch()`. Fonte real: `enablers/TweakXL/src/App/Tweaks/
// Executable/Scriptable/ScriptBatch.hpp` — classe redscript `"TweakDBBatch"`, instância que
// acumula chamadas e aplica tudo no `Commit()`.
//
// SIMPLIFICAÇÃO CONSCIENTE (mesma já documentada pro lado C++ do `Batch`, item TweakXL #9,
// EQUIVALENTE): o BWMS não tem staging — cada método aqui já aplica IMEDIATAMENTE (reusa os
// MESMOS handlers Rust de `TweakDBManager`, que são stateless e não leem `self`). `Commit()` é
// um no-op: não há nada pra "aplicar depois", já foi tudo aplicado na hora de cada chamada.
// Efeito observável final é o mesmo de um batch real tudo-ou-nada só no caso feliz (sem erro no
// meio) — se um método no meio da cadeia falhar, os anteriores JÁ foram aplicados (diferente do
// `TweakDBBatch` real, que só aplica no Commit). Documentado aqui pra não virar surpresa.
//
// STATUS (2026-08-08): ✅ PROVADO AO VIVO (`tweakdbbatch-smoke.reds`, boot real):
//   StartBatch() -> 0x70182170d0    (instância REAL, IsDefined()==true)
//   batch.SetFlat(...) -> true      (chamada de método de INSTÂNCIA na classe forjada, com sucesso)
//   batch.Commit()                  (chamado sem crash)
// Mesma receita 100% provada de `CallbackSystemHandler` (`register_type_instantiable_with_parent`
// + `rtti::new_object`/`rtti::make_handle` pro retorno de instância nova).
public native class TweakDBBatch {
  public native func SetFlat(id: TweakDBID, value: Variant) -> Bool;
  public native func CreateRecord(id: TweakDBID, type: CName) -> Bool;
  public native func CloneRecord(id: TweakDBID, base: TweakDBID) -> Bool;
  public native func UpdateRecord(id: TweakDBID) -> Bool;
  public native func RegisterEnum(id: TweakDBID) -> Bool;
  public native func RegisterName(name: CName) -> Bool;
  public native func Commit() -> Void;

  // TweakXL #34 (2026-08-10): `TweakChangeset::ReinheritFlat` real NÃO é redscript-callable no
  // TweakXL oficial (só usado internamente por YamlReader/RedReader ao parsear `.yaml`/`.tweak`)
  // — exposto aqui como método PRÓPRIO do BWMS (mesma capacidade: reponta `flatId` (já existente
  // no clone) pro offset de `sourceId + "." + appendix`). `flatId`/`sourceId` são TweakDBID de
  // verdade (não string) — `appendix` é o nome do campo SEM o ponto (ex. "damage", não ".damage").
  public native func ReinheritFlat(flatId: TweakDBID, sourceId: TweakDBID, appendix: String) -> Bool;

  // TweakXL #23 (2026-08-10): `RegisterExtraFlat` real NÃO é redscript-callable no TweakXL
  // oficial (só usado por `MetadataImporter.cpp`, import de metadata) — exposto aqui como
  // método PRÓPRIO do BWMS. `flatId`/`donorTypeFlat` são TweakDBID PRONTOS (`TDBID.Create(...)`,
  // resolvido em compile-time pelo scc) — `donorTypeFlat` é o TweakDBID de um flat JÁ EXISTENTE
  // do MESMO tipo que `value` (empresta a vtable nativa). Cria um flat GENUINAMENTE NOVO
  // (storage próprio, não compartilhado) — falha se `flatId` já existir (use SetFlat pra isso).
  public native func AddFlat(flatId: TweakDBID, donorTypeFlat: TweakDBID, value: Variant) -> Bool;
}
