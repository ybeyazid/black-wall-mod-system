// -----------------------------------------------------------------------------
// Codeware.World.StaticEntitySystem — item `#75` do catálogo exaustivo — IMPLEMENTAÇÃO NOVA 2026-08-15
// -----------------------------------------------------------------------------
//
// Módulo IRMÃO de `DynamicEntitySystem` (`#73`, FECHADO desde 2026-08-06/2026-08-12) — a nota do
// catálogo dizia literalmente "MÓDULO PARALELO INTEIRO, 18 métodos, NUNCA mencionado em nenhuma sessão
// anterior do projeto... mesmo padrão do Dynamic". Confirmado por grep antes de codar: zero linha
// "StaticEntitySystem"/"tramp_ses_" existia em `register.rs`/`blackwall-mods-dev/*.reds` até esta
// rodada. Fonte real lida por completo agora: `enablers/Codeware/src/App/World/StaticEntitySystem.hpp`
// (assinaturas, 68 linhas) + `.cpp` (linhas 244-417, os 8 métodos de tag-bookkeeping) +
// `enablers/Codeware/scripts/World/StaticEntitySystem.reds` (18 métodos, declaração real).
//
// Mesmo subconjunto de risco BAIXO já aplicado ao `#73`: bookkeeping puro (registry Rust-side,
// `HashMap<EntityID-hash,Set<CName-hash>>` + espelho inverso, zero ponteiro de `Entity` real, zero
// endereço nativo/`RawFunc`). O C++ real (`IsManaged`/`IsTagged`/`AssignTag`/`UnassignTag`/
// `IsPopulated`/`GetTaggedID`/`GetTaggedIDs`, linhas 244-417 de `.cpp`) é literalmente
// `std::shared_lock`/`scoped_lock` sobre `Core::Map<EntityID,Set<CName>>` — o MESMO par de mapas já
// portado (e provado ao vivo) pro `#73`, só duplicado com registry PRÓPRIO (Static≠Dynamic são
// estados independentes na fonte real, nunca compartilhados).
//
// Divergência de escopo consciente (mesma linha de raciocínio já documentada pro `#73`, não repetida
// por completo aqui): `SpawnEntity`/`DespawnEntity`/`AttachEntity`/`DetachEntity`/`IsSpawned`/
// `IsSpawning`/`GetEntity` ficam FORA (precisam de `EntitySpawner`/`IDynamicEntityIDSystem` reais,
// ausentes no BWMS). `DespawnTagged`/`AttachTagged`/`DetachTagged` (bulk-lifecycle) ficam FORA pelo
// mesmo anti-padrão já corrigido em `#79`/`#138`/`#139` — dispararia efeito FALSO sem spawn real por
// trás. `GetTags`/`GetTagged` (retorno `array<T>`) ficam FORA — categoria que já crashou 2x no
// projeto sem donor real emprestado (TweakXL `GetRecordsArray`/2026-08-10). `IsManaged` segue a MESMA
// aproximação já usada pro `#73`: "já foi tagueada alguma vez" (a fonte real também checa `m_tokens`,
// que só populamos via spawn real — sempre vazio nesta implementação).
//
// `#75` NÃO tem `IsRestored` nem `DeleteTagged` na API real (diferença genuína vs `#73`, confirmado
// lendo o header — `StaticEntitySystem.hpp` não declara nenhum dos dois) — não inventados aqui.
// 5 getters escalares (idioma "Count()+ByIndex(i)") reusam a MESMA disciplina anti-array-retorno já
// estabelecida pro `#73`/round 8.

public native class StaticEntitySystem extends IGameSystem {
    public native func IsReady() -> Bool

    public native func IsManaged(id: EntityID) -> Bool
    public native func IsTagged(id: EntityID, tag: CName) -> Bool

    public native func IsPopulated(tag: CName) -> Bool
    public native func AssignTag(id: EntityID, tag: CName) -> Void
    public native func UnassignTag(id: EntityID, tag: CName) -> Void

    // Getters escalares (evita array-retorno de propósito, ver comentário acima).
    public native func GetTaggedID(tag: CName) -> EntityID
    public native func GetTaggedCount(tag: CName) -> Int32
    public native func GetTaggedIDByIndex(tag: CName, index: Int32) -> EntityID
    public native func GetTagCount(id: EntityID) -> Int32
    public native func GetTagByIndex(id: EntityID, index: Int32) -> CName
}

public static native func BwmsGetStaticEntitySystem() -> ref<StaticEntitySystem>
