//! Resolve a raiz de instalação do Cyberpunk 2077 — fonte única do fallback de paths que
//! antes vivia duplicado entre `deploy.sh` e `check-native-deployed.sh` (aposentados).

use std::path::PathBuf;

/// Locais PADRÃO de instalação no macOS, na ordem em que valem a pena tentar.
///
/// Genéricos de propósito: a lista tinha caminhos da máquina de desenvolvimento
/// (`/Volumes/<nome-do-disco-do-autor>/...`), que só funcionavam para uma pessoa e ainda vazavam o
/// nome do volume dela em todo binário/repositório publicado. Quem instala fora daqui usa
/// `BWMS_GAME` (checado ANTES desta lista) ou uma biblioteca Steam em disco externo — coberto pelo
/// varredor de `/Volumes/*` em `resolve_game_root_from`.
const KNOWN_GAME_PATHS: &[&str] = &[
    // GOG / instalação em /Applications (o `~/Applications` entra via `home`, abaixo)
    "/Applications/Cyberpunk 2077.app",
];

/// Sufixos de biblioteca Steam a tentar dentro de cada volume montado em `/Volumes`.
/// Cobre o caso comum de biblioteca em disco EXTERNO, que nenhum caminho fixo acerta.
const SUFIXOS_STEAM: &[&str] = &[
    "SteamLibrary/steamapps/common/Cyberpunk 2077",
    "steamapps/common/Cyberpunk 2077",
    "Games/Steam/steamapps/common/Cyberpunk 2077",
];

/// Núcleo puro/genérico: 1º candidato da lista que tem subpasta `red4ext/`. Separado da
/// lista real de paths conhecidos pra ficar testável com listas 100% sintéticas — este Mac
/// de desenvolvimento TEM o jogo instalado num dos `KNOWN_GAME_PATHS` reais, então testar
/// contra essa lista misturaria "o teste passou" com "o jogo está instalado aqui agora".
fn first_valid_game_root(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|c| c.join("red4ext").is_dir())
}

/// Monta a lista real de candidatos (env var + paths conhecidos + fallback de `$HOME`) e
/// devolve o 1º que existir de verdade — sem tocar `std::env` diretamente (recebe os valores
/// já lidos), pra ficar testável.
pub fn resolve_game_root_from(env_bwms_game: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = env_bwms_game {
        let path = PathBuf::from(p);
        if path.join("red4ext").is_dir() {
            return Some(path);
        }
        // BWMS_GAME setado mas inválido: cai pro fallback mesmo assim (robustez > rigor).
    }
    let mut candidates: Vec<PathBuf> = KNOWN_GAME_PATHS.iter().map(PathBuf::from).collect();
    if let Some(h) = home {
        let home = PathBuf::from(h);
        candidates.push(home.join("Library/Application Support/Steam/steamapps/common/Cyberpunk 2077"));
        candidates.push(home.join("Applications/Cyberpunk 2077.app"));
    }
    first_valid_game_root(candidates)
}

/// Varre os volumes montados em `/Volumes` atrás de uma biblioteca Steam.
///
/// Sem isto, quem tem o jogo em disco EXTERNO (caso comum, e o do próprio desenvolvimento deste
/// projeto) não é atendido por nenhum caminho fixo — e a alternativa que existia antes era pior:
/// embutir o nome do disco de uma pessoa específica na lista de fallback.
fn scan_volumes() -> Vec<PathBuf> {
    let mut fora = Vec::new();
    let Ok(entradas) = std::fs::read_dir("/Volumes") else {
        return fora;
    };
    for e in entradas.flatten() {
        let vol = e.path();
        for suf in SUFIXOS_STEAM {
            fora.push(vol.join(suf));
        }
    }
    fora
}

pub fn resolve_game_root() -> Option<PathBuf> {
    let env = std::env::var("BWMS_GAME").ok();
    let home = std::env::var("HOME").ok();
    resolve_game_root_from(env.as_deref(), home.as_deref())
        // Só varre `/Volumes` se os caminhos padrão falharem — é I/O em disco removível,
        // não vale pagar quando a instalação está no lugar comum.
        .or_else(|| first_valid_game_root(scan_volumes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkgame(tmp: &std::path::Path, name: &str) -> PathBuf {
        let p = tmp.join(name);
        std::fs::create_dir_all(p.join("red4ext")).unwrap();
        p
    }

    /// Testa o NÚCLEO puro (`first_valid_game_root`) com candidatos 100% sintéticos — nunca
    /// toca `KNOWN_GAME_PATHS` (que pode genuinamente existir na máquina de quem roda o teste).
    #[test]
    fn first_valid_picks_first_existing_candidate_in_order() {
        let tmp = std::env::temp_dir().join(format!("bwms-catalog-game-test-order-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let real = mkgame(&tmp, "real-game");
        let candidates = vec![tmp.join("nao-existe-1"), tmp.join("nao-existe-2"), real.clone(), tmp.join("nao-existe-3")];
        assert_eq!(first_valid_game_root(candidates), Some(real));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn first_valid_none_when_nothing_matches() {
        let tmp = std::env::temp_dir().join(format!("bwms-catalog-game-test-none-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let candidates = vec![tmp.join("nao-existe-1"), tmp.join("nao-existe-2")];
        assert_eq!(first_valid_game_root(candidates), None);
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// `resolve_game_root_from` com env var válida: ganha, sem nem olhar pra `KNOWN_GAME_PATHS`.
    #[test]
    fn env_var_valid_wins_over_fallback() {
        let tmp = std::env::temp_dir().join(format!("bwms-catalog-game-test-env-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let game = mkgame(&tmp, "Cyberpunk 2077");
        let found = resolve_game_root_from(Some(game.to_str().unwrap()), None);
        assert_eq!(found, Some(game));
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// `resolve_game_root_from` com env var AUSENTE e home sintética: só prova que a função
    /// não crasha e devolve `Some`/`None` de forma consistente — não assume que
    /// `KNOWN_GAME_PATHS` está vazio nesta máquina (pode estar instalado de verdade aqui).
    #[test]
    fn no_env_var_does_not_panic_and_is_deterministic() {
        let out1 = resolve_game_root_from(None, Some("/tmp/bwms-nao-existe-home"));
        let out2 = resolve_game_root_from(None, Some("/tmp/bwms-nao-existe-home"));
        assert_eq!(out1, out2, "mesma entrada, mesma saída");
    }
}
