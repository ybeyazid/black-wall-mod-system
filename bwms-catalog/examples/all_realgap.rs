//! uso ad-hoc (auditoria): cargo run --release --example all_realgap -- CODEWARE
//! lista TODOS os itens REAL_GAP do catálogo, ignorando o filtro touched-today (aponta pra
//! um path inexistente de propósito) — usado só pra reconciliar a contagem exata.
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let framework = args.get(0).map(String::as_str).unwrap_or("CODEWARE");

    let catalog = if framework.eq_ignore_ascii_case("ARCHIVEXL") {
        Path::new("../cp77-symbols/notes/CATALOGO-EXAUSTIVO-ARCHIVEXL.md")
    } else {
        Path::new("../cp77-symbols/notes/CATALOGO-EXAUSTIVO-CODEWARE.md")
    };
    let nonexistent = Path::new("/tmp/bwms-catalog-nonexistent-touched.tsv");

    match bwms_catalog::next_candidates(catalog, nonexistent, framework, 500) {
        Ok(mut cands) => {
            println!("total={}", cands.len());
            // Also load full row length (proxy for how much investigation this item has
            // accumulated in the catalog note) by re-reading the catalog and finding the
            // row starting with "| <item> |".
            let content = std::fs::read_to_string(catalog).unwrap_or_default();
            let mut lens: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for line in content.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix('|') {
                    if let Some(idx) = rest.find('|') {
                        let item = rest[..idx].trim().to_string();
                        if !item.is_empty() && item.chars().all(|c| c.is_ascii_digit()) {
                            let e = lens.entry(item).or_insert(0);
                            if line.len() > *e {
                                *e = line.len();
                            }
                        }
                    }
                }
            }
            cands.sort_by_key(|c| lens.get(&c.item).copied().unwrap_or(0));
            for c in &cands {
                println!("[{}] len={} {}", c.item, lens.get(&c.item).copied().unwrap_or(0), c.snippet);
            }
        }
        Err(e) => eprintln!("erro: {e}"),
    }
}
