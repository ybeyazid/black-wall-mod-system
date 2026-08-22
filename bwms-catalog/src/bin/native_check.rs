//! native-check — substitui `cp77-console/check-native-deployed.sh` (aposentado): mesma UX
//! manual (uso de terminal), sobre a MESMA lógica que o tool `native_no_dylib` do
//! `bwms-ai-agent` usa (fonte única, sem 2 implementações do mesmo parser divergindo).
//!
//! uso: native-check BwmsFoo BwmsBar ...

fn main() {
    let names: Vec<String> = std::env::args().skip(1).collect();
    if names.is_empty() {
        eprintln!("uso: native-check <NomeDaNative> [outro-nome ...]");
        std::process::exit(1);
    }
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();

    let game_root = bwms_catalog::resolve_game_root();
    if game_root.is_none() {
        eprintln!("AVISO: jogo não encontrado (BWMS_GAME não setado / nenhum path padrão bateu)");
    }
    let deployed = game_root.map(|r| r.join("red4ext/libcp77_console.dylib"));
    let fresh = std::path::PathBuf::from("target/release/libcp77_console.dylib");

    let dep_status = deployed.as_deref().and_then(|p| bwms_catalog::native_status_opt(p, &name_refs));
    let fresh_status = bwms_catalog::native_status_opt(&fresh, &name_refs);

    for (i, name) in names.iter().enumerate() {
        let dep = dep_status.as_ref().map(|v| v[i].to_string()).unwrap_or_else(|| "?".into());
        let fr = fresh_status.as_ref().map(|v| v[i].to_string()).unwrap_or_else(|| "?".into());
        println!("{name}: deployado={dep} fresh={fr}");
    }
}
