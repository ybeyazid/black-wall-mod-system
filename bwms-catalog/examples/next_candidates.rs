//! uso ad-hoc: cargo run --release --example next_candidates -- CODEWARE 20
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let framework = args.get(0).map(String::as_str).unwrap_or("CODEWARE");
    let limit: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(15);

    let catalog = Path::new("../cp77-symbols/notes/CATALOGO-EXAUSTIVO-CODEWARE.md");
    let catalog = if framework.eq_ignore_ascii_case("ARCHIVEXL") {
        Path::new("../cp77-symbols/notes/CATALOGO-EXAUSTIVO-ARCHIVEXL.md")
    } else {
        catalog
    };
    let touched = Path::new("../cp77-symbols/notes/touched-today.tsv");

    match bwms_catalog::next_candidates(catalog, touched, framework, limit) {
        Ok(cands) => {
            for c in cands {
                println!("[{}] score={} {}", c.item, c.score, c.snippet);
            }
        }
        Err(e) => eprintln!("erro: {e}"),
    }
}
