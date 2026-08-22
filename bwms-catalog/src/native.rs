//! Checa se uma native Rust (nome usado em `register_native`/`register_method`) já está
//! presente no dylib deployado/build mais recente. Substitui
//! `cp77-console/check-native-deployed.sh` (aposentado).
//!
//! **Achado de implementação, não óbvio**: literais `&'static str` do Rust NÃO são
//! delimitados por NUL como strings C — o compilador empacota literais adjacentes byte-a-
//! byte sem separador nenhum (confirmado no dylib real: `"BwmsBitTest8"` e `"BwmsBitSet8"`
//! aparecem grudados como 1 run contínuo de bytes imprimíveis, sem NUL entre eles). Uma
//! checagem "delimitado por NUL" (a 1ª tentativa aqui) falha silenciosamente pra qualquer
//! nome com um literal vizinho colado — o script bash original (`strings -a | grep -qx`,
//! comparação de LINHA INTEIRA) muito provavelmente sofria do mesmo problema. A checagem
//! certa é substring simples nos bytes crus — aceita um risco teórico pequeno de falso-
//! positivo (um nome específico aparecendo como substring de outro por coincidência), mas é
//! o que corresponde à realidade do layout binário.

use std::path::Path;

/// Substring exata nos bytes crus do binário.
pub fn contains_token(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Lê `dylib_path` UMA VEZ pra checar TODOS os nomes — evita reler o arquivo N vezes.
pub fn native_status(dylib_path: &Path, names: &[&str]) -> Result<Vec<bool>, String> {
    let bytes = std::fs::read(dylib_path).map_err(|e| format!("falha lendo {}: {e}", dylib_path.display()))?;
    Ok(names.iter().map(|n| contains_token(&bytes, n.as_bytes())).collect())
}

/// Versão que devolve `None` em vez de `Err` quando o arquivo não existe — o tool
/// `native_no_dylib` quer reportar deployed/fresh-build de forma independente, sem que a
/// ausência de um derrube o outro.
pub fn native_status_opt(dylib_path: &Path, names: &[&str]) -> Option<Vec<bool>> {
    native_status(dylib_path, names).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_token_as_substring() {
        let buf = b"prefixoBwmsFooBar";
        assert!(contains_token(buf, b"BwmsFoo"));
        assert!(contains_token(buf, b"BwmsFooBar"));
    }

    #[test]
    fn needle_absent_returns_false() {
        let buf = b"BwmsFoo";
        assert!(!contains_token(buf, b"BwmsBar"));
    }

    #[test]
    fn needle_at_buffer_edges_counts() {
        let buf = b"BwmsFoo";
        assert!(contains_token(buf, b"BwmsFoo"));
    }

    #[test]
    fn packed_adjacent_literals_no_separator_still_found() {
        // reproduz o layout real observado no dylib: literais colados sem NUL/separador.
        let buf = b"BwmsBitTest8BwmsBitSet8BwmsBitShiftL8";
        assert!(contains_token(buf, b"BwmsBitTest8"));
        assert!(contains_token(buf, b"BwmsBitSet8"));
        assert!(contains_token(buf, b"BwmsBitShiftL8"));
        assert!(!contains_token(buf, b"BwmsBitTest16"));
    }

    #[test]
    fn multiple_names_against_one_buffer_indices_match() {
        let buf = b"AlphaBeta";
        let out: Vec<bool> = ["Alpha", "Gamma", "Beta"].iter().map(|n| contains_token(buf, n.as_bytes())).collect();
        assert_eq!(out, vec![true, false, true]);
    }

    #[test]
    fn io_error_on_missing_path_has_clear_message() {
        let missing = Path::new("/definitivamente/nao/existe/bwms-catalog-test.dylib");
        let err = native_status(missing, &["Anything"]).unwrap_err();
        assert!(err.contains("falha lendo"));
    }

    /// Grounding: contra o dylib deployado real, se disponível neste checkout. `BwmsArchiveExists`
    /// é uma native antiga (Codeware.Depot.ArchiveExists) e confirmada presente por leitura
    /// direta do binário antes de escrever este teste.
    #[test]
    fn grounding_real_deployed_dylib_has_known_native() {
        let Some(root) = crate::game::resolve_game_root() else { return };
        let dylib = root.join("red4ext/libcp77_console.dylib");
        if !dylib.exists() {
            return;
        }
        let status = native_status(&dylib, &["BwmsArchiveExists", "BwmsDoesNotExistXYZ"]).unwrap();
        assert_eq!(status.len(), 2);
        assert!(status[0], "BwmsArchiveExists deveria estar no dylib deployado");
        assert!(!status[1], "nome inventado não deveria estar presente");
    }
}
