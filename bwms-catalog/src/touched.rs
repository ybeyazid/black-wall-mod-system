//! Bookkeeping do que já foi investigado hoje (`cp77-symbols/notes/touched-today.tsv`) —
//! TSV simples (`framework<TAB>item<TAB>status<TAB>note<TAB>date`), append atômico
//! (tmp+rename). A coluna `date` (achado 2026-08-19: nunca existiu antes — o arquivo era
//! append-only pra sempre, "touched-TODAY" só de nome; um dia de sessão longa o bastante
//! tocava o catálogo inteiro e travava `next_candidates` permanentemente, mesmo no dia
//! seguinte) deixa `load_touched_fresh` ignorar entradas de dias anteriores.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const HEADER: &str = "framework\titem\tstatus\tnote\n";

/// Algoritmo civil_from_days de Howard Hinnant (domínio público,
/// http://howardhinnant.github.io/date_algorithms.html) — dias desde 1970-01-01 (UTC) para
/// `(ano, mês, dia)`. Zero dependência externa, mesmo idioma do resto do projeto.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// Data de hoje (UTC) como `YYYY-MM-DD` — usada tanto pra carimbar novas linhas quanto pra
/// filtrar `load_touched_fresh`. UTC em vez de fuso local: determinístico, sem depender de
/// `TZ`/config do sistema, suficiente pro propósito (bookkeeping de "mesmo dia de trabalho",
/// não um relógio de parede exato).
pub fn today_date_string() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Nunca falha — arquivo ausente/malformado = mapa vazio/parcial (linhas com <3 campos são
/// puladas em silêncio, mesma tolerância do script original que esta função substitui).
/// Ignora a data (se presente) — "já foi tocado alguma vez", não "hoje". Ver
/// `load_touched_fresh` pra semântica com expiração diária.
pub fn load_touched(path: &Path) -> HashMap<(String, String), (String, String)> {
    let mut touched = HashMap::new();
    let Ok(content) = fs::read_to_string(path) else { return touched };
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let fw = parts[0].trim().to_uppercase();
        let item = parts[1].trim().to_string();
        let status = parts[2].trim().to_string();
        let note = parts.get(3).map(|s| s.trim().to_string()).unwrap_or_default();
        touched.insert((fw, item), (status, note));
    }
    touched
}

/// Igual `load_touched`, mas só conta uma entrada como "tocada" se a 5ª coluna (data) bater
/// EXATO com `today` — linhas de dias anteriores OU sem data (formato antigo, todas as
/// entradas escritas antes deste fix) não bloqueiam mais nada. Isso é o que dá o "-today" de
/// verdade ao nome do arquivo; sem isso o arquivo só cresce e trava `next_candidates` pra
/// sempre depois do 1º dia de uso pesado.
pub fn load_touched_fresh(path: &Path, today: &str) -> HashMap<(String, String), (String, String)> {
    let mut touched = HashMap::new();
    let Ok(content) = fs::read_to_string(path) else { return touched };
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let Some(date) = parts.get(4) else { continue }; // sem data = formato antigo, expirado
        if date.trim() != today {
            continue;
        }
        let fw = parts[0].trim().to_uppercase();
        let item = parts[1].trim().to_string();
        let status = parts[2].trim().to_string();
        let note = parts.get(3).map(|s| s.trim().to_string()).unwrap_or_default();
        touched.insert((fw, item), (status, note));
    }
    touched
}

fn sanitize_tsv_field(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

/// Append atômico de 1 linha: lê o arquivo inteiro (pequeno), concatena, escreve num `.tmp`,
/// RENOMEIA por cima. Não protege contra 2 processos escrevendo ao mesmo tempo — hoje não
/// acontece (bwms-ai-agent roda 1 tarefa por vez, sequencial).
pub fn append_touched(touched_path: &Path, framework: &str, item: &str, status: &str, note: &str) -> io::Result<()> {
    let item_digits: String = item.chars().filter(|c| c.is_ascii_digit()).collect();
    let line = format!(
        "{}\t{}\t{}\t{}\t{}\n",
        framework.to_uppercase(),
        item_digits,
        sanitize_tsv_field(status),
        sanitize_tsv_field(note),
        today_date_string(),
    );
    let mut content = fs::read_to_string(touched_path).unwrap_or_else(|_| HEADER.to_string());
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&line);
    let tmp = touched_path.with_extension("tmp");
    fs::write(&tmp, &content)?;
    fs::rename(&tmp, touched_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("bwms-catalog-touched-test-{}-{}", std::process::id(), name))
    }

    #[test]
    fn append_creates_header_when_absent() {
        let p = tmp_path("create");
        std::fs::remove_file(&p).ok();
        append_touched(&p, "codeware", "142", "negative", "teste").unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        assert!(content.starts_with(HEADER));
        assert!(content.contains(&format!("CODEWARE\t142\tnegative\tteste\t{}\n", today_date_string())));
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn append_preserves_previous_lines() {
        let p = tmp_path("preserve");
        std::fs::remove_file(&p).ok();
        append_touched(&p, "codeware", "1", "closed", "a").unwrap();
        append_touched(&p, "archivexl", "2", "coded", "b").unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        let today = today_date_string();
        assert!(content.contains(&format!("CODEWARE\t1\tclosed\ta\t{today}\n")));
        assert!(content.contains(&format!("ARCHIVEXL\t2\tcoded\tb\t{today}\n")));
        let touched = load_touched(&p);
        assert_eq!(touched.len(), 2);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn civil_from_days_matches_known_reference_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-08-19 — conferido independentemente via `datetime.date(2026,8,19) - datetime.date(1970,1,1)`.
        assert_eq!(civil_from_days(20684), (2026, 8, 19));
    }

    #[test]
    fn today_date_string_has_iso_shape() {
        let s = today_date_string();
        assert_eq!(s.len(), 10);
        assert_eq!(s.as_bytes()[4], b'-');
        assert_eq!(s.as_bytes()[7], b'-');
    }

    #[test]
    fn load_touched_fresh_ignores_rows_without_date_and_from_other_days() {
        let p = tmp_path("fresh");
        std::fs::remove_file(&p).ok();
        let today = today_date_string();
        std::fs::write(
            &p,
            format!(
                "{HEADER}CODEWARE\t1\tclosed\tlegado sem data\nCODEWARE\t2\tclosed\tontem\t2020-01-01\nCODEWARE\t3\tclosed\thoje\t{today}\n"
            ),
        )
        .unwrap();
        let fresh = load_touched_fresh(&p, &today);
        assert!(!fresh.contains_key(&("CODEWARE".to_string(), "1".to_string())));
        assert!(!fresh.contains_key(&("CODEWARE".to_string(), "2".to_string())));
        assert!(fresh.contains_key(&("CODEWARE".to_string(), "3".to_string())));
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn item_normalizes_hash_prefix() {
        let p = tmp_path("hash");
        std::fs::remove_file(&p).ok();
        append_touched(&p, "codeware", "#142", "negative", "").unwrap();
        let touched = load_touched(&p);
        assert!(touched.contains_key(&("CODEWARE".to_string(), "142".to_string())));
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn sanitizes_embedded_tab_and_newline() {
        let p = tmp_path("sanitize");
        std::fs::remove_file(&p).ok();
        append_touched(&p, "codeware", "1", "status\twith\ttabs", "note\nwith\nnewlines").unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        // exatamente 2 linhas (header + a nossa) — se a sanitização falhasse, teríamos mais.
        assert_eq!(content.lines().count(), 2);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn two_sequential_appends_both_present_in_order() {
        let p = tmp_path("sequential");
        std::fs::remove_file(&p).ok();
        append_touched(&p, "codeware", "1", "a", "").unwrap();
        append_touched(&p, "codeware", "2", "b", "").unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2
        assert!(lines[1].starts_with("CODEWARE\t1\ta"));
        assert!(lines[2].starts_with("CODEWARE\t2\tb"));
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn no_leftover_tmp_file_after_append() {
        let p = tmp_path("notmp");
        std::fs::remove_file(&p).ok();
        append_touched(&p, "codeware", "1", "a", "").unwrap();
        assert!(!p.with_extension("tmp").exists());
        std::fs::remove_file(&p).ok();
    }
}
