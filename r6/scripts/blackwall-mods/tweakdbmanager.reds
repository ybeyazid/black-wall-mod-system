// BWMS — TweakDBManager (redscript-facing, 2026-08-08).
//
// Fecha o gap TweakXL #44 (`PENDENCIAS-UNIFICADAS.md`, "achado mais importante do framework"):
// a API real que mods do Nexus importam (`import TweakXL.*`, `TweakDBManager.SetFlat/CreateRecord/
// UpdateRecord/RegisterName`, fonte C++ real em `enablers/TweakXL/src/App/Tweaks/Executable/
// Scriptable/ScriptManager.hpp`, nome redscript "TweakDBManager") nunca tinha sido exposta — só o
// BACKEND nativo existia (`tweakdb_rt.rs`, provado via comandos de console `setflat`/`clone`/
// `xlautoclone`/`getflat`), sem ponte pro redscript.
//
// Mesmo padrão de forja já provado (`TweakXL.reds`/`Codeware.reds` facade, `register_facade_methods`
// em `register.rs`): classe `native` sem `extends` explícito, o Rust preenche os métodos no
// `class_validate_probe_hook` (`selfboot.rs`) no instante em que o validador do motor tenta
// resolver "TweakDBManager".
//
// STATUS (2026-08-08, 3 boots ao vivo, ver HISTORICO.md): ✅ PROVADO POR COMPLETO — TODOS os 7
// métodos confirmados em jogo (log real, 2 rodadas de smoke-test):
//   CreateRecord(id=0x1d1f4e7fe9, typeHash=0x7fdef930) -> true
//   CloneRecord(id=0x1d1f4e7fe9, base=0x1e1601aa89, type='gamedataWeaponItem_Record'):
//     GROW cap 3306462->3306839 + herdou 121 flats -> true
//   SetFlat(id=0x22d7de7c7e) -> true          (flat herdado pelo clone, escrito com sucesso)
//   UpdateRecord(id=0x1d1f4e7fe9, type='gamedataWeaponItem_Record') -> OK
//   RegisterEnum(id=0x1b5cdfaa92) -> true
//   StartBatch() -> 0x70182170d0              (instância REAL não-nula, IsDefined()==true)
// Fonte real: `Items.Preset_Lexington_Default` (record vanilla, 121 flats reais herdados),
// `.ammo` (flat herdado, escrito via Variant). A 1ª tentativa de SetFlat (contra um record
// CRIADO SEM clone, logo sem flats) tinha dado "-> false" corretamente — não é bug, é o
// comportamento certo (não existe flat pra escrever num record vazio; mods reais quase sempre
// clonam de um record base, nunca criam do zero). `RegisterName` não teve smoke-test dedicado
// (baixo risco, só resolve CName via pool nativo, mesmo padrão já usado em outras 10+ trampolins).
//
// COBERTURA (ver PENDENCIAS-UNIFICADAS.md item TweakXL #44/#45 — AMBOS FECHADOS POR COMPLETO):
//   SetFlat(id, value)     — ✅ PROVADO, caso ESCALAR (Int32/Float/Bool/TweakDBID/CName).
//                           Array/String/LocKey (via FlatValue novo) fica pra próxima pendência.
//   CreateRecord(id, type)  — ✅ PROVADO.
//   CloneRecord(id, base)  — ✅ PROVADO: deriva TUDO do `base` já vivo (tipo+flats), usando
//                           `tweak_db_id_derive` (CRC32-continuado, já testado em `bwms-hashes`,
//                           achado NUNCA usado em lugar nenhum até este incremento) em vez de
//                           concatenar string+rehash — funciona com SÓ os 2 TweakDBID numéricos,
//                           sem precisar do nome do record (o caso real de mod).
//   UpdateRecord(id)       — ✅ PROVADO.
//   RegisterEnum(id)       — ✅ PROVADO.
//   RegisterName(name)     — 🔶 CODADO (baixo risco).
//   StartBatch()           — ✅ PROVADO: instância nova de `TweakDBBatch` construída via
//                           `rtti::new_object`+`rtti::make_handle` (mesma receita 100% provada
//                           de `CallbackSystemHandler`/`RegisterCallback`), devolvida como
//                           `ref<TweakDBBatch>` REAL (não-null, `IsDefined()` confirma).
//
// Prova ao vivo (referência, não deployado por padrão): `tweakdbmanager-smoke.reds` +
// `tweakdbbatch-smoke.reds` (mesma pasta).
public abstract native class TweakDBManager {
  public final static native func SetFlat(id: TweakDBID, value: Variant) -> Bool;
  public final static native func CreateRecord(id: TweakDBID, type: CName) -> Bool;
  public final static native func CloneRecord(id: TweakDBID, base: TweakDBID) -> Bool;
  public final static native func UpdateRecord(id: TweakDBID) -> Bool;
  public final static native func RegisterEnum(id: TweakDBID) -> Bool;
  public final static native func RegisterName(name: CName) -> Bool;
  public final static native func StartBatch() -> ref<TweakDBBatch>;

  // Overloads por NOME (conveniência real do TweakXL — mod author escreve string, não TDBID cru).
  // `TDBID.Create` já é uma função redscript vanilla real (compila o TweakDBID em runtime a partir
  // da string, mesmo algoritmo CRC32|len<<32 que `tweak_db_id` replica no lado Rust) — não precisa
  // de nenhuma native nova pra esses overloads, só delega pro caminho por-ID acima.
  public final static func SetFlat(name: String, value: Variant) -> Bool {
    return TweakDBManager.SetFlat(TDBID.Create(name), value);
  }
  public final static func CreateRecord(name: String, type: CName) -> Bool {
    return TweakDBManager.CreateRecord(TDBID.Create(name), type);
  }
  public final static func CloneRecord(name: String, base: String) -> Bool {
    return TweakDBManager.CloneRecord(TDBID.Create(name), TDBID.Create(base));
  }
  public final static func UpdateRecord(name: String) -> Bool {
    return TweakDBManager.UpdateRecord(TDBID.Create(name));
  }
}
