// BWMS — TweakXL Facade (redscript puro, 2026-08-05).
//
// Achado de auditoria: o catálogo marca TweakXL "11/11 IMPL", mas a API de CONTROLE que mods
// reais chamam (`TweakXL.Require(version)`/`TweakXL.Version()`/`TweakXL.Reload()`, fonte real em
// `enablers/TweakXL/src/App/Facade.hpp`) nunca tinha sido portada — os 11 gaps cobrem o pipeline
// de APLICAÇÃO de flats/records, não essa fachada de controle.
//
// Mesmo padrão EXATO já provado pro `Codeware.Require`/`Version` (ver `codeware-facade.reds`):
// uma `class` script-defined comum é criada pelo compilador/loader do jogo ao carregar o bundle
// (sem precisar de RegisterType) — o Rust (`register_tweakxl_facade`, `register.rs`) só preenche
// os 3 slots "native" com implementações reais, forjando a classe no MESMO ponto do validador
// (`class_validate_probe_hook`, `selfboot.rs`) onde o Codeware já forja a dele.
//
// `Reload()` é o gatilho MANUAL real do TweakXL (`Facade::Reload()`) — reaplica os `.yaml` já
// carregados sem reiniciar o jogo. Não é hot-reload automático (isso exigiria FSEventStream,
// escopo maior, documentado como pendência separada) — é um comando explícito, mesmo padrão que
// o TweakXL real oferece.
public abstract native class TweakXL {
  public static native func Require(version: String) -> Bool;
  public static native func Version() -> String;
  public static native func Reload() -> Void;
}
