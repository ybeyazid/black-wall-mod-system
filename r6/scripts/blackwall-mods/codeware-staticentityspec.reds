// -----------------------------------------------------------------------------
// Codeware.World.StaticEntitySpec — item `#76` do catálogo exaustivo — IMPLEMENTAÇÃO NOVA 2026-08-15 (rodada 9)
// -----------------------------------------------------------------------------
//
// Companion struct do `#75`/`StaticEntitySystem` (`codeware-staticentitysystem.reds`, FECHADO nesta
// mesma sessão) — a nota do catálogo dizia literalmente "companion struct do #75, nunca tocado, 0%".
// Confirmado por grep antes de codar: zero ocorrência de "StaticEntitySpec" em `register.rs`/
// `cp77-console/src/`/`blackwall-mods-dev/*.reds`/`bwms-core/` até esta rodada. Fonte real lida por
// completo agora: `enablers/Codeware/src/App/World/StaticEntitySpec.hpp` (33 linhas) — struct de dado
// PURO (`struct StaticEntitySpec : Red::IScriptable`), ZERO método/native func, só 6 `RTTI_PROPERTY`:
//
//   Red::RaRef<>              templatePath;
//   Red::CName                appearanceName;
//   Red::Vector4               position;
//   Red::Quaternion            orientation;
//   Red::DynArray<Red::CName> tags;
//   bool                      attached;
//
// Receita: EXATAMENTE a de `CallbackSystemHandler` (já provada, `codeware-*` correspondente) —
// `extends IScriptable` direto, INSTANCIÁVEL, estado por-instância num registry Rust keyed pelo
// ponteiro `this` (ver `register.rs`, "Codeware `#76`"). Zero endereço nativo — é 100% dado nosso,
// construído do zero (não há C++ real que popule isto no Mac; o único consumidor real, `SpawnEntity`,
// precisa de `EntitySpawner`, ausente no BWMS, já fora de escopo pro `#75` irmão).
//
// `templatePath` tipado como `ResRef` (não `RaRef<T>`/`ResourceAsyncReference`, tipo nunca
// declarado) — mesma divergência já aceita pro `#63`/Casts.reds. `tags` exposto via idioma
// "AddTag + Count()+ByIndex(i)" (evita array-retorno de propósito, mesma disciplina anti-array já
// estabelecida pro `#73`/`#75`/`ReflectionClass`).
//
// 2026-08-15, rodada 10: `position`(Vector4)/`orientation`(Quaternion) IMPLEMENTADOS — a rodada
// anterior tinha deixado de fora por "marshalling de PARÂMETRO struct de 16 bytes nunca exercitado
// nesta codebase". Investigação desta rodada achou que a máquina genérica de leitura de param
// (`type_inst_alloc`+dispatch de opcode, já usada e provada pra CString/Variant) já é size-agnostic
// — só faltava capturar os 16 bytes. Testado ISOLADO primeiro via `BwmsVector4RoundTrip` (ver
// `codeware-vector4-roundtrip-smoke.reds`, native global solta, zero dependência desta classe).

public native class StaticEntitySpec extends IScriptable {
    public static native func Create() -> ref<StaticEntitySpec>

    public native func SetAppearanceName(name: CName) -> Bool
    public native func GetAppearanceName() -> CName

    public native func SetTemplatePath(path: ResRef) -> Bool
    public native func GetTemplatePath() -> ResRef

    public native func SetAttached(attached: Bool) -> Bool
    public native func GetAttached() -> Bool

    public native func AddTag(tag: CName) -> Bool
    public native func GetTagCount() -> Int32
    public native func GetTagByIndex(index: Int32) -> CName

    public native func SetPosition(v: Vector4) -> Bool
    public native func GetPosition() -> Vector4

    public native func SetOrientation(q: Quaternion) -> Bool
    public native func GetOrientation() -> Quaternion
}
