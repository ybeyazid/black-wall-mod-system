// codeware-weathersystem.reds — Codeware `World/WeatherSystem.reds` (catálogo item #81/#140,
// 2026-08-12, agente dedicado Codeware, 100% offline, sem ambiente de boot disponível).
//
// DIVERGÊNCIA DE ESCOPO CONSCIENTE (documentada, mesmo padrão já usado dezenas de vezes neste
// projeto): a fonte real do Codeware (`src/App/World/WeatherSystemEx.hpp`/`.cpp`) implementa os
// 4 métodos originais (`SetWeather`/`ResetWeather`/`GetEnvironmentDefinition`/`GetWeatherState`)
// via `Raw::RuntimeSystemWeather::SetWeatherByName`/`SetWeatherByIndex`/`GetEnvironmentDefinition`/
// `GetWeatherState` — TODOS `Core::RawFunc` (hash `Red::AddressLib`, endereço Mac desconhecido,
// genuína RE não feita nesta rodada). Esses 4 métodos NÃO são implementados aqui — item #81/#140
// continua REAL_GAP pra troca de clima ativa.
//
// Em vez disso, esta implementação expõe uma capacidade MENOR mas 100% composable sem nenhum
// RawFunc: LER o estado de clima atual/anterior. `enablers/Codeware/src/Red/RuntimeScene.hpp`
// (header vendorizado real) mostra que, ao lado dos 4 RawFuncs, existem 4 campos de DADO puro
// (`Core::OffsetPtr`, não vtable/hash) dentro de `worldRuntimeSystemWeather`:
//
//   namespace Raw::WeatherScriptInterface { using System = Core::OffsetPtr<0x40,
//       Red::worldRuntimeSystemWeather*>; }               // dentro de WeatherSystem
//   namespace Raw::RuntimeSystemWeather {
//     using CycleWeather        = Core::OffsetPtr<0x8C, bool>;
//     using CurrentStateIndex   = Core::OffsetPtr<0x90, uint32_t>;
//     using CurrentSource       = Core::OffsetPtr<0x98, Red::CName>;
//     using PreviouseStateIndex = Core::OffsetPtr<0xB8, uint32_t>;
//   }
//
// Composição em `cp77-console/src/register.rs` (`register_weathersystem_state_natives`/
// `weathersystem_read_state`/`decode_weather_runtime_state`): segue o ponteiro
// `WeatherSystem+0x40 -> worldRuntimeSystemWeather*`, lê um chunk de 0xC0 bytes (1 syscall via
// `gum::read_chunk`) e decodifica os 3 campos — zero chamada de função em qualquer etapa (mais
// seguro até que o padrão de vtable-dispatch já usado pro `MappinSystem`/#142, que ao menos
// CHAMA um function pointer real; aqui só lemos bytes).
//
// Dependências confirmadas ANTES de escrever:
//   - `WeatherSystem extends IScriptable` — vanilla real, `cp77-symbols/redscript-src/
//     orphans.script:43653` (`importonly class WeatherSystem extends IScriptable {}`).
//   - `GameInstance.GetWeatherSystem(self: GameInstance) -> ref<WeatherSystem>` — native
//     vanilla real, `orphans.script:11547`.
//   - `@addMethod(WeatherSystem)` validado via scratch-compile contra o bundle real
//     (`redscript/dist/redscript-macos/engine/tools/scc`) — resolve limpo, zero UNRESOLVED_REF.

native func BwmsWeatherSystemGetCurrentStateIndex(sys: ref<WeatherSystem>) -> Uint32;
native func BwmsWeatherSystemGetCurrentSource(sys: ref<WeatherSystem>) -> CName;
native func BwmsWeatherSystemGetPreviousStateIndex(sys: ref<WeatherSystem>) -> Uint32;

// GetCurrentWeatherStateIndex()/GetPreviousWeatherStateIndex(): índice do estado de clima
// (dentro da tabela `worldEnvironmentDefinition`) atual/anterior. GetCurrentWeatherSource():
// CName da fonte que setou o clima atual (ex. nome da quest/área que forçou o clima, ou
// n"None"/hash-0 se nunca setado nesta sessão). Todos LEITURA PURA — seguro chamar a qualquer
// momento, nunca muta estado.
@addMethod(WeatherSystem)
public func GetCurrentWeatherStateIndex() -> Uint32 {
    return BwmsWeatherSystemGetCurrentStateIndex(this);
}

@addMethod(WeatherSystem)
public func GetCurrentWeatherSource() -> CName {
    return BwmsWeatherSystemGetCurrentSource(this);
}

@addMethod(WeatherSystem)
public func GetPreviousWeatherStateIndex() -> Uint32 {
    return BwmsWeatherSystemGetPreviousStateIndex(this);
}
