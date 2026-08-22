//! Filtra o catálogo exaustivo (`CATALOGO-EXAUSTIVO-<FW>.md`) por itens REAL_GAP (❌) ainda
//! não tocados hoje (ver `touched.rs`), ranqueados por sinal de confiança na própria nota.
//! Porta 1:1 a lógica que antes vivia em `cp77-symbols/tools/next_candidates.py` (aposentado
//! — o projeto é Rust-first, zero Python pra lógica).

use crate::touched::{load_touched_fresh, today_date_string};
use std::fs;
use std::path::Path;

const SNIPPET_MAX_CHARS: usize = 90;
const BAD_SIGNS: &[&str] = &["iserializable", "cmesh-trap", "forge de classe nova", "forjar classe nova"];
const GOOD_SIGNS: &[&str] = &["codado", "candidato", "generated/", "prova ao vivo pendente"];
const DATA_SIGNS: &[&str] = &["campo", "struct", "offset"];

/// Frameworks cujo catálogo tem veredito ❌=REAL_GAP consistente o bastante pra ranquear —
/// RED4EXT/redscript/TweakXL/CET têm formato de tabela diferente (❌ não mapeia 1:1 pra
/// REAL_GAP nesses, a maioria é FORA_DE_ESCOPO marcado de outro jeito).
pub const SUPPORTED_FRAMEWORKS: &[&str] = &["CODEWARE", "ARCHIVEXL"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub item: String,
    pub score: u8,
    pub snippet: String,
}

fn score_text(full_text_lower: &str) -> u8 {
    if BAD_SIGNS.iter().any(|s| full_text_lower.contains(s)) {
        0
    } else if GOOD_SIGNS.iter().any(|s| full_text_lower.contains(s)) {
        3
    } else if DATA_SIGNS.iter().any(|s| full_text_lower.contains(s)) {
        2
    } else {
        1
    }
}

/// Acha a célula de "Status" (coluna 3: `| # | Item | Status | Nota |`) por CONTEÚDO, não por
/// posição — devolve o range `(start, end)` dentro de `after_item` (texto após a coluna 1,
/// já sem o pipe separador). Necessário porque a coluna 2 (descrição/nota) às vezes contém
/// pipes literais no meio do texto (ex. um closure Rust citado, `filter(|(e,_,_)|...)`), que
/// descasam um split posicional ingênuo (`str::split('|')`) e fazem a célula de veredito real
/// nunca ser reconhecida. Todo o catálogo (CODEWARE/ARCHIVEXL, ~390 linhas de tabela
/// verificadas) tem no máximo 1 célula por linha que começa (após trim) com ❌ ou ✅ — marcador
/// confiável mesmo com pipes extras em outras colunas.
///
/// **Investigado e DELIBERADAMENTE NÃO estendido pra `⚠` puro (auditoria de discrepância
/// 2026-08-19)**: testei reconhecer células que começam só com `⚠️` (sem `✅` na frente) como
/// REAL_GAP — no CODEWARE isso pega ~11 itens genuinamente abertos (`#31`/`#33`/`#58`/`#75`/
/// `#88`/etc., texto da própria célula diz "mantido/continua REAL_GAP"), mas no ARCHIVEXL a
/// MESMA regra pega uma porção grande de FALSOS POSITIVOS — vários autores usam `⚠️` ali como
/// marcador de NOTA/RESSALVA genérica sobre um item JÁ RESOLVIDO ou uma observação lateral
/// (ex. item `#86`: "registrar a coincidência mas NÃO tratar como conexão real"; `#69`:
/// "confirma... nenhuma contradição, só validação"), não "ainda é gap". Ou seja, o glifo `⚠`
/// sozinho tem semântica INCONSISTENTE entre os 2 catálogos — não dá pra generalizar por
/// regex sem reintroduzir contagem errada (desta vez inflando ArchiveXL de 18 pra 35, a
/// maioria falso-positivo). Ficou como achado registrado em prosa (`HISTORICO.md`), não como
/// mudança de código — corrigir isso de verdade exigiria 1 sessão de leitura item-a-item.
fn find_verdict_marker(after_item: &str) -> Option<(usize, usize)> {
    let mut search_from = 0;
    while let Some(rel) = after_item[search_from..].find('|') {
        let cell_start = search_from + rel + 1;
        let cell_rest = &after_item[cell_start..];
        let trimmed = cell_rest.trim_start();
        if trimmed.starts_with('❌') || trimmed.starts_with('✅') {
            let cell_end = cell_start + cell_rest.find('|').unwrap_or(cell_rest.len());
            return Some((cell_start, cell_end));
        }
        search_from = cell_start;
    }
    None
}

/// Linha de tabela markdown `| N | descrição | veredito | notas |`. Devolve
/// `(item, veredito_bruto, texto_completo_em_minusculas)` só se a 1ª célula for um item
/// numérico E a célula de veredito (achada por conteúdo, ver `find_verdict_marker`) começar
/// com ❌ (não `✅❌` = já fechado, marca histórica).
fn parse_real_gap_row(line: &str) -> Option<(String, String, String)> {
    let t = line.trim();
    if !t.starts_with('|') {
        return None;
    }
    let rest = &t[1..];
    let first_pipe = rest.find('|')?;
    let item = rest[..first_pipe].trim();
    if item.is_empty() || !item.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let after_item = &rest[first_pipe + 1..];
    let (start, end) = find_verdict_marker(after_item)?;
    let verdict = after_item[start..end].trim();
    if !verdict.starts_with('❌') {
        return None;
    }
    Some((item.to_string(), verdict.to_string(), rest.to_lowercase()))
}

/// Lista TODOS os itens ❌ REAL_GAP do catálogo (sem filtro de `touched-today`, sem limite,
/// ordenado por número do item) — usado pelo painel pra renderizar a lista numerada de
/// pendências reais (equivalente local ao dashboard web), não pra ranquear "próximo candidato
/// a investigar" (isso é `next_candidates`). Mesmo parser robusto a pipe-literal.
pub fn all_real_gap_items(catalog_path: &Path, framework: &str) -> Result<Vec<Candidate>, String> {
    let fw_upper = framework.to_uppercase();
    if !SUPPORTED_FRAMEWORKS.contains(&fw_upper.as_str()) {
        return Err(format!(
            "'{framework}' não tem veredito ❌=REAL_GAP 1:1 — só {} são suportados",
            SUPPORTED_FRAMEWORKS.join("/")
        ));
    }
    let content = fs::read_to_string(catalog_path)
        .map_err(|e| format!("falha lendo {}: {e}", catalog_path.display()))?;

    let mut items: Vec<Candidate> = content
        .lines()
        .filter_map(parse_real_gap_row)
        .map(|(item, verdict, full_lower)| Candidate {
            score: score_text(&full_lower),
            snippet: verdict.chars().take(SNIPPET_MAX_CHARS).collect(),
            item,
        })
        .collect();

    items.sort_by_key(|c| c.item.parse::<u32>().unwrap_or(u32::MAX));
    Ok(items)
}

pub fn next_candidates(
    catalog_path: &Path,
    touched_path: &Path,
    framework: &str,
    limit: usize,
) -> Result<Vec<Candidate>, String> {
    let fw_upper = framework.to_uppercase();
    if !SUPPORTED_FRAMEWORKS.contains(&fw_upper.as_str()) {
        return Err(format!(
            "'{framework}' não tem veredito ❌=REAL_GAP 1:1 — só {} são suportados",
            SUPPORTED_FRAMEWORKS.join("/")
        ));
    }
    let content = fs::read_to_string(catalog_path)
        .map_err(|e| format!("falha lendo {}: {e}", catalog_path.display()))?;
    let touched = load_touched_fresh(touched_path, &today_date_string());

    let mut candidates: Vec<Candidate> = content
        .lines()
        .filter_map(parse_real_gap_row)
        .filter(|(item, _, _)| !touched.contains_key(&(fw_upper.clone(), item.clone())))
        .map(|(item, verdict, full_lower)| Candidate {
            score: score_text(&full_lower),
            snippet: verdict.chars().take(SNIPPET_MAX_CHARS).collect(),
            item,
        })
        .collect();

    candidates.sort_by(|a, b| b.score.cmp(&a.score)); // sort estável — empate preserva ordem do catálogo
    candidates.truncate(limit.max(1));
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::touched::append_touched;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("bwms-catalog-candidates-test-{}-{}", std::process::id(), name))
    }

    #[test]
    fn ignores_non_table_and_non_realgap_lines() {
        let catalog = tmp_path("catalog1.md");
        std::fs::write(
            &catalog,
            "# título\n\nnão é tabela\n| 1 | desc | ✅ fechado | nota |\n| 2 | desc | ❌ REAL_GAP | nota |\n",
        )
        .unwrap();
        let touched = tmp_path("touched1.tsv");
        std::fs::remove_file(&touched).ok();
        let out = next_candidates(&catalog, &touched, "CODEWARE", 10).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].item, "2");
        std::fs::remove_file(&catalog).ok();
    }

    #[test]
    fn filters_touched_items_case_insensitive_framework() {
        let catalog = tmp_path("catalog2.md");
        std::fs::write(&catalog, "| 5 | desc | ❌ REAL_GAP | nota |\n| 6 | desc | ❌ REAL_GAP | nota |\n").unwrap();
        let touched = tmp_path("touched2.tsv");
        std::fs::remove_file(&touched).ok();
        append_touched(&touched, "codeware", "5", "negative", "").unwrap();
        let out = next_candidates(&catalog, &touched, "CODEWARE", 10).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].item, "6");
        std::fs::remove_file(&catalog).ok();
        std::fs::remove_file(&touched).ok();
    }

    #[test]
    fn scoring_four_bands_bad_beats_good() {
        let catalog = tmp_path("catalog3.md");
        std::fs::write(
            &catalog,
            "| 1 | mistura CODADO e ISerializable, categoria arriscada | ❌ REAL_GAP | |\n\
             | 2 | tem candidato pronto pra testar | ❌ REAL_GAP | |\n\
             | 3 | precisa ler o campo/offset/struct | ❌ REAL_GAP | |\n\
             | 4 | nada demais aqui | ❌ REAL_GAP | |\n",
        )
        .unwrap();
        let touched = tmp_path("touched3.tsv");
        std::fs::remove_file(&touched).ok();
        let out = next_candidates(&catalog, &touched, "CODEWARE", 10).unwrap();
        let by_item = |id: &str| out.iter().find(|c| c.item == id).unwrap().score;
        assert_eq!(by_item("1"), 0, "BAD_SIGNS tem prioridade sobre GOOD_SIGNS");
        assert_eq!(by_item("2"), 3);
        assert_eq!(by_item("3"), 2);
        assert_eq!(by_item("4"), 1);
        std::fs::remove_file(&catalog).ok();
    }

    #[test]
    fn plain_warning_marker_is_deliberately_not_treated_as_real_gap() {
        // Documenta a decisão de NÃO reconhecer `⚠️` puro como REAL_GAP (ver comentário de
        // `find_verdict_marker`) — testei estender e reverti: no CODEWARE `⚠️` sozinho às
        // vezes é "ainda aberto" (~11 casos reais auditados manualmente em 2026-08-19), mas no
        // ARCHIVEXL a mesma célula frequentemente é só uma nota/ressalva sobre algo já
        // resolvido — semântica inconsistente entre catálogos, não dá pra generalizar por
        // regex sem reintroduzir contagem errada. `✅⚠️` (combo fechado-com-ressalva) também
        // fica de fora, como já era.
        let catalog = tmp_path("catalog_warn.md");
        std::fs::write(
            &catalog,
            "| 1 | item parcial ainda aberto (nota diz aberto) | ⚠️ **PARCIAL (3/5) — Item mantido REAL_GAP** | nota |\n\
             | 2 | item fechado com ressalva | ✅⚠️ **FECHADO, ressalva documentada** | nota |\n\
             | 3 | item aberto normal | ❌ REAL_GAP | nota |\n\
             | 4 | item fechado normal | ✅ FECHADO | nota |\n",
        )
        .unwrap();
        let touched = tmp_path("touched_warn.tsv");
        std::fs::remove_file(&touched).ok();
        let out = next_candidates(&catalog, &touched, "CODEWARE", 10).unwrap();
        let ids: Vec<&str> = out.iter().map(|c| c.item.as_str()).collect();
        assert!(!ids.contains(&"1"), "⚠️ puro fica de fora do parser mecânico (ver nota acima)");
        assert!(ids.contains(&"3"), "❌ continua contando normalmente");
        assert!(!ids.contains(&"2"), "✅⚠️ (combo fechado-com-ressalva) NÃO deve contar");
        assert!(!ids.contains(&"4"), "✅ puro continua excluído");
        assert_eq!(ids.len(), 1);
        std::fs::remove_file(&catalog).ok();
    }

    #[test]
    fn limit_truncates_and_sort_is_stable_on_ties() {
        let catalog = tmp_path("catalog4.md");
        std::fs::write(
            &catalog,
            "| 1 | sem sinal | ❌ REAL_GAP | |\n| 2 | sem sinal | ❌ REAL_GAP | |\n| 3 | sem sinal | ❌ REAL_GAP | |\n",
        )
        .unwrap();
        let touched = tmp_path("touched4.tsv");
        std::fs::remove_file(&touched).ok();
        let out = next_candidates(&catalog, &touched, "CODEWARE", 2).unwrap();
        assert_eq!(out.len(), 2);
        // todos score 1 (empate) — ordem do catálogo preservada (sort estável).
        assert_eq!(out[0].item, "1");
        assert_eq!(out[1].item, "2");
        std::fs::remove_file(&catalog).ok();
    }

    #[test]
    fn snippet_truncation_safe_with_multibyte_emoji_at_boundary() {
        // veredito rico em emoji perto do limite de 90 chars — .chars() nunca deve panicar.
        let emoji_heavy: String = "❌🔑".repeat(50); // bem além de 90 chars
        let catalog = tmp_path("catalog5.md");
        std::fs::write(&catalog, format!("| 1 | desc | {emoji_heavy} REAL_GAP | |\n")).unwrap();
        let touched = tmp_path("touched5.tsv");
        std::fs::remove_file(&touched).ok();
        let out = next_candidates(&catalog, &touched, "CODEWARE", 10).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].snippet.chars().count() <= SNIPPET_MAX_CHARS);
        std::fs::remove_file(&catalog).ok();
    }

    #[test]
    fn survives_literal_pipe_inside_description_text() {
        // Reproduz o bug real (2026-08-19): item #6 do catálogo CODEWARE cita um closure
        // Rust com pipes literais (`filter(|(e,_,_)| *e==eh)`) dentro da NOTA — o pipe extra
        // desloca a contagem posicional de colunas e a coluna de veredito (❌) para de bater,
        // fazendo `next_candidates` devolver 0 candidatos mesmo com REAL_GAP genuíno aberto.
        let catalog = tmp_path("catalog_pipebug.md");
        std::fs::write(
            &catalog,
            "| 6 | despacha via `fire_event_args_by_hash` (`CALLBACKS.lock()...filter(|(e,_,_)| *e==eh)`) sem checar targets | ❌ REAL_GAP | nota |\n",
        )
        .unwrap();
        let touched = tmp_path("touched_pipebug.tsv");
        std::fs::remove_file(&touched).ok();
        let out = next_candidates(&catalog, &touched, "CODEWARE", 10).unwrap();
        assert_eq!(out.len(), 1, "item 6 com pipe literal na descrição deve ser reconhecido como candidato");
        assert_eq!(out[0].item, "6");
        std::fs::remove_file(&catalog).ok();
    }

    #[test]
    fn unsupported_framework_returns_err() {
        let catalog = tmp_path("catalog6.md");
        std::fs::write(&catalog, "| 1 | desc | ❌ REAL_GAP | |\n").unwrap();
        let touched = tmp_path("touched6.tsv");
        let err = next_candidates(&catalog, &touched, "RED4ext.SDK", 10).unwrap_err();
        assert!(err.contains("RED4ext.SDK"));
        std::fs::remove_file(&catalog).ok();
    }

    /// Grounding: roda contra o catálogo REAL do projeto, mas com um `touched-today.tsv`
    /// SINTÉTICO (marcando hoje um item real de verdade extraído do catálogo) — não depende
    /// do conteúdo do arquivo real, que agora expira diariamente (achado 2026-08-19: o
    /// arquivo real era append-only pra sempre, travando `next_candidates` permanentemente
    /// depois do 1º dia de uso pesado; o teste antigo dependia justamente do bug pra passar).
    /// Confirma as 2 pontas: um item marcado HOJE some da lista; um marcado ONTEM não.
    #[test]
    fn grounding_real_catalog_item_hidden_only_while_touched_today() {
        let catalog = Path::new("../cp77-symbols/notes/CATALOGO-EXAUSTIVO-CODEWARE.md");
        if !catalog.exists() {
            return;
        }
        let all = super::all_real_gap_items(catalog, "CODEWARE").unwrap();
        let Some(real_item) = all.first() else { return }; // catálogo sem REAL_GAP nenhum hoje = nada a testar
        let item = real_item.item.clone();

        let touched = tmp_path("grounding_real_touched.tsv");
        std::fs::remove_file(&touched).ok();
        let out_before = next_candidates(catalog, &touched, "CODEWARE", 1000).unwrap();
        assert!(out_before.iter().any(|c| c.item == item), "item real #{item} deveria aparecer sem nada tocado");

        crate::touched::append_touched(&touched, "CODEWARE", &item, "negative", "grounding test").unwrap();
        let out_after = next_candidates(catalog, &touched, "CODEWARE", 1000).unwrap();
        assert!(
            !out_after.iter().any(|c| c.item == item),
            "item real #{item} marcado tocado HOJE não deveria mais aparecer"
        );
        std::fs::remove_file(&touched).ok();
    }
}
