//! Eixo TEMÁTICO do mod (roupa/cabelo/carro/...) — complementa o eixo técnico (classify.rs).
//! "tool sugere, usuário confirma": `suggest()` chuta o tema por palavras-chave; o usuário decide.
//! Também o modelo de ESTADO (ativo/inativo/favorito/ordem) persistido em `.cp77-mods/bwms-mods.json`
//! (zero-dep, JSON na mão). Esse JSON é o CONTRATO que a UI in-game vai ler/escrever (via ponte Codeware).

use crate::classify::ModReport;
use std::path::Path;

/// Temas user-facing (as "abas" da página de Mods). Eixo ortogonal ao FileKind técnico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Roupas,   // jaquetas, outfits, peças de roupa
    Cabelos,  // cabelo, barba
    Npc,      // aparência de NPC nomeado (Panam, Judy, River...)
    Estetica, // rosto/pele/corpo do V (complexion, makeup, tattoo)
    Veiculos, // carros, motos, paint jobs
    Armas,    // armas, skins de arma, scopes
    Clima,    // clima/tempo: weather, iluminação, timecycle
    Lut,      // LUT / color grading / presets de cor (Nova, Preem, packs cinematográficos)
    Mundo,    // posters, billboards, TVs, props, ambiente
    Gameplay, // balance, combate, economia, cyberware, AI
    Cheats,   // god mode, money, unlock — runtime
    Outros,   // não classificado
}

impl Theme {
    pub fn slug(self) -> &'static str {
        match self {
            Theme::Roupas => "roupas",
            Theme::Cabelos => "cabelos",
            Theme::Npc => "npc",
            Theme::Estetica => "estetica",
            Theme::Veiculos => "veiculos",
            Theme::Armas => "armas",
            Theme::Clima => "clima",
            Theme::Lut => "lut",
            Theme::Mundo => "mundo",
            Theme::Gameplay => "gameplay",
            Theme::Cheats => "cheats",
            Theme::Outros => "outros",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Theme::Roupas => "Roupas",
            Theme::Cabelos => "Cabelos",
            Theme::Npc => "NPCs",
            Theme::Estetica => "Estética (V)",
            Theme::Veiculos => "Veículos",
            Theme::Armas => "Armas",
            Theme::Clima => "Clima",
            Theme::Lut => "LUT",
            Theme::Mundo => "Mundo",
            Theme::Gameplay => "Gameplay",
            Theme::Cheats => "Cheats",
            Theme::Outros => "Outros",
        }
    }
    pub fn all() -> [Theme; 12] {
        [
            Theme::Roupas, Theme::Cabelos, Theme::Npc, Theme::Estetica, Theme::Veiculos,
            Theme::Armas, Theme::Clima, Theme::Lut, Theme::Mundo, Theme::Gameplay, Theme::Cheats, Theme::Outros,
        ]
    }
    pub fn from_slug(s: &str) -> Theme {
        Theme::all().into_iter().find(|t| t.slug() == s).unwrap_or(Theme::Outros)
    }
}

/// Tabela de palavras-chave por tema (substring, minúsculas). Ordem = prioridade no desempate:
/// temas mais específicos primeiro (nomes próprios de NPC/veículo ganham de termos genéricos).
/// Nomes de NPC = sinal FORTE: se aparecem, o mod é sobre aquele personagem (override do hit-count).
/// (Jackie fica de fora de propósito — "Jackie's Arch" é a MOTO dele, não a aparência.)
const NPC_NAMES: &[&str] = &[
    "panam", "judy", "river ward", " river ", "johnny", "takemura", "kerry", "rogue", "goro", "misty", "claire", "viktor",
];

const KW: &[(Theme, &[&str])] = &[
    (Theme::Npc, &["npc", "companion"]),
    (Theme::Veiculos, &["vehicle", "quadra", "caliburn", "rayfield", "porsche", "kusanagi", "yaiba", "brennan", "apollo", "mizutani", "thorton", "motorcycle", " moto", "motorbike", " bike", "paintjob", "paint job", "reskin car", " car ", "supercar", "aerondight", "mordred", " arch ", "nazare"]),
    (Theme::Armas, &["weapon", "katana", "pistol", "rifle", "shotgun", "revolver", "scope", "sword", "blade", "lizzie", "malorian", "scalpel", "skippy", "yasha", "copperhead", "nekomata", "iconic weapon", "gun "]),
    (Theme::Cabelos, &["hair", "hairstyle", "haircut", "beard", "ponytail", "braid", "cabelo"]),
    (Theme::Roupas, &["jacket", "coat", "trench", "outfit", "clothing", "shirt", "pants", "trousers", "dress", "apparel", "shorts", "hotpants", "vest", "jumpsuit", "bodysuit", "wardrobe", "corpocore", "nomadcore", "netrunner jacket", "roupa", "jaqueta"]),
    (Theme::Lut, &["lut", "reshade", "nova", "preem", "color grade", "colorgrade", "gamma", "tonemap", "cinematic", "grading", "color preset"]),
    (Theme::Clima, &["weather", "lighting", "timecycle", "climate", "clima", "rain", "fog", "skybox"]),
    (Theme::Mundo, &["poster", "billboard", " sign", "signage", " tv ", "remaster", "environment", "props", "street", "building", "neon", "advertis", "world textures"]),
    (Theme::Estetica, &["complexion", "skin", " face", "makeup", "freckle", "scar", "tattoo", " eye", "eyebrow", "brow", "body texture", "pele", "rosto", "smoother"]),
    (Theme::Gameplay, &["balance", "difficulty", "combat", "economy", " ai ", "cyberware", "stamina", "loot", "spawn rate", "overhaul", "gameplay", "rebalance", "perk"]),
    (Theme::Cheats, &["cheat", "trainer", "godmode", "god mode", "unlimited", "infinite", "unlock all", "money"]),
];

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub theme: Theme,
    pub confidence: u8, // 0..=95
    pub reason: String, // quais palavras casaram
}

/// "Feno" pra busca: nome do mod + nomes de todos os arquivos, minúsculo, com bordas em espaço
/// pra os matches de " car "/" tv " pegarem palavra inteira.
fn haystack(report: &ModReport) -> String {
    let mut s = String::with_capacity(256);
    s.push(' ');
    s.push_str(&report.name.to_ascii_lowercase());
    for f in &report.files {
        s.push(' ');
        s.push_str(&f.rel.to_string_lossy().to_ascii_lowercase());
    }
    s.push(' ');
    // normaliza separadores p/ espaço (underscores/hifens viram fronteira de palavra)
    s.chars().map(|c| if c == '_' || c == '-' || c == '/' || c == '.' { ' ' } else { c }).collect()
}

/// Sugere um tema por palavras-chave. Conta hits por tema; o de mais hits vence (desempate = ordem da KW).
pub fn suggest(report: &ModReport) -> Suggestion {
    let hay = haystack(report);
    // nome de NPC = override forte (mod de personagem nomeado vence face/pele genérico)
    let npc: Vec<&str> = NPC_NAMES.iter().copied().filter(|w| hay.contains(*w)).collect();
    if !npc.is_empty() {
        return Suggestion { theme: Theme::Npc, confidence: 90, reason: format!("personagem: {}", npc.join(", ")) };
    }
    let mut best: Option<(Theme, Vec<&str>)> = None;
    for (theme, words) in KW {
        let hits: Vec<&str> = words.iter().copied().filter(|w| hay.contains(*w)).collect();
        if hits.is_empty() {
            continue;
        }
        let better = match &best {
            None => true,
            Some((_, cur)) => hits.len() > cur.len(),
        };
        if better {
            best = Some((*theme, hits));
        }
    }
    match best {
        Some((theme, hits)) => {
            let conf = (hits.len() as u32 * 35).min(95) as u8;
            Suggestion { theme, confidence: conf, reason: format!("casou: {}", hits.join(", ")) }
        }
        None => Suggestion { theme: Theme::Outros, confidence: 0, reason: "nenhuma palavra-chave casou".into() },
    }
}

/// Estado de UM mod na página (o CONTRATO com a UI in-game).
#[derive(Debug, Clone)]
pub struct ModState {
    pub name: String,
    pub theme: Theme,
    pub active: bool,
    pub favorite: bool,
    pub order: i32,
    pub variant: String, // p/ packs com variantes (ex.: cor da pintura); "" = única
}

fn state_path(game: &Path) -> std::path::PathBuf {
    game.join(".cp77-mods").join("bwms-mods.json")
}

fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            _ => o.push(c),
        }
    }
    o
}

/// Serializa a lista de estados em JSON (array de objetos) — pretty o suficiente p/ humano ler.
pub fn to_json(states: &[ModState]) -> String {
    let mut s = String::from("[\n");
    for (i, m) in states.iter().enumerate() {
        s.push_str(&format!(
            "  {{\"name\":\"{}\",\"theme\":\"{}\",\"active\":{},\"favorite\":{},\"order\":{},\"variant\":\"{}\"}}",
            json_escape(&m.name), m.theme.slug(), m.active, m.favorite, m.order, json_escape(&m.variant)
        ));
        s.push_str(if i + 1 < states.len() { ",\n" } else { "\n" });
    }
    s.push(']');
    s
}

pub fn save_states(game: &Path, states: &[ModState]) -> std::io::Result<()> {
    let p = state_path(game);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&p, to_json(states))
}

// ---- leitura tolerante (parse na mão dos campos do nosso próprio formato) ----

/// extrai o valor string de `"key":"..."` a partir de `obj` (o trecho entre { }).
fn field_str(obj: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":\"");
    let start = obj.find(&pat)? + pat.len();
    let rest = &obj[start..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(match n {
                        'n' => '\n',
                        other => other,
                    });
                }
            }
            '"' => return Some(out),
            _ => out.push(c),
        }
    }
    None
}

fn field_bool(obj: &str, key: &str) -> Option<bool> {
    let pat = format!("\"{key}\":");
    let start = obj.find(&pat)? + pat.len();
    let rest = obj[start..].trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn field_int(obj: &str, key: &str) -> Option<i32> {
    let pat = format!("\"{key}\":");
    let start = obj.find(&pat)? + pat.len();
    let rest = obj[start..].trim_start();
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Parser tolerante do nosso JSON (array de objetos planos). Quebra por '}' — seguro porque
/// não há '}' aninhado nos nossos objetos.
pub fn from_json(s: &str) -> Vec<ModState> {
    let mut out = Vec::new();
    for chunk in s.split('}') {
        if let Some(open) = chunk.find('{') {
            let obj = &chunk[open + 1..];
            if let Some(name) = field_str(obj, "name") {
                out.push(ModState {
                    name,
                    theme: Theme::from_slug(&field_str(obj, "theme").unwrap_or_default()),
                    active: field_bool(obj, "active").unwrap_or(false),
                    favorite: field_bool(obj, "favorite").unwrap_or(false),
                    order: field_int(obj, "order").unwrap_or(0),
                    variant: field_str(obj, "variant").unwrap_or_default(),
                });
            }
        }
    }
    out
}

pub fn load_states(game: &Path) -> Vec<ModState> {
    match std::fs::read_to_string(state_path(game)) {
        Ok(s) => from_json(&s),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::{FileEntry, FileKind, ModClass, Compat, ModReport};
    use std::path::PathBuf;

    fn rep(name: &str, files: &[&str]) -> ModReport {
        ModReport {
            name: name.into(),
            class: ModClass::PureContent,
            compat: Compat::Universal,
            files: files.iter().map(|f| FileEntry { rel: PathBuf::from(f), kind: FileKind::Archive }).collect(),
            deps: vec![],
            risks: vec![],
        }
    }

    #[test]
    fn sugere_veiculo() {
        assert_eq!(suggest(&rep("Quadra Turbo R V-Tec E3 Recolor", &[])).theme, Theme::Veiculos);
        assert_eq!(suggest(&rep("Free Black Caliburn Reskin", &[])).theme, Theme::Veiculos);
        assert_eq!(suggest(&rep("Jackie's Arch Recolor", &["arch_nazare.archive"])).theme, Theme::Veiculos);
    }
    #[test]
    fn sugere_npc_e_estetica() {
        assert_eq!(suggest(&rep("Panam scar and freckles", &[])).theme, Theme::Npc);
        assert_eq!(suggest(&rep("4K Detailed Complexion makeup", &[])).theme, Theme::Estetica);
    }
    #[test]
    fn sugere_roupa_arma_mundo() {
        assert_eq!(suggest(&rep("Black Leather Trenchcoat", &[])).theme, Theme::Roupas);
        assert_eq!(suggest(&rep("Arasaka Thermal Katana Errata Replacer", &[])).theme, Theme::Armas);
        assert_eq!(suggest(&rep("Posters Remastered", &[])).theme, Theme::Mundo);
    }
    #[test]
    fn outros_quando_sem_pista() {
        assert_eq!(suggest(&rep("zzz mystery blob", &[])).theme, Theme::Outros);
    }
    #[test]
    fn json_roundtrip() {
        let states = vec![
            ModState { name: "Caliburn \"Red\"".into(), theme: Theme::Veiculos, active: true, favorite: true, order: 0, variant: "vermelho".into() },
            ModState { name: "Posters".into(), theme: Theme::Mundo, active: false, favorite: false, order: 3, variant: "".into() },
        ];
        let js = to_json(&states);
        let back = from_json(&js);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].name, "Caliburn \"Red\"");
        assert_eq!(back[0].theme, Theme::Veiculos);
        assert!(back[0].active && back[0].favorite);
        assert_eq!(back[0].variant, "vermelho");
        assert_eq!(back[1].order, 3);
        assert!(!back[1].active);
    }
}
