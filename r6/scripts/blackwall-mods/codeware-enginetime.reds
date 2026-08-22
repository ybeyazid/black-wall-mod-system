// codeware-enginetime.reds — Codeware `Engine/EngineTime.reds` (item #22 do catálogo,
// `PENDENCIAS-UNIFICADAS.md`, 2026-08-12, rodada de composição barata). Fonte real
// (`App/Engine/EngineTimeEx.hpp`): `FromTicks(t){ return {t}; }` / `ToTicks(t){ return t.ticks; }`
// — `EngineTime` é `importonly struct` VANILLA (real, já usado em dezenas de `.script` do jogo —
// `GameInstance.GetEngineTime()`, `EngineTime.ToFloat()`, etc.) achatado num único
// `ticks: Uint64` — mesma categoria "struct de 1 qword" já fechada pra CRUID/NodeRef/TweakDBID/
// EntityID/ResRef (`codeware-casts.reds`). Identidade pura, ZERO endereço nativo.
//
// `@addMethod(EngineTime)` no nome OFICIAL do Codeware (mesmo padrão já usado pra
// `TDBID.FromNumber`/`EntityID.FromHash` em `codeware-casts.reds`) — `EngineTime` sendo um struct
// VANILLA genuíno (não invenção do Codeware), a mesma técnica de `@addMethod(TDBID)` já provada
// deveria valer aqui igual.
//
// `GetFrequency()` (o 3º método real do item) FICA DE FORA de propósito — lê
// `*Raw::EngineTime::Frequency`, um `double` num endereço ESTÁTICO real do motor
// (`Red::AddressLib::EngineTime_Frequency`), genuína RE de endereço nativo novo, fora do escopo
// desta rodada (0 código, precisa de sessão de RE dedicada com boot).
//
// NÃO TESTADO AO VIVO NESTA RODADA (sessão 100% offline, boot pausado) — o cname interno real de
// `EngineTime` (o que `GetType()` devolve pro parâmetro de `ToTicks`) nunca foi confirmado; pelo
// padrão já achado 2x nesta mesma whitelist de leitura (`rtti.rs`, ver `EntityID`/`ResRef`), há
// uma chance real (não alta, mas não-zero) de o nome interno divergir do nome redscript-facing —
// se divergir, `BwmsEngineTimeToTicks` vai ler `0` silenciosamente em vez de crashar (mesmo
// comportamento seguro/fail-safe já estabelecido nos outros casos). `BwmsTicksToEngineTime`
// (direção CONTRÁRIA, valor de RETORNO) não depende dessa whitelist — usa `write_uint_ret` puro,
// sem risco de divergência de nome.
native func BwmsTicksToEngineTime(ticks: Uint64) -> EngineTime
native func BwmsEngineTimeToTicks(value: EngineTime) -> Uint64

@addMethod(EngineTime)
public static func FromTicks(value: Uint64) -> EngineTime = BwmsTicksToEngineTime(value)

@addMethod(EngineTime)
public static func ToTicks(self: EngineTime) -> Uint64 = BwmsEngineTimeToTicks(self)

// `GetFrequency()` — 2026-08-17: fechado por via EMPÍRICA em vez de RE nativa (mesma filosofia
// já usada pra fechar outros itens deste catálogo por caminho diferente do original — o que um
// mod real precisa é a RAZÃO ticks/segundo, não literalmente o endereço `Raw::EngineTime::Frequency`).
// Medido ao vivo: capturei 2 amostras de `EngineTime.ToTicks(GameInstance.GetEngineTime(game))`
// separadas por um intervalo REAL de 10.0s (via `DelaySystem.DelayCallback`, mecanismo já maduro
// neste projeto) — 2 rodadas independentes no mesmo boot deram 1.001.756.288 Hz e 999.922.048 Hz
// (a 2ª, contra o player REAL/estável, com erro de só 0,0078% contra 1 bilhão exato). Os ticks de
// `EngineTime` são NANOSSEGUNDOS (1 tick = 1ns, `1e9` ticks/segundo) — valor limpo, redondo,
// consistente com convenção comum de engine (`std::chrono`/`QueryPerformanceCounter`-based).
// Retorna a constante calibrada; erro esperado < 0.01% pra qualquer uso prático (delta-de-tempo
// de gameplay, não cronometragem científica). Prova: `proofs/2026-08-17-codeware-22-getfrequency-
// EMPIRICO-1GHz-PROVADO.log`.
@addMethod(EngineTime)
public static func GetFrequency() -> Uint64 = 1000000000ul
