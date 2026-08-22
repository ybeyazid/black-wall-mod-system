// codeware-mappinsystem.reds — Codeware `World/MappinSystem.reds` (catálogo item #78,
// `MappinSystem.GetAllMappins()`, 2026-08-11, sessão de candidatos baratos).
//
// DIVERGÊNCIA DE ESCOPO CONSCIENTE (documentada, mesmo padrão já usado dezenas de vezes neste
// projeto): a fonte real do Codeware (`src/App/World/MappinSystemEx.hpp`) implementa isto lendo
// direto os campos C++ privados `MappinSystem::MappinsLock@0x198`/`MappinsData@0x1A0`
// (`SortedArray<MappinData>`, item de struct `{id:u64, instance:Handle<IMappin>}`) — offset de
// CAMPO confirmado no header vendorizado, mas o LAYOUT EXATO do elemento dentro do
// `SortedArray` (stride/alinhamento) nunca foi confirmado contra o Mac, então ler cru ali seria
// chute, não composição.
//
// Em vez disso, ESTA implementação compõe 3 chamadas ao NATIVO VANILLA REAL já existente
// (`MappinSystem.GetMappins(targetType, out mappins)`, confirmado em
// `cp77-symbols/redscript-src/orphans.script`, classe `MappinSystem extends IMappinSystem`) —
// uma por cada valor do enum `gamemappinsMappinTargetType` (`World`/`Minimap`/`Map`) — e funde o
// resultado, deduplicando por identidade de handle (`ArrayContains`). Mesma capacidade prática
// (enumerar TODOS os mappins ativos, não só um alcance), zero campo cru, zero endereço nativo
// novo, zero forge de classe.
//
// Dependências confirmadas via grep ANTES de escrever (`cp77-symbols/redscript-src/orphans.script`):
//   - `GameInstance.GetMappinSystem(self: GameInstance) -> ref<MappinSystem>` — native vanilla real.
//   - `MappinSystem.GetMappins(targetType: gamemappinsMappinTargetType, out mappins: [ref<IMappin>])`
//     — native vanilla real (`public final native func`).
//   - `enum gamemappinsMappinTargetType { World = 0, Minimap = 1, Map = 2 }` — vanilla real.
//   - `ArrayContains`/`ArrayPush`/`ArraySize` — intrínsecos do compilador, já usados em produção
//     (ex. `codeware-ui-custompopup.reds`).

@addMethod(MappinSystem)
private final func BwmsCollectMappins(targetType: gamemappinsMappinTargetType, all: array<ref<IMappin>>) -> array<ref<IMappin>> {
    let batch: array<ref<IMappin>>;
    this.GetMappins(targetType, batch);

    let i: Int32 = 0;
    while i < ArraySize(batch) {
        if !ArrayContains(all, batch[i]) {
            ArrayPush(all, batch[i]);
        }
        i += 1;
    }

    return all;
}

@addMethod(MappinSystem)
public func GetAllMappins() -> array<ref<IMappin>> {
    let all: array<ref<IMappin>>;
    all = this.BwmsCollectMappins(gamemappinsMappinTargetType.World, all);
    all = this.BwmsCollectMappins(gamemappinsMappinTargetType.Minimap, all);
    all = this.BwmsCollectMappins(gamemappinsMappinTargetType.Map, all);
    return all;
}

// ---------------------------------------------------------------------------------------------
// Codeware `#142` (`OpenWorldTracker`), mecanismo confirmado pelo item `#232` do catálogo
// exaustivo (2026-08-12) — `MappinSystem.hpp` real:
//
//   namespace Raw::MappinSystem {
//   constexpr auto SetPoiMappinPhase = Core::RawVFunc</* addr = */ 0x1D0,
//       void(const uint32_t aJournalPath, const gamedataMappinPhase aMappinPhase)>();
//   constexpr auto GetPoiMappinPhase = Core::RawVFunc</* addr = */ 0x3C8,
//       gamedataMappinPhase(const uint32_t aJournalPath)>();
//   }
//
// São slots de VTABLE de `MappinSystem`/`IMappinSystem` (offset Windows já em BYTES) — não
// `RawFunc`/`AddressLib`, então não precisam de RE de endereço Mac nova, só do shift Itanium
// +0x08 já estabelecido neste projeto pra vtable de classe C++ real (`DATABASE.md`: "vtable
// macOS = +0x08 vs Windows, 2 dtors Itanium"). Mac `SetPoiMappinPhase`=`0x1D0+0x08=0x1D8`;
// Mac `GetPoiMappinPhase`=`0x3C8+0x08=0x3D0`. Composição em `register.rs`
// (`register_mappinsystem_poi_phase_natives`/`mappinsystem_vtbl_fn`): lê o 1º qword do
// objeto (vtable), aplica o offset Mac, lê o slot como function pointer e chama — validando
// `gum::is_readable` em cada ponteiro antes de dereferenciar (objeto/vtable/slot/fnptr).
//
// `gamedataMappinPhase` (`orphans.script:1328`, enum RTTI-visível real: CompletedPhase=0/
// DefaultPhase=1/DiscoveredPhase=2/UndiscoveredPhase=3/Count=4/Invalid=5) cruza a fronteira
// redscript<->native como `Int32` puro (`EnumInt`/`IntEnum<T>`, idioma já usado em dezenas de
// lugares deste projeto — ex. `codeware-districtresolver-smoke.reds`).
// ---------------------------------------------------------------------------------------------

native func BwmsMappinSystemSetPoiPhase(sys: ref<MappinSystem>, journalPath: Uint32, phase: Int32) -> Void;
native func BwmsMappinSystemGetPoiPhase(sys: ref<MappinSystem>, journalPath: Uint32) -> Int32;

// SetPoiMappinPhase MUTA estado real de mappin de quest — só a composição fica pronta aqui;
// NUNCA chamado automaticamente por nenhum smoke test deste projeto (ver
// codeware-mappinsystem-smoke.reds, comentário explícito lá).
@addMethod(MappinSystem)
public func SetPoiMappinPhase(journalPath: Uint32, phase: gamedataMappinPhase) -> Void {
    BwmsMappinSystemSetPoiPhase(this, journalPath, EnumInt(phase));
}

// GetPoiMappinPhase é só LEITURA — seguro pra chamar com um journalPath fabricado (pior caso:
// devolve Invalid/Count, nunca muta nada).
@addMethod(MappinSystem)
public func GetPoiMappinPhase(journalPath: Uint32) -> gamedataMappinPhase {
    return IntEnum<gamedataMappinPhase>(BwmsMappinSystemGetPoiPhase(this, journalPath));
}
