// archivexl-44-mappin-cooked-count.reds — ArchiveXL `JournalExtension` (catálogo item #44,
// 2026-08-14) — varredura sistemática dos 86 acessores `GameInstance.GetXSystem`/`GetXManager`
// aplicada pela 1ª vez ao ArchiveXL (a mesma técnica já tinha esgotado o terreno no Codeware hoje
// mais cedo, zero match novo lá). Achado real: `Extension.hpp` real do ArchiveXL
// (`App/Extensions/Journal/Extension.hpp:33-35`) mostra que 3 hooks nunca tentados do item #44
// (`OnMappinDataLoaded`/`OnGetMappinData`/`OnGetPoiData`) recebem `aMappinSystem` como parâmetro
// — a MESMA classe `MappinSystem` já provada acessível neste projeto desde 2026-08-11
// (`GameInstance.GetMappinSystem`, item #78/`GetAllMappins`) e reusada pelo Codeware
// `#142`/`#232` (`codeware-mappinsystem.reds`, `SetPoiMappinPhase`/`GetPoiMappinPhase`).
//
// Escopo HONESTO: expõe só LEITURA — quantos mappins/multi-mappins/POIs o `MappinSystem` já tem
// CACHEADOS agora (`CookedMappinResource@0x58`/`CookedPoiResource@0x68`, campos de DADO, offset
// confirmado pelo header GERADO da reflection real do jogo — ver nota grande em
// `cp77-console/src/register.rs::register_mappinsystem_cooked_count_natives`). NÃO implementa a
// capacidade de escrita/injeção que é o coração real do item #44 (precisaria hookar
// `MappinSystem::GetMappinData`/`GetPoiData`, ambos `RawFunc`/`AddressLib`, RE de endereço Mac
// nunca feita — mesma categoria bloqueada do `#81`/WeatherSystem). Item #44 PERMANECE REAL_GAP —
// isto é uma sub-capacidade nova, honesta, zero RE de endereço.
//
// Retorno `Int32`: `-1` = recurso ainda não cacheado / MappinSystem inválido (sentinela seguro,
// NUNCA confundir com `0` = cacheado mas vazio). `-1` é o caso ESPERADO em muitos momentos do
// jogo (o cache só popula sob demanda) — não é erro nem crash, só ausência de dado ainda.

native func BwmsMappinSystemCookedMappinCount(sys: ref<MappinSystem>) -> Int32;
native func BwmsMappinSystemCookedMultiMappinCount(sys: ref<MappinSystem>) -> Int32;
native func BwmsMappinSystemCookedPoiCount(sys: ref<MappinSystem>) -> Int32;

// Só LEITURA em todos os 3 casos (nunca muta o MappinSystem/os recursos cacheados) — seguro pra
// chamar automaticamente em qualquer momento do gameplay, sem gate/aval manual.
@addMethod(MappinSystem)
public func GetCookedMappinCount() -> Int32 {
    return BwmsMappinSystemCookedMappinCount(this);
}

@addMethod(MappinSystem)
public func GetCookedMultiMappinCount() -> Int32 {
    return BwmsMappinSystemCookedMultiMappinCount(this);
}

@addMethod(MappinSystem)
public func GetCookedPoiCount() -> Int32 {
    return BwmsMappinSystemCookedPoiCount(this);
}
