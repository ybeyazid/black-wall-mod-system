//! bwms-scoreboard — parser/writer da tabela `## 🏁 PLACAR REAL` de
//! `cp77-symbols/notes/PENDENCIAS-UNIFICADAS.md`. Fonte ÚNICA (mesma filosofia de
//! `bwms-hashes`): qualquer coisa que precise ler ou decrementar o placar (o painel de
//! controle, o `promote()` do `bwms-ai-agent`, futuras ferramentas) usa este crate, nunca
//! reimplementa o parse da tabela.
//!
//! Formato real da tabela (2026-08, `PENDENCIAS-UNIFICADAS.md:273-282`):
//! ```text
//! ## 🏁 PLACAR REAL
//!
//! | | RED4ext.SDK | redscript | TweakXL | ArchiveXL | Codeware | CET | **TOTAL** |
//! |---|---:|---:|---:|---:|---:|---:|---:|
//! | ✅ Provado | 145 | 10 | 23 | 55 | 156 | 49 | **438** |
//! | 🟢 Equivalente | 187 | 2 | 9 | 13 | 12 | 2 | **225** |
//! | 🔴 **REAL_GAP** | **3** | **0** | **0** | **17** | **81** | **0** | **101** |
//! | 🟡 Incerto | 1 | 0 | 0 | 2 | 0 | 0 | **3** |
//! | ⚪ Fora de escopo | 142 | 26 | 15 | 24 | 13 | 9 | **229** |
//! | **Universo relevante** (tudo - fora de escopo) | 336 | 12 | 32 | 87 | 249 | 51 | **767** |
//! ```
//! Robusto a variação de estilo bold (`**101**` vs `101`) e a texto extra depois do rótulo
//! (ex. `(tudo - fora de escopo)`), mas assume: heading contendo "PLACAR REAL", header row
//! começando com `| |`, 6 colunas de framework + coluna TOTAL, na mesma ordem em toda linha.

use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowKind {
    Provado,
    Equivalente,
    RealGap,
    Incerto,
    ForaDeEscopo,
    UniversoRelevante,
}

impl RowKind {
    /// Reconhece o RowKind a partir do texto bruto da 1ª célula de uma linha (com emoji/bold/
    /// parênteses ainda presentes) — casa por substring, ordem importa (REAL_GAP antes de
    /// "Universo" etc. não colidem, mas a ordem abaixo é a mais específica primeiro por clareza).
    fn from_label(label: &str) -> Option<RowKind> {
        let l = label;
        if l.contains("REAL_GAP") {
            Some(RowKind::RealGap)
        } else if l.contains("Universo relevante") {
            Some(RowKind::UniversoRelevante)
        } else if l.contains("Fora de escopo") {
            Some(RowKind::ForaDeEscopo)
        } else if l.contains("Provado") {
            Some(RowKind::Provado)
        } else if l.contains("Equivalente") {
            Some(RowKind::Equivalente)
        } else if l.contains("Incerto") {
            Some(RowKind::Incerto)
        } else {
            None
        }
    }
}

impl fmt::Display for RowKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            RowKind::Provado => "Provado",
            RowKind::Equivalente => "Equivalente",
            RowKind::RealGap => "REAL_GAP",
            RowKind::Incerto => "Incerto",
            RowKind::ForaDeEscopo => "Fora de escopo",
            RowKind::UniversoRelevante => "Universo relevante",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone)]
pub struct RowCounts {
    /// Valores por framework, na MESMA ordem de `Scoreboard::frameworks`.
    pub per_framework: Vec<u32>,
    pub total: u32,
}

#[derive(Debug, Clone)]
pub struct Scoreboard {
    /// Nomes dos frameworks na ordem das colunas (ex. ["RED4ext.SDK","redscript",...]).
    pub frameworks: Vec<String>,
    pub rows: Vec<(RowKind, RowCounts)>,
    /// Linha (1-indexed) do arquivo-fonte onde cada RowKind foi encontrado — usado por
    /// `apply_decrement` para editar a linha certa sem precisar re-parsear tudo.
    line_of: Vec<(RowKind, usize)>,
    /// Índice (0-indexed) da linha do header (framework names) no arquivo-fonte. Não lido
    /// ainda por ninguém — reservado para uma futura operação de inserir linha/coluna nova.
    #[allow(dead_code)]
    header_line: usize,
}

impl Scoreboard {
    pub fn row(&self, kind: RowKind) -> Option<&RowCounts> {
        self.rows.iter().find(|(k, _)| *k == kind).map(|(_, c)| c)
    }

    pub fn real_gap_total(&self) -> u32 {
        self.row(RowKind::RealGap).map(|r| r.total).unwrap_or(0)
    }

    pub fn universo_total(&self) -> u32 {
        self.row(RowKind::UniversoRelevante)
            .map(|r| r.total)
            .unwrap_or(0)
    }

    pub fn provado_total(&self) -> u32 {
        self.row(RowKind::Provado).map(|r| r.total).unwrap_or(0)
    }

    pub fn framework_index(&self, name: &str) -> Option<usize> {
        self.frameworks.iter().position(|f| f == name)
    }

    /// Checagem de consistência aritmética: para cada framework (e para o TOTAL),
    /// Provado + Equivalente + REAL_GAP + Incerto == Universo relevante. Não depende de
    /// números mágicos — serve tanto de teste quanto de guarda real dentro de
    /// `apply_decrement` (nunca escreve um resultado que viole isto).
    pub fn is_arithmetically_consistent(&self) -> bool {
        let provado = self.row(RowKind::Provado);
        let equiv = self.row(RowKind::Equivalente);
        let gap = self.row(RowKind::RealGap);
        let incerto = self.row(RowKind::Incerto);
        let universo = self.row(RowKind::UniversoRelevante);
        let (provado, equiv, gap, incerto, universo) =
            match (provado, equiv, gap, incerto, universo) {
                (Some(a), Some(b), Some(c), Some(d), Some(e)) => (a, b, c, d, e),
                _ => return false,
            };
        let n = self.frameworks.len();
        for i in 0..n {
            let sum = provado.per_framework[i]
                + equiv.per_framework[i]
                + gap.per_framework[i]
                + incerto.per_framework[i];
            if sum != universo.per_framework[i] {
                return false;
            }
        }
        let sum_total = provado.total + equiv.total + gap.total + incerto.total;
        sum_total == universo.total
    }
}

fn split_row(line: &str) -> Vec<String> {
    let line = line.trim();
    let inner = line
        .trim_start_matches('|')
        .trim_end_matches('|')
        .trim_end_matches('\r');
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

fn parse_cell_number(cell: &str) -> Option<u32> {
    let digits: String = cell.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

/// Verdadeiro se a linha "parece" uma linha de tabela markdown (`| ... |` ou `|---|...`).
fn looks_like_table_row(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.ends_with('|') && t.len() > 1
}

fn is_divider_row(cells: &[String]) -> bool {
    // linha "|---|---:|---:|..." — todas as células só têm '-'/':' depois do trim.
    cells
        .iter()
        .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
}

pub fn parse_placar_table(path: &Path) -> Result<Scoreboard, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("falha lendo {}: {e}", path.display()))?;
    parse_placar_table_str(&content)
}

pub fn parse_placar_table_str(content: &str) -> Result<Scoreboard, String> {
    let lines: Vec<&str> = content.lines().collect();

    let heading_idx = lines
        .iter()
        .position(|l| l.contains("PLACAR REAL"))
        .ok_or_else(|| "heading 'PLACAR REAL' não encontrado".to_string())?;

    // A partir do heading, acha a 1ª linha de tabela (header row, "| | Fw1 | Fw2 | ...").
    let mut i = heading_idx + 1;
    while i < lines.len() && !looks_like_table_row(lines[i]) {
        i += 1;
    }
    if i >= lines.len() {
        return Err("nenhuma linha de tabela encontrada após o heading".to_string());
    }
    let header_line_idx = i;
    let header_cells = split_row(lines[i]);
    if header_cells.len() < 2 {
        return Err(format!(
            "header row com poucas células ({}): {:?}",
            header_cells.len(),
            lines[i]
        ));
    }
    // header_cells[0] é o canto vazio; header_cells[1..len-1] são frameworks; a última é TOTAL.
    if header_cells.len() < 3 {
        return Err("header row não tem frameworks suficientes".to_string());
    }
    let frameworks: Vec<String> = header_cells[1..header_cells.len() - 1].to_vec();
    let n_fw = frameworks.len();
    i += 1;

    // linha divisora "|---|---:|..." — opcional mas normalmente presente; pula se achar.
    if i < lines.len() && looks_like_table_row(lines[i]) {
        let cells = split_row(lines[i]);
        if is_divider_row(&cells) {
            i += 1;
        }
    }

    let mut rows: Vec<(RowKind, RowCounts)> = Vec::new();
    let mut line_of: Vec<(RowKind, usize)> = Vec::new();

    while i < lines.len() && looks_like_table_row(lines[i]) {
        let cells = split_row(lines[i]);
        if cells.len() != n_fw + 2 {
            // linha de formato inesperado (não deveria acontecer na tabela real) — para aqui,
            // trata como fim da tabela em vez de falhar (robustez a texto solto logo abaixo).
            break;
        }
        let label = &cells[0];
        let kind = match RowKind::from_label(label) {
            Some(k) => k,
            None => break, // não reconhecida -> fim da tabela
        };
        let per_framework: Vec<u32> = cells[1..cells.len() - 1]
            .iter()
            .map(|c| parse_cell_number(c).unwrap_or(0))
            .collect();
        let total = parse_cell_number(&cells[cells.len() - 1]).unwrap_or(0);
        rows.push((kind, RowCounts { per_framework, total }));
        line_of.push((kind, i));
        i += 1;
    }

    if rows.is_empty() {
        return Err("nenhuma linha de dado reconhecida na tabela".to_string());
    }

    Ok(Scoreboard {
        frameworks,
        rows,
        line_of,
        header_line: header_line_idx,
    })
}

/// Reescreve, EM MEMÓRIA (o chamador decide se/como grava em disco), a célula de
/// `framework` na linha `kind`, subtraindo `amount` do valor atual e recomputando o TOTAL
/// daquela linha (soma dos frameworks) — preserva o estilo de wrap (`**N**` vs `N`) já
/// presente na célula original. Retorna erro se o resultado ficaria negativo ou se
/// `framework`/`kind` não existem.
///
/// Não escreve outra linha nem recalcula `Universo relevante` — o chamador (`promote.rs`)
/// é responsável por também chamar `add_to_row` na linha de destino (ex. mover 1 item de
/// REAL_GAP pra Provado precisa de 1 `subtract` + 1 `add`) e por checar
/// `is_arithmetically_consistent()` no resultado final antes de gravar.
pub fn subtract_from_cell(
    content: &str,
    scoreboard: &Scoreboard,
    kind: RowKind,
    framework: &str,
    amount: u32,
) -> Result<String, String> {
    add_or_subtract_cell(content, scoreboard, kind, framework, -(amount as i64))
}

pub fn add_to_cell(
    content: &str,
    scoreboard: &Scoreboard,
    kind: RowKind,
    framework: &str,
    amount: u32,
) -> Result<String, String> {
    add_or_subtract_cell(content, scoreboard, kind, framework, amount as i64)
}

fn add_or_subtract_cell(
    content: &str,
    scoreboard: &Scoreboard,
    kind: RowKind,
    framework: &str,
    delta: i64,
) -> Result<String, String> {
    let fw_idx = scoreboard
        .framework_index(framework)
        .ok_or_else(|| format!("framework desconhecido: {framework}"))?;
    let line_idx = scoreboard
        .line_of
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, l)| *l)
        .ok_or_else(|| format!("row kind não encontrado no arquivo: {kind}"))?;

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    if line_idx >= lines.len() {
        return Err("índice de linha fora do arquivo (arquivo mudou desde o parse?)".to_string());
    }
    let orig_line = lines[line_idx].clone();
    let cells = split_row(&orig_line);
    if cells.len() < fw_idx + 2 {
        return Err("linha não tem células suficientes pro framework pedido".to_string());
    }
    let cell_idx_in_split = fw_idx + 1; // +1 pq índice 0 é o rótulo
    let old_cell = &cells[cell_idx_in_split];
    let old_val = parse_cell_number(old_cell).unwrap_or(0) as i64;
    let new_val = old_val + delta;
    if new_val < 0 {
        return Err(format!(
            "resultado negativo ({old_val} + {delta} = {new_val}) pra {kind}/{framework} — recusado"
        ));
    }
    let bold = old_cell.trim().starts_with("**") && old_cell.trim().ends_with("**");
    let new_cell_text = if bold {
        format!("**{new_val}**")
    } else {
        new_val.to_string()
    };

    // recompõe a linha inteira a partir das células (preserva o rótulo original,
    // incl. emoji/bold/parênteses, e o wrap de cada outra célula tal como estava).
    let mut new_cells = cells.clone();
    new_cells[cell_idx_in_split] = new_cell_text;
    // recomputa o total (última célula) como soma dos frameworks, preservando o wrap
    // que a célula de total já tinha.
    let n = scoreboard.frameworks.len();
    let total_cell_idx = cells.len() - 1;
    let old_total_cell = &cells[total_cell_idx];
    let total_bold = old_total_cell.trim().starts_with("**") && old_total_cell.trim().ends_with("**");
    let mut new_total: i64 = 0;
    for c in new_cells[1..1 + n].iter() {
        new_total += parse_cell_number(c).unwrap_or(0) as i64;
    }
    new_cells[total_cell_idx] = if total_bold {
        format!("**{new_total}**")
    } else {
        new_total.to_string()
    };

    let new_line = format!("| {} |", new_cells.join(" | "));
    lines[line_idx] = new_line;
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
# doc de teste

## 🏁 PLACAR REAL

| | RED4ext.SDK | redscript | TweakXL | ArchiveXL | Codeware | CET | **TOTAL** |
|---|---:|---:|---:|---:|---:|---:|---:|
| ✅ Provado | 145 | 10 | 23 | 55 | 156 | 49 | **438** |
| 🟢 Equivalente | 187 | 2 | 9 | 13 | 12 | 2 | **225** |
| 🔴 **REAL_GAP** | **3** | **0** | **0** | **17** | **81** | **0** | **101** |
| 🟡 Incerto | 1 | 0 | 0 | 2 | 0 | 0 | **3** |
| ⚪ Fora de escopo | 142 | 26 | 15 | 24 | 13 | 9 | **229** |
| **Universo relevante** (tudo - fora de escopo) | 336 | 12 | 32 | 87 | 249 | 51 | **767** |

texto depois da tabela, não deve ser parseado como linha.
";

    #[test]
    fn parses_fixture_frameworks_and_rows() {
        let sb = parse_placar_table_str(FIXTURE).expect("parse ok");
        assert_eq!(
            sb.frameworks,
            vec!["RED4ext.SDK", "redscript", "TweakXL", "ArchiveXL", "Codeware", "CET"]
        );
        assert_eq!(sb.rows.len(), 6);
        assert_eq!(sb.real_gap_total(), 101);
        assert_eq!(sb.universo_total(), 767);
        assert_eq!(sb.provado_total(), 438);
    }

    #[test]
    fn real_gap_row_matches_known_values() {
        let sb = parse_placar_table_str(FIXTURE).unwrap();
        let gap = sb.row(RowKind::RealGap).unwrap();
        assert_eq!(gap.per_framework, vec![3, 0, 0, 17, 81, 0]);
        assert_eq!(gap.total, 101);
    }

    #[test]
    fn fixture_is_arithmetically_consistent() {
        let sb = parse_placar_table_str(FIXTURE).unwrap();
        assert!(sb.is_arithmetically_consistent());
    }

    #[test]
    fn subtract_then_add_moves_one_item_and_stays_consistent() {
        let sb = parse_placar_table_str(FIXTURE).unwrap();
        // simula promover 1 item REAL_GAP -> Provado no framework ArchiveXL.
        let step1 =
            subtract_from_cell(FIXTURE, &sb, RowKind::RealGap, "ArchiveXL", 1).expect("subtract ok");
        let sb2 = parse_placar_table_str(&step1).unwrap();
        assert_eq!(sb2.row(RowKind::RealGap).unwrap().per_framework[3], 16);
        assert_eq!(sb2.row(RowKind::RealGap).unwrap().total, 100);

        let step2 =
            add_to_cell(&step1, &sb2, RowKind::Provado, "ArchiveXL", 1).expect("add ok");
        let sb3 = parse_placar_table_str(&step2).unwrap();
        assert_eq!(sb3.row(RowKind::Provado).unwrap().per_framework[3], 56);
        assert_eq!(sb3.row(RowKind::Provado).unwrap().total, 439);

        // Universo relevante não muda (item só migrou de categoria, não saiu do universo).
        assert_eq!(sb3.universo_total(), 767);
        assert!(sb3.is_arithmetically_consistent());
    }

    #[test]
    fn subtract_below_zero_is_rejected() {
        let sb = parse_placar_table_str(FIXTURE).unwrap();
        let err = subtract_from_cell(FIXTURE, &sb, RowKind::RealGap, "redscript", 1);
        assert!(err.is_err(), "redscript já está em 0, subtrair deveria falhar");
    }

    #[test]
    fn unknown_framework_is_rejected() {
        let sb = parse_placar_table_str(FIXTURE).unwrap();
        let err = subtract_from_cell(FIXTURE, &sb, RowKind::RealGap, "NaoExiste", 1);
        assert!(err.is_err());
    }

    #[test]
    fn parses_real_pendencias_file_and_is_consistent() {
        // Grounding contra o arquivo REAL do projeto — não hardcoda números (eles mudam
        // quase todo dia), só verifica que o parser funciona nele e que a tabela real está
        // aritmeticamente consistente (isso também serve de guarda de qualidade contínua:
        // se algum dia a tabela real ficar inconsistente, este teste falha e avisa).
        let path = Path::new("../cp77-symbols/notes/PENDENCIAS-UNIFICADAS.md");
        if !path.exists() {
            eprintln!("PENDENCIAS-UNIFICADAS.md não encontrado neste checkout, pulando teste de grounding");
            return;
        }
        let sb = parse_placar_table(path).expect("parse do arquivo real deve funcionar");
        assert_eq!(sb.frameworks.len(), 6);
        assert!(sb.real_gap_total() > 0, "REAL_GAP total deveria ser > 0 no estado atual do projeto");
        assert!(
            sb.is_arithmetically_consistent(),
            "tabela real do placar está aritmeticamente inconsistente — investigar antes de confiar em apply_decrement"
        );
    }
}
