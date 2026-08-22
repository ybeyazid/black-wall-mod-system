// codeware-resourcedepot.reds — Codeware `#16`/`#198` (`ResourceDepot`, `PENDENCIAS-UNIFICADAS.md`),
// declarações PERMANENTES pras 2 natives já fechadas mas nunca declaradas fora de smoke tests
// removidos (mesmo padrão de gap já corrigido em `tweakxl-scriptinterface.reds`/`codeware-casts.reds`:
// registradas no Rust, mas sem nenhum .reds ativo declarando — nenhum mod conseguia chamá-las).
native func BwmsResourceExists(path: String) -> Bool
native func BwmsArchiveExists(name: String) -> Bool
