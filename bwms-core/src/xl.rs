//! Parser do formato `.xl` do ArchiveXL (subconjunto YAML), zero-dep (só `std`).
//!
//! O `.xl` é um YAML que diz ao ArchiveXL o que fazer ao carregar um mod. Este módulo lê o
//! arquivo num modelo TIPADO (`XlFile`) fiel à fonte C++ do ArchiveXL
//! (`cp2077-archive-xl/src/App/Extensions/*/Config.cpp`). Cobre as seções NÚCLEO usadas pela
//! esmagadora maioria dos mods:
//!   - `factories:`            (FactoryIndex/Config.cpp)  — scalar OU lista de paths .csv
//!   - `resource.patch:`       (ResourcePatch/Config.cpp) — path → scalar | seq | {props,targets}, tag `!exclude`
//!   - `resource.link:`        (ResourceLink/Config.cpp)  — alvo → scalar | seq de fontes
//!   - `localization.{onscreens,subtitles,lipmaps,vomaps}` + `extend` (Localization/Config.cpp)
//!
//! Seções ainda NÃO tipadas (streaming, resource.copy, customNodes, garment, animation, ...) NÃO
//! são descartadas em silêncio: seus nomes de chave de topo entram em `XlFile::other_sections`,
//! pra a ferramenta poder avisar "tem coisa aqui que ainda não processo".
//!
//! O parser YAML é um subconjunto deliberado (o que `.xl` usa): mapas/sequências por indentação,
//! sequências em bloco (`- item`), sequências inline (`[ a, b ]`), tags de nó (`!exclude`),
//! comentários (`# ...`) e aspas opcionais. NÃO é um YAML completo (sem âncoras, multi-doc,
//! flow-maps, escapes de aspas) — de propósito.

use std::collections::HashMap;

// ============================ modelo YAML genérico ============================

/// Valor YAML do subconjunto. Ordem preservada nos mapas (chaves podem repetir, como no YAML).
#[derive(Debug, Clone, PartialEq)]
pub enum Yaml {
    Scalar(String),
    Seq(Vec<Yaml>),
    Map(Vec<(String, Yaml)>),
    /// nó com tag, ex.: `!exclude [ a, b ]` → Tagged("exclude", Seq([...]))
    Tagged(String, Box<Yaml>),
}

impl Yaml {
    /// scalar (atravessa tag) → &str
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Yaml::Scalar(s) => Some(s),
            Yaml::Tagged(_, b) => b.as_str(),
            _ => None,
        }
    }
    /// sequência (atravessa tag)
    pub fn as_seq(&self) -> Option<&[Yaml]> {
        match self {
            Yaml::Seq(v) => Some(v),
            Yaml::Tagged(_, b) => b.as_seq(),
            _ => None,
        }
    }
    /// mapa (atravessa tag)
    pub fn as_map(&self) -> Option<&[(String, Yaml)]> {
        match self {
            Yaml::Map(m) => Some(m),
            Yaml::Tagged(_, b) => b.as_map(),
            _ => None,
        }
    }
    /// valor de uma chave no mapa (atravessa tag)
    pub fn get(&self, key: &str) -> Option<&Yaml> {
        self.as_map()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    /// tag do nó, se houver (`!exclude` → "exclude")
    pub fn tag(&self) -> Option<&str> {
        match self {
            Yaml::Tagged(t, _) => Some(t),
            _ => None,
        }
    }
    /// coerção "um-ou-vários": scalar→[s], seq→[itens scalar]. (O padrão do `.xl`.)
    pub fn to_str_list(&self) -> Vec<String> {
        match self {
            Yaml::Scalar(s) => vec![s.clone()],
            Yaml::Seq(v) => v.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            Yaml::Tagged(_, b) => b.to_str_list(),
            Yaml::Map(_) => vec![],
        }
    }
}

// ============================ parser (linhas + recursão) ============================

struct Line {
    indent: usize,
    text: String,
}

/// Tira comentário YAML: `#` inicia comentário se for início de linha (após indent) ou precedido
/// por espaço/tab. (Paths do `.xl` não têm espaço+`#`, então é seguro pra esse subconjunto.)
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut prev_space = true; // início de linha conta como "após espaço"
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'#' && prev_space {
            return &line[..i];
        }
        prev_space = b == b' ' || b == b'\t';
    }
    line
}

/// Quebra em linhas significativas (sem comentário, sem brancas), com indent contado em espaços.
fn prelex(input: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for raw in input.lines() {
        let no_comment = strip_comment(raw);
        let indent = no_comment.chars().take_while(|c| *c == ' ').count();
        let text = no_comment[indent..].trim_end();
        if text.is_empty() {
            continue;
        }
        out.push(Line { indent, text: text.to_string() });
    }
    out
}

/// tira aspas simples/duplas externas, se houver.
fn unquote(s: &str) -> String {
    let s = s.trim();
    let b = s.as_bytes();
    if s.len() >= 2 && ((b[0] == b'"' && b[s.len() - 1] == b'"') || (b[0] == b'\'' && b[s.len() - 1] == b'\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// "chave: resto" → (chave, resto). A chave termina no PRIMEIRO `:` seguido de espaço ou fim.
/// (Paths do `.xl` usam `\`/`.`, nunca `:`, então não há ambiguidade.)
fn split_key(s: &str) -> Option<(String, String)> {
    let bytes = s.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ') {
            let key = unquote(s[..i].trim());
            let rest = s[i + 1..].trim().to_string();
            return Some((key, rest));
        }
    }
    None
}

/// valor inline (na mesma linha): tag `!x`, flow-seq `[..]`, ou scalar.
fn parse_inline(s: &str) -> Result<Yaml, String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('!') {
        let mut parts = rest.splitn(2, ' ');
        let tag = parts.next().unwrap_or("").to_string();
        let val = parts.next().unwrap_or("").trim();
        let inner = if val.is_empty() { Yaml::Scalar(String::new()) } else { parse_inline(val)? };
        return Ok(Yaml::Tagged(tag, Box::new(inner)));
    }
    if s.starts_with('[') {
        return parse_flow_seq(s);
    }
    Ok(Yaml::Scalar(unquote(s)))
}

/// `[ a, b, c ]` / `[]` → Seq de scalars (sem aninhamento — `.xl` não usa).
fn parse_flow_seq(s: &str) -> Result<Yaml, String> {
    let inner = s
        .strip_prefix('[')
        .and_then(|x| x.strip_suffix(']'))
        .ok_or_else(|| format!("sequência inline malformada: '{s}'"))?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Yaml::Seq(vec![]));
    }
    Ok(Yaml::Seq(inner.split(',').map(|p| Yaml::Scalar(unquote(p.trim()))).collect()))
}

/// Estado do parser recursivo. Carrega o registro de ÂNCORAS (`&nome`) p/ resolver ALIASES
/// (`*nome`) — usados de verdade nos `.xl` de customização (EyesFix/BrowsFix/LashesFix).
struct Parser {
    lines: Vec<Line>,
    pos: usize,
    anchors: HashMap<String, Yaml>,
}

impl Parser {
    fn parse_block(&mut self, indent: usize) -> Result<Yaml, String> {
        if self.pos >= self.lines.len() {
            return Ok(Yaml::Scalar(String::new()));
        }
        if self.lines[self.pos].text.starts_with('-') {
            self.parse_seq(indent)
        } else {
            self.parse_map(indent)
        }
    }

    fn parse_map(&mut self, indent: usize) -> Result<Yaml, String> {
        let mut entries = Vec::new();
        while self.pos < self.lines.len() {
            let line = &self.lines[self.pos];
            if line.indent < indent {
                break;
            }
            if line.indent > indent {
                return Err(format!("indentação inesperada em '{}'", line.text));
            }
            let text = line.text.clone();
            let (key, rest) = split_key(&text).ok_or_else(|| format!("esperava 'chave:' em '{text}'"))?;
            self.pos += 1;
            let val = self.parse_value(&rest, indent)?;
            entries.push((key, val));
        }
        Ok(Yaml::Map(entries))
    }

    fn parse_seq(&mut self, indent: usize) -> Result<Yaml, String> {
        let mut items = Vec::new();
        while self.pos < self.lines.len() {
            let line = &self.lines[self.pos];
            if line.indent < indent {
                break;
            }
            if line.indent > indent {
                return Err(format!("indentação inesperada em '{}'", line.text));
            }
            if !line.text.starts_with('-') {
                break; // fim da sequência (uma chave de mapa no mesmo indent)
            }
            let after = line.text[1..].trim_start().to_string();
            self.pos += 1;
            if after.is_empty() {
                items.push(self.parse_nested_or_empty(indent)?);
            } else if after.starts_with('&') || after.starts_with('*') || after.starts_with('[') || after.starts_with('!') {
                items.push(self.parse_value(&after, indent)?);
            } else if let Some((k, r)) = split_key(&after) {
                // "- chave: ..." → item-mapa; chaves seguintes mais indentadas pertencem a ele
                let mut map = Vec::new();
                let v = self.parse_value(&r, indent)?;
                map.push((k, v));
                while self.pos < self.lines.len()
                    && self.lines[self.pos].indent > indent
                    && !self.lines[self.pos].text.starts_with('-')
                {
                    let l2t = self.lines[self.pos].text.clone();
                    let li = self.lines[self.pos].indent;
                    let (k2, r2) = split_key(&l2t).ok_or_else(|| format!("esperava chave em '{l2t}'"))?;
                    self.pos += 1;
                    let v2 = self.parse_value(&r2, li)?;
                    map.push((k2, v2));
                }
                items.push(Yaml::Map(map));
            } else {
                items.push(parse_inline(&after)?);
            }
        }
        Ok(Yaml::Seq(items))
    }

    /// Resolve o valor de uma chave/item a partir do `rest` inline + bloco que segue.
    /// Trata aliases (`*nome`), âncoras (`&nome [valor]`), tags/flow/scalar inline e bloco aninhado.
    fn parse_value(&mut self, rest: &str, indent: usize) -> Result<Yaml, String> {
        let rest = rest.trim();
        if let Some(name) = rest.strip_prefix('*') {
            let name = name.trim();
            return self
                .anchors
                .get(name)
                .cloned()
                .ok_or_else(|| format!("alias *{name} sem âncora correspondente"));
        }
        if let Some(after) = rest.strip_prefix('&') {
            let mut it = after.splitn(2, ' ');
            let name = it.next().unwrap_or("").trim().to_string();
            let inline = it.next().unwrap_or("").trim();
            let val = if inline.is_empty() {
                self.parse_nested_or_empty(indent)?
            } else {
                parse_inline(inline)?
            };
            self.anchors.insert(name, val.clone());
            return Ok(val);
        }
        if !rest.is_empty() {
            return parse_inline(rest);
        }
        self.parse_nested_or_empty(indent)
    }

    /// valor de uma chave sem inline: bloco mais indentado, seq em bloco no mesmo indent, ou vazio.
    fn parse_nested_or_empty(&mut self, indent: usize) -> Result<Yaml, String> {
        if self.pos < self.lines.len() && self.lines[self.pos].indent > indent {
            let child = self.lines[self.pos].indent;
            self.parse_block(child)
        } else if self.pos < self.lines.len()
            && self.lines[self.pos].indent == indent
            && self.lines[self.pos].text.starts_with('-')
        {
            self.parse_seq(indent)
        } else {
            Ok(Yaml::Scalar(String::new()))
        }
    }
}

/// Parseia um documento YAML (subconjunto `.xl`) na árvore genérica `Yaml`.
pub fn parse_yaml(input: &str) -> Result<Yaml, String> {
    let input = input.replace('\t', "  ");
    let lines = prelex(&input);
    if lines.is_empty() {
        return Ok(Yaml::Map(vec![]));
    }
    let base = lines[0].indent;
    let mut p = Parser { lines, pos: 0, anchors: HashMap::new() };
    let v = p.parse_block(base)?;
    if p.pos != p.lines.len() {
        return Err(format!("conteúdo não consumido a partir de '{}'", p.lines[p.pos].text));
    }
    Ok(v)
}

// ============================ modelo tipado do .xl ============================

/// Um patch de recurso: estampa `props` do recurso `patch` nos recursos-alvo `includes`
/// (ou os remove via `excludes`, tag `!exclude`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourcePatch {
    pub patch: String,
    pub props: Vec<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
}

/// Um link de recurso. **Direção depende da FORMA no YAML (achado 2026-07-13, RE de
/// `ResourceLink/Config.cpp` + `Extension.cpp` real do ArchiveXL — não é uma escolha nossa, é
/// assimetria genuína no C++ upstream):**
/// - forma SCALAR (`target: source`): `target` (fake) resolve pra `source` (real) — 1 fonte.
/// - forma SEQUÊNCIA (`target: [source1, source2, ...]`): **INVERTIDO** — CADA item da lista
///   resolve pro `target` (o padrão real de uso, confirmado no fixture `Migration.xl`: o
///   `target` é o path NOVO/real, e a lista contém nomes ANTIGOS/legados que devem redirecionar
///   pra ele — é o caso de uso de MIGRAÇÃO, não "várias fontes candidatas pro mesmo alvo").
/// Rastreado via `is_sequence_form` (setado no parse, consumido em `apply_xl::build_apply_plan`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceLink {
    pub target: String,
    pub sources: Vec<String>,
    pub is_sequence_form: bool,
}

/// Um escopo de recurso (`resource.scope`): o recurso `resource` define o ESCOPO dos `targets`
/// (ResourceMeta/Config.cpp `LoadScopes`). Alvos = scalar OU lista.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceScope {
    pub resource: String,
    pub targets: Vec<String>,
}

/// Uma cópia de recurso (`resource.copy`): copia `source` para cada `targets`
/// (ResourceLink/Config.cpp). Alvos = scalar OU lista.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceCopy {
    pub source: String,
    pub targets: Vec<String>,
}

/// Um conserto de recurso (`resource.fix`): no recurso `resource`, remapeia nomes (`names`:
/// nome→nome), paths (`paths`: path→path) e parâmetros de contexto (`context`: nome→valor).
/// (ResourceMeta/Config.cpp `LoadFixes`.)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceFix {
    pub resource: String,
    pub names: Vec<(String, String)>,
    pub paths: Vec<(String, String)>,
    pub context: Vec<(String, String)>,
}

/// Um grupo de localização (onscreens/subtitles/lipmaps/vomaps): idioma → paths.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalizationGroup {
    pub kind: String,
    pub entries: Vec<(String, Vec<String>)>,
}

/// `player.bodyTypes` (`PuppetState/Config.cpp`): lista de nomes de tipo de corpo (scalar OU seq).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PuppetStateConfig {
    pub body_types: Vec<String>,
}

/// `customizations.{male,female}` (`Customization/Config.cpp`): paths de opção de customização
/// por gênero (cada um scalar OU seq — `ReadOptions` idêntica pros dois).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CustomizationConfig {
    pub male_options: Vec<String>,
    pub female_options: Vec<String>,
}

/// Uma entrada de `animations[]` (`Animation/Config.cpp::AnimationEntry`). `entity`: nomes de
/// entidade-alvo (scalar OU seq — **achado de RE: o C++ upstream tem um bug de copy-paste,
/// `else if (entityNode.IsScalar())` deveria ser `IsSequence()`, então a forma-lista NUNCA
/// preenche no binário real; implementamos CORRETO aqui pro nosso parser ter valor prático,
/// documentando a divergência**). `set`: nome do anim set (obrigatório). `vars`: **outro bug
/// upstream** (`if (variablesNode.IsScalar())` checa o nó ERRADO — o pai, não o item do loop —
/// `variables` fica SEMPRE vazio no binário real; idem, implementamos correto). `priority`
/// (default 128) e `component` (default "root") opcionais.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationEntry {
    pub entities: Vec<String>,
    pub set: String,
    pub variables: Vec<String>,
    pub priority: u8,
    pub component: String,
}

impl Default for AnimationEntry {
    fn default() -> Self {
        AnimationEntry { entities: vec![], set: String::new(), variables: vec![], priority: 128, component: "root".to_string() }
    }
}

/// Vetor 3D (`scale`, sempre `[x, y, z]` — exatamente 3 floats).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Vetor 4D (`position`). **Achado de RE:** o `.W` final é SEMPRE forçado a `0` no C++ real
/// (`WorldStreaming/Config.cpp`), mesmo quando a lista YAML tem 4 valores — o 4º valor (índice 3)
/// é lido em `positionValues[3]` mas NUNCA usado; só serve pra decidir se a forma de 4 é aceita
/// (nó de sector aceita 3 OU 4 valores; sub-node exige EXATAMENTE 4). Replicado aqui fielmente.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// Quaternion (`orientation`, sempre `[i, j, k, r]` — exatamente 4 floats, ordem i/j/k/r do RED4).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Quat {
    pub i: f32,
    pub j: f32,
    pub k: f32,
    pub r: f32,
}

/// Mutação de um SUB-node (`actorMutations`/`instanceMutations` dentro de um `nodeMutations[]`;
/// `WorldStreaming/Config.cpp::ParseSubMutations`). Exige `expectedActors`/`expectedInstances`
/// (a contagem esperada) presente e válido, senão a lista inteira é ignorada (fiel ao C++: sem
/// count válido, `ParseSubMutations` devolve `false` cedo e NADA é lido). `position` aqui exige
/// EXATAMENTE 4 valores (≠ do node-level, que aceita 3 ou 4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldSubNodeMutation {
    pub sub_node_index: i64,
    pub position: Option<Vec4>,
    pub orientation: Option<Quat>,
    pub scale: Option<Vec3>,
}

/// Mutação de um node de streaming sector (`nodeMutations[]`, `WorldStreaming/Config.cpp`).
/// `resource_path`/`appearance_name`/`record_id` cada um lido de VÁRIAS chaves-sinônimo (a última
/// presente vence — `resource`/`mesh`/`meshRef`/`material`/`effect`/`entityTemplate` pro path;
/// `appearance`/`appearanceName`/`meshAppearance` pro nome; `recordID`/`recordId`/`objectRecordId`
/// pro TweakDBID). `sub_node_mutations` vem de `actorMutations` OU `instanceMutations` (mutuamente
/// exclusivos na prática — o 2º chamado, se presente e válido, sobrescreve).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldNodeMutation {
    pub node_index: i64,
    pub node_type: String,
    pub position: Option<Vec4>,
    pub orientation: Option<Quat>,
    pub scale: Option<Vec3>,
    pub resource_path: Option<String>,
    pub appearance_name: Option<String>,
    pub record_id: Option<String>,
    pub nb_nodes_under_proxy_diff: Option<i32>,
    pub expected_sub_nodes: i64,
    pub sub_node_mutations: Vec<WorldSubNodeMutation>,
}

/// Deleção de um node de streaming sector (`nodeDeletions[]`). `sub_node_deletions` vem de
/// `actorDeletions` OU `instanceDeletions` (mesmo padrão sinônimo-sobrescreve de `WorldNodeMutation`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldNodeDeletion {
    pub node_index: i64,
    pub node_type: String,
    pub expected_sub_nodes: i64,
    pub sub_node_deletions: Vec<i64>,
}

/// Um streaming sector modificado (`streaming.sectors[]`). `expected_nodes` é obrigatório e usado
/// como LIMITE de sanidade pros índices de `nodeIndex` (fora do range = descartado, fiel ao C++).
/// Uma entrada só é mantida se tiver PELO MENOS 1 deleção ou 1 mutação (senão é ruído).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldSectorMod {
    pub path: String,
    pub expected_nodes: i64,
    pub node_deletions: Vec<WorldNodeDeletion>,
    pub node_mutations: Vec<WorldNodeMutation>,
}

/// `streaming.{blocks,sectors}` (`WorldStreaming/Config.cpp`) — o 7º e último item da lista
/// original de "11 seções" do parser `.xl` (as outras 4 citadas — Attachment/Mesh/Transmog/
/// InkSpawner — não têm `Config.cpp`/seção própria nenhuma, RE 2026-07-15 cont.60).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldStreamingSection {
    pub blocks: Vec<String>,
    pub sectors: Vec<WorldSectorMod>,
}

/// Máscara de chunks de mesh (`Garment/ChunkMask.hpp`, RE 2026-07-15): decide quais índices de
/// chunk do componente aparecem. `show=true` = seleção POSITIVA (a máscara final tem bit setado
/// exatamente nos chunks listados, forma `show: [...]`, sem inversão). `show=false` = seleção por
/// EXCLUSÃO (a máscara final é o COMPLEMENTO dos chunks listados — forma `hide: [...]`, sequência
/// nua, OU escalar cru já-computado — replica `ChunkMask::Set` bit a bit: OR dos `1<<chunk`, depois
/// `mask = ~mask` se `!show && mask != 0`). Confirma e generaliza o achado da RE do full-body:
/// `chunkMask=0xFFFFFFFFFFFFFF1F` em `t0_000_pwa_fpp__01_ca_pale` = `hide: [5, 6, 7]` por esta
/// fórmula exata (`~((1<<5)|(1<<6)|(1<<7)) = 0xFFFFFFFFFFFFFF1F`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkMask {
    pub show: bool,
    pub mask: u64,
}

impl ChunkMask {
    /// Constrói a partir de uma lista de índices de chunk (0-63), replicando `ChunkMask::Set`.
    fn from_chunks(show: bool, chunks: &[u8]) -> Self {
        let mut mask: u64 = 0;
        for &c in chunks {
            mask |= 1u64 << (c & 63);
        }
        if !show && mask != 0 {
            mask = !mask;
        }
        ChunkMask { show, mask }
    }
}

/// Tabela BUILT-IN de tags do ArchiveXL (`OverrideTagManager::OverrideTagManager()`,
/// `enablers/ArchiveXL/src/App/Extensions/Garment/Tags.cpp:3-156`, RE 2026-08-11, item
/// `PENDENCIAS-UNIFICADAS.md` #21/`GetTagManager`) — mapeia nome-de-tag PADRÃO do ArchiveXL (ex.
/// "hide_Torso", "HighHeels") pra lista de (prefixo-de-nome-de-parte, `ChunkMask`). Mods reais
/// referenciam essas 14 tags por NOME em vez de escrever a `ChunkMask` crua — sem esta tabela, o
/// BWMS só suporta `overrides.tags` AUTORAIS (`.xl` do próprio mod), não as tags de referência do
/// framework. Zero endereço nativo — dado estático puro, portado fielmente byte-a-byte da fonte
/// C++ real (constantes hardcoded, idênticas em qualquer build/plataforma). `None` = tag
/// desconhecida (mesmo comportamento de `GetOverrides` real: devolve definição vazia).
pub fn builtin_tag_overrides(tag: &str) -> Option<Vec<(&'static str, ChunkMask)>> {
    fn hide_all() -> ChunkMask {
        ChunkMask::from_chunks(false, &[])
    }
    fn hide(chunks: &[u8]) -> ChunkMask {
        ChunkMask::from_chunks(false, chunks)
    }
    fn show(chunks: &[u8]) -> ChunkMask {
        ChunkMask::from_chunks(true, chunks)
    }
    Some(match tag {
        "hide_Head" => vec![
            ("h0_", hide_all()),
            ("he_", hide_all()),
            ("heb_", hide_all()),
            ("ht_", hide_all()),
            ("hx_", hide_all()),
            ("i1_", hide_all()),
            ("beard", hide_all()),
            ("beard_", hide_all()),
            ("MorphTargetSkinnedMesh3637", hide_all()),
            ("MorphTargetSkinnedMesh6675", hide_all()),
            ("MorphTargetSkinnedMesh7243", hide_all()),
            ("MorphTargetSkinnedMesh7561", hide_all()),
        ],
        "hide_Arms" => vec![("a0_", hide_all()), ("left_arm", hide_all()), ("right_arm", hide_all())],
        "hide_Torso" => vec![
            ("n0_", hide_all()),
            ("tx_", hide_all()),
            ("t0_000_pma_base__full", hide(&[0, 1, 2, 3])),
            ("t0_000_pma_base__full_seamfix", hide_all()),
            ("t0_000_pwa_base__full", hide(&[0, 1, 2, 3])),
            ("t0_000_pwa_base__full_seamfix", hide_all()),
            ("t0_000_pwa_fpp__torso", hide(&[0, 1, 2, 3])),
            ("MorphTargetSkinnedMesh0531", hide_all()),
        ],
        "hide_LowerAbdomen" => vec![
            ("t0_000_pma_base__full", hide(&[3])),
            ("t0_000_pwa_base__full", hide(&[3])),
            ("t0_000_pwa_fpp__torso", hide(&[3])),
        ],
        "hide_UpperAbdomen" => vec![
            ("t0_000_pma_base__full", hide(&[2])),
            ("t0_000_pwa_base__full", hide(&[2])),
            ("t0_000_pwa_fpp__torso", hide(&[2])),
        ],
        "hide_CollarBone" => vec![
            ("t0_000_pma_base__full", hide(&[1])),
            ("t0_000_pwa_base__full", hide(&[1])),
            ("t0_000_pwa_fpp__torso", hide(&[1])),
        ],
        "hide_Chest" => vec![
            ("t0_000_pma_base__full", hide(&[0])),
            ("t0_000_pwa_base__full", hide(&[0])),
            ("t0_000_pwa_fpp__torso", hide(&[0])),
            ("MorphTargetSkinnedMesh0531", hide_all()),
        ],
        "hide_Legs" => vec![
            ("l0_", hide_all()),
            ("s0_", hide_all()),
            ("t0_000_pma_base__full", hide(&[4, 5, 6, 7])),
            ("t0_000_pwa_base__full", hide(&[4, 5, 6, 7])),
            ("t0_000_pwa_fpp__torso", hide(&[4, 5, 6, 7])),
        ],
        "hide_Thighs" => vec![
            ("t0_000_pma_base__full", hide(&[4])),
            ("t0_000_pwa_base__full", hide(&[4])),
            ("t0_000_pwa_fpp__torso", hide(&[4])),
        ],
        "hide_Calves" => vec![
            ("t0_000_pma_base__full", hide(&[5])),
            ("l0_000_pma_base__high_heels", hide(&[0])),
            ("l0_000_pma_base__flat_shoes", hide(&[0])),
            ("t0_000_pwa_base__full", hide(&[5])),
            ("t0_000_pwa_fpp__torso", hide(&[5])),
            ("l0_000_pwa_base__cs_flat", hide(&[0])),
            ("l0_000_pwa_base__high_heels", hide(&[0])),
            ("l0_000_pwa_base__flat_shoes", hide(&[0])),
        ],
        "hide_Ankles" => vec![
            ("s0_", hide_all()),
            ("t0_000_pma_base__full", hide(&[6])),
            ("l0_000_pma_base__high_heels", hide(&[1])),
            ("l0_000_pma_base__flat_shoes", hide(&[1])),
            ("t0_000_pwa_base__full", hide(&[6])),
            ("t0_000_pwa_fpp__torso", hide(&[6])),
            ("l0_000_pwa_base__cs_flat", hide(&[1])),
            ("l0_000_pwa_base__high_heels", hide(&[1])),
            ("l0_000_pwa_base__flat_shoes", hide(&[1])),
        ],
        "hide_Feet" => vec![
            ("t0_000_pma_base__full", hide(&[7])),
            ("l0_000_pma_base__high_heels", hide(&[2])),
            ("l0_000_pma_base__flat_shoes", hide(&[2])),
            ("t0_000_pwa_base__full", hide(&[7])),
            ("t0_000_pwa_fpp__torso", hide(&[7])),
            ("l0_000_pwa_base__cs_flat", hide(&[2])),
            ("l0_000_pwa_base__high_heels", hide(&[2])),
            ("l0_000_pwa_base__flat_shoes", hide(&[2])),
        ],
        "HighHeels" => vec![
            ("t0_000_pma_base__full", hide(&[5, 6, 7])),
            ("l0_000_pma_base__high_heels", show(&[0, 1, 2])),
            ("t0_000_pwa_base__full", hide(&[5, 6, 7])),
            ("t0_000_pwa_fpp__torso", hide(&[5, 6, 7])),
            ("l0_000_pwa_base__high_heels", show(&[0, 1, 2])),
        ],
        "FlatShoes" => vec![
            ("t0_000_pma_base__full", hide(&[5, 6, 7])),
            ("l0_000_pma_base__flat_shoes", show(&[0, 1, 2])),
            ("t0_000_pwa_base__full", hide(&[5, 6, 7])),
            ("t0_000_pwa_fpp__torso", hide(&[5, 6, 7])),
            ("l0_000_pwa_base__flat_shoes", show(&[0, 1, 2])),
        ],
        _ => return None,
    })
}

/// `App::ComponentState` (`PENDENCIAS-UNIFICADAS.md` ArchiveXL item #22/`OverrideStateManager`,
/// RE 2026-08-11) — bookkeeping de compatibilidade MULTI-MOD por componente: cada `hash` (u64)
/// representa 1 FONTE/1 MOD de override distinta, permitindo COMBINAR overrides de MÚLTIPLAS
/// fontes no MESMO componente sem um sobrescrever o outro (`Garment/States.cpp`, algoritmo
/// portado byte-a-byte: hiding = AND cumulativo por hash, começando de `~0`; showing = OR
/// cumulativo por hash, começando de `0`; combinado = `original & todos_hiding | todos_showing`).
/// Quando um mod é desinstalado, `remove_override(hash)` some com APENAS as mudanças daquele
/// hash — os overrides de outros mods no mesmo componente permanecem intactos. Zero endereço
/// nativo — estrutura de dados pura.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComponentState {
    hiding_masks: std::collections::BTreeMap<u64, u64>,
    showing_masks: std::collections::BTreeMap<u64, u64>,
    appearance_overrides: std::collections::BTreeMap<u64, String>,
}

impl ComponentState {
    pub fn new() -> Self {
        Self::default()
    }

    /// `AddHidingChunkMaskOverride(hash, mask)` — AND cumulativo, inicia em `~0` (sem restrição).
    pub fn add_hiding_override(&mut self, hash: u64, mask: u64) {
        let entry = self.hiding_masks.entry(hash).or_insert(!0u64);
        *entry &= mask;
    }

    /// `AddShowingChunkMaskOverride(hash, mask)` — OR cumulativo, inicia em `0` (nada mostrado).
    pub fn add_showing_override(&mut self, hash: u64, mask: u64) {
        let entry = self.showing_masks.entry(hash).or_insert(0u64);
        *entry |= mask;
    }

    /// `RemoveChunkMaskOverride(hash)` — remove hiding E showing daquele hash (1 fonte/1 mod).
    /// Devolve `true` sempre (fiel à fonte real, que também sempre devolve `true`).
    pub fn remove_chunk_mask_override(&mut self, hash: u64) -> bool {
        self.hiding_masks.remove(&hash);
        self.showing_masks.remove(&hash);
        true
    }

    /// `AddAppearanceOverride(hash, appearance)` — 1 valor por hash (não cumulativo como as
    /// chunk masks — a fonte real também só guarda o ÚLTIMO valor setado por hash).
    pub fn add_appearance_override(&mut self, hash: u64, appearance: &str) {
        self.appearance_overrides.insert(hash, appearance.to_string());
    }

    pub fn remove_appearance_override(&mut self, hash: u64) -> bool {
        self.appearance_overrides.remove(&hash).is_some()
    }

    /// `GetOverriddenChunkMask(originalMask)` — combina TODOS os hashes registrados no componente.
    pub fn overridden_chunk_mask(&self, original_mask: u64) -> u64 {
        let mut mask = original_mask;
        for hiding in self.hiding_masks.values() {
            mask &= hiding;
        }
        for showing in self.showing_masks.values() {
            mask |= showing;
        }
        mask
    }

    pub fn has_overridden_chunk_mask(&self) -> bool {
        !self.hiding_masks.is_empty() || !self.showing_masks.is_empty()
    }

    /// `GetAppearanceOverridde()` — devolve `"default"` se vazio (typo preservado da fonte real
    /// só no nome do método C++, não no comportamento), senão o PRIMEIRO valor por ordem de
    /// inserção mais antiga (`m_appearanceNames.begin()`, um `Map` ordenado por chave/hash na
    /// fonte real — replicado aqui via `BTreeMap`, que também itera em ordem de chave).
    pub fn appearance_override(&self) -> &str {
        self.appearance_overrides.values().next().map(|s| s.as_str()).unwrap_or("default")
    }

    pub fn has_appearance_overrides(&self) -> bool {
        !self.appearance_overrides.is_empty()
    }

    pub fn is_overridden(&self) -> bool {
        self.has_overridden_chunk_mask() || self.has_appearance_overrides()
    }
}

/// Metade PURA de `App::ResourceState` (`PENDENCIAS-UNIFICADAS.md` ArchiveXL item #22,
/// `Garment/States.hpp:51-76`/`States.cpp:157-190`) — só a parte de "offset override" (`Core::Map
/// <uint64_t,int32_t> m_overridenOffsets`), que é bookkeeping puro (1 valor por hash-de-mod,
/// `GetOverriddenOffset` devolve a entrada de ordem-de-chave mais antiga se houver alguma, senão
/// `0`). As outras responsabilidades de `ResourceState` real (`LinkToAppearance`/
/// `GetActiveVariant*`) guardam um `DynamicAppearanceName` (struct rica com `Handle`s/`CName`s
/// nativos resolvidos por `ParseAppearance`) — fora de escopo aqui (precisa de RTTI/runtime, não é
/// lógica pura).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceStateOffsets {
    overridden_offsets: std::collections::BTreeMap<u64, i32>,
}

impl ResourceStateOffsets {
    pub fn new() -> Self {
        Self::default()
    }

    /// `AddOffsetOverride(hash, offset)` — 1 valor por hash (sobrescreve, não acumula).
    pub fn add_offset_override(&mut self, hash: u64, offset: i32) {
        self.overridden_offsets.insert(hash, offset);
    }

    /// `RemoveOffsetOverride(hash)`.
    pub fn remove_offset_override(&mut self, hash: u64) -> bool {
        self.overridden_offsets.remove(&hash).is_some()
    }

    /// `GetOverriddenOffset()` — devolve a 1ª entrada por ordem de chave (`m_overridenOffsets.
    /// begin()->second`, `Map` real é ordenado, `BTreeMap` replica a mesma ordem), ou `0` se vazio.
    pub fn overridden_offset(&self) -> i32 {
        self.overridden_offsets.values().next().copied().unwrap_or(0)
    }

    pub fn is_overridden(&self) -> bool {
        !self.overridden_offsets.is_empty()
    }
}

/// `App::EntityState` (`PENDENCIAS-UNIFICADAS.md` ArchiveXL item #22, `Garment/States.hpp:78-165`/
/// `States.cpp:222-388`, RE 2026-08-11) — agrega o bookkeeping de UMA entidade inteira: 1
/// `ComponentState` por NOME de componente (`Core::Map<Red::CName,SharedPtr<ComponentState>>
/// m_componentStates`, chave replicada aqui como hash `u64` de CName) + 1 `ResourceStateOffsets`
/// por recurso (`Core::Map<Red::ResourcePath,...> m_resourceStates`, chave = hash do path). Os
/// métodos "remove POR HASH" (`RemoveChunkMaskOverrides`/`RemoveAppearanceOverrides`/
/// `RemoveOffsetOverrides`/`RemoveAllOverrides`) iteram TODOS os component/resource states e
/// removem só as entradas daquele hash — é o mecanismo real que permite "mod desinstalado, só as
/// mudanças DAQUELE mod somem, outros mods intactos" (mesma regra já testada em `ComponentState::
/// remove_chunk_mask_override`, agora composta em escala de entidade inteira).
///
/// Fora de escopo (precisam de `Red::Entity*`/`Red::IComponent`/RTTI reais, não são lógica pura):
/// `ApplyChunkMaskOverride`/`ApplyAppearanceOverride`/`ApplyOffsetOverrides` (aplicam o resultado
/// já computado aqui num componente VIVO do motor), `SelectDynamicAppearance`/
/// `ToggleConditionalComponents`/`ApplyDynamicAppearance`/`Link*ToAppearance`/
/// `UpdateDynamicAttributes` (dependem de `DynamicAppearanceController`/entidade nativa).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntityState {
    component_states: std::collections::BTreeMap<u64, ComponentState>,
    resource_states: std::collections::BTreeMap<u64, ResourceStateOffsets>,
}

impl EntityState {
    pub fn new() -> Self {
        Self::default()
    }

    /// `GetComponentState(name)` — get-or-create (`m_componentStates.emplace(name,
    /// MakeShared<ComponentState>(name))` se ausente).
    pub fn component_state(&mut self, component_hash: u64) -> &mut ComponentState {
        self.component_states.entry(component_hash).or_insert_with(ComponentState::new)
    }

    /// `FindComponentState(name)` — só lê, nunca cria.
    pub fn find_component_state(&self, component_hash: u64) -> Option<&ComponentState> {
        self.component_states.get(&component_hash)
    }

    /// `GetResourceState(path)` — get-or-create.
    pub fn resource_state(&mut self, resource_hash: u64) -> &mut ResourceStateOffsets {
        self.resource_states.entry(resource_hash).or_insert_with(ResourceStateOffsets::new)
    }

    pub fn find_resource_state(&self, resource_hash: u64) -> Option<&ResourceStateOffsets> {
        self.resource_states.get(&resource_hash)
    }

    /// `AddChunkMaskOverride(hash, componentName, chunkMask, show)`.
    pub fn add_chunk_mask_override(&mut self, hash: u64, component_hash: u64, chunk_mask: u64, show: bool) {
        let cs = self.component_state(component_hash);
        if show {
            cs.add_showing_override(hash, chunk_mask);
        } else {
            cs.add_hiding_override(hash, chunk_mask);
        }
    }

    /// `RemoveChunkMaskOverrides(hash)` — varre TODOS os componentes, remove só `hash`.
    pub fn remove_chunk_mask_overrides(&mut self, hash: u64) {
        for cs in self.component_states.values_mut() {
            cs.remove_chunk_mask_override(hash);
        }
    }

    /// `AddAppearanceOverride(hash, componentName, appearance)`.
    pub fn add_appearance_override(&mut self, hash: u64, component_hash: u64, appearance: &str) {
        let cs = self.component_state(component_hash);
        cs.add_appearance_override(hash, appearance);
    }

    /// `RemoveAppearanceOverrides(hash)` — varre TODOS os componentes.
    pub fn remove_appearance_overrides(&mut self, hash: u64) {
        for cs in self.component_states.values_mut() {
            cs.remove_appearance_override(hash);
        }
    }

    /// `AddOffsetOverride(hash, resourcePath, offset)`.
    pub fn add_offset_override(&mut self, hash: u64, resource_hash: u64, offset: i32) {
        let rs = self.resource_state(resource_hash);
        rs.add_offset_override(hash, offset);
    }

    /// `RemoveOffsetOverrides(hash)` — varre TODOS os recursos.
    pub fn remove_offset_overrides(&mut self, hash: u64) {
        for rs in self.resource_states.values_mut() {
            rs.remove_offset_override(hash);
        }
    }

    pub fn offset_override(&self, resource_hash: u64) -> i32 {
        self.resource_states.get(&resource_hash).map(|rs| rs.overridden_offset()).unwrap_or(0)
    }

    /// `RemoveAllOverrides(hash)` — os 3 removes de uma vez (o desinstalar-mod real).
    pub fn remove_all_overrides(&mut self, hash: u64) {
        self.remove_chunk_mask_overrides(hash);
        self.remove_appearance_overrides(hash);
        self.remove_offset_overrides(hash);
    }

    pub fn component_count(&self) -> usize {
        self.component_states.len()
    }

    pub fn resource_count(&self) -> usize {
        self.resource_states.len()
    }
}

/// `App::OverrideStateManager` (`PENDENCIAS-UNIFICADAS.md` ArchiveXL item #22,
/// `Garment/States.hpp:167-194`/`States.cpp:790-946`, RE 2026-08-11) — indexa `EntityState`s por
/// **4 chaves diferentes**, cada uma um mapa PRÓPRIO pra um contexto de lookup diferente que o
/// pipeline de garment usa em pontos distintos do código: pelo ponteiro/ID da entidade
/// (`m_entityStates`, dono de verdade — `GetEntityState` cria aqui), por `ResourcePath` do
/// `.app` dinâmico (`m_entityStatesByPath`), por `GarmentProcessingContext*` (`m_entityStatesByProcessor`,
/// linkado via `LinkEntityToAssembler` — só linka se a entidade JÁ existir, nunca cria) e por
/// `uintptr_t` genérico (`m_entityStatesByPointer`, linkado via `LinkEntityToPointer` — cria se
/// preciso). Aqui todas as 4 chaves são `u64` opacos (o próprio C++ já trata `uint64_t aContext`
/// como um `Entity*` reinterpretado — `GetEntityState(uint64_t)` faz exatamente
/// `reinterpret_cast<Red::Entity*>(aContext)` — então usar `u64` em vez de ponteiro nativo aqui
/// não é divergência, é o MESMO tipo que a própria API pública já expõe).
///
/// Divergência de escopo consciente: a fonte real também registra automaticamente `entity.
/// templatePath.hash` como alias de path (3 variantes, sufixo `_0.app`/`_1.app`/`_2.app`) dentro
/// de `GetEntityState(Entity*)` — precisa ler o campo nativo `templatePath` da entidade, fora do
/// escopo de uma estrutura pura; aqui o alias de path é registrado explicitamente por quem chama
/// (`link_path`), que já tem o hash calculado por fora.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverrideStateManager {
    entity_states: std::collections::BTreeMap<u64, EntityState>,
    by_path: std::collections::BTreeMap<u64, u64>,
    by_processor: std::collections::BTreeMap<u64, u64>,
    by_pointer: std::collections::BTreeMap<u64, u64>,
}

impl OverrideStateManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// `GetEntityState(entity)` — get-or-create (dono real; as outras 3 chaves só apontam de
    /// volta pra cá).
    pub fn entity_state(&mut self, entity_key: u64) -> &mut EntityState {
        self.entity_states.entry(entity_key).or_insert_with(EntityState::new)
    }

    /// `FindEntityState(entity)` — só lê.
    pub fn find_entity_state(&self, entity_key: u64) -> Option<&EntityState> {
        self.entity_states.get(&entity_key)
    }

    pub fn find_entity_state_by_path(&self, path_hash: u64) -> Option<&EntityState> {
        self.by_path.get(&path_hash).and_then(|k| self.entity_states.get(k))
    }

    pub fn find_entity_state_by_processor(&self, processor_key: u64) -> Option<&EntityState> {
        self.by_processor.get(&processor_key).and_then(|k| self.entity_states.get(k))
    }

    pub fn find_entity_state_by_pointer(&self, pointer_key: u64) -> Option<&EntityState> {
        self.by_pointer.get(&pointer_key).and_then(|k| self.entity_states.get(k))
    }

    /// Registra um alias de path pra uma entidade JÁ existente (equivalente ao efeito colateral
    /// de `GetEntityState(Entity*)` real, mas com o hash de path calculado por fora).
    pub fn link_path(&mut self, entity_key: u64, path_hash: u64) {
        self.entity_states.entry(entity_key).or_insert_with(EntityState::new);
        self.by_path.insert(path_hash, entity_key);
    }

    /// `LinkEntityToAssembler(entity, processor)` — SÓ linka se a entidade já existir (fiel: a
    /// fonte real faz `m_entityStates.find(aEntity)` e só escreve dentro do `if (it != end())`,
    /// nunca cria). Devolve `false` se a entidade era desconhecida (nada foi linkado).
    pub fn link_processor(&mut self, entity_key: u64, processor_key: u64) -> bool {
        if self.entity_states.contains_key(&entity_key) {
            self.by_processor.insert(processor_key, entity_key);
            true
        } else {
            false
        }
    }

    /// `LinkEntityToPointer(entity, pointer)` — cria a entidade se preciso (fiel: o `else` do C++
    /// chama `GetEntityState(aEntity)`, get-or-create).
    pub fn link_pointer(&mut self, entity_key: u64, pointer_key: u64) {
        self.entity_states.entry(entity_key).or_insert_with(EntityState::new);
        self.by_pointer.insert(pointer_key, entity_key);
    }

    /// `ClearStates()`.
    pub fn clear_states(&mut self) {
        self.entity_states.clear();
        self.by_path.clear();
        self.by_processor.clear();
        self.by_pointer.clear();
    }

    pub fn entity_count(&self) -> usize {
        self.entity_states.len()
    }
}

/// `App::DynamicAppearanceController::IsDynamicValue` (ArchiveXL item #23, RE 2026-08-11) —
/// `Garment/Dynamic.cpp:674-697`, todas as 5 sobrecargas colapsam pra 1 checagem trivial: a
/// string começa com o marcador `*` (`DynamicValueMarker`).
pub fn is_dynamic_value(s: &str) -> bool {
    s.starts_with('*')
}

/// Resultado de `ProcessString` (ArchiveXL `DynamicAppearanceController::DynamicString`,
/// `Garment/Dynamic.cpp:442-556`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DynamicString {
    pub valid: bool,
    pub missed: bool,
    pub optional: bool,
    pub value: String,
    pub attributes: std::collections::BTreeSet<u64>,
}

/// `App::DynamicAppearanceController::ProcessString` (ArchiveXL #23, RE 2026-08-11, também
/// reusável pro item `#48`/`MeshExtension::ExpandResourcePath`, que chama a MESMA função —
/// confirmado lendo `Mesh/Extension.cpp:901-916`) — substitui marcadores `{attr}` embutidos numa
/// string por valores de 2 fontes (locais têm prioridade sobre globais), portado byte-a-byte:
/// pula o `*` inicial se presente; anda achando pares `{`/`}`, copia o texto entre eles pro
/// buffer de saída, hasheia o nome do atributo (FNV1a64, `bwms_hashes::fnv1a64` já provado) e
/// resolve o valor (local > global > `missed=true` se nenhum achar); ao final, se sobrou texto
/// não-processado (par `{`/`}` incompleto) OU nenhum atributo foi encontrado, o resultado fica
/// inválido (mesma regra exata do C++: `if (*str || attributes.empty()) return result;`); um `?`
/// final marca `optional=true` e é removido do valor. Limite de 512 bytes de saída (igual à
/// fonte, `buffer[MaxLength+1]`).
pub fn process_dynamic_string(
    local_attrs: &std::collections::HashMap<u64, String>,
    global_attrs: &std::collections::HashMap<u64, String>,
    input: &str,
) -> DynamicString {
    const MAX_LENGTH: usize = 512;
    let mut result = DynamicString::default();
    if input.is_empty() {
        return result;
    }
    let bytes = input.as_bytes();
    let mut idx = if bytes[0] == b'*' { 1 } else { 0 };
    let mut out: Vec<u8> = Vec::with_capacity(MAX_LENGTH + 1);

    while idx < bytes.len() {
        let attr_open = match bytes[idx..].iter().position(|&b| b == b'{') {
            Some(p) => idx + p,
            None => {
                while idx < bytes.len() && out.len() < MAX_LENGTH {
                    out.push(bytes[idx]);
                    idx += 1;
                }
                break;
            }
        };
        let attr_close = match bytes[attr_open..].iter().position(|&b| b == b'}') {
            Some(p) => attr_open + p,
            None => break,
        };
        while idx != attr_open && out.len() < MAX_LENGTH {
            out.push(bytes[idx]);
            idx += 1;
        }
        if out.len() == MAX_LENGTH {
            break;
        }
        idx = attr_close + 1;
        let attr_name = &input[attr_open + 1..attr_close];
        let attr_hash = bwms_hashes::fnv1a64(attr_name.as_bytes());
        result.attributes.insert(attr_hash);
        let value = local_attrs.get(&attr_hash).or_else(|| global_attrs.get(&attr_hash));
        match value {
            Some(v) => {
                let vbytes = v.as_bytes();
                let mut vi = 0;
                while vi < vbytes.len() && out.len() < MAX_LENGTH {
                    out.push(vbytes[vi]);
                    vi += 1;
                }
                if out.len() == MAX_LENGTH {
                    break;
                }
            }
            None => {
                result.missed = true;
            }
        }
    }

    if idx < bytes.len() || result.attributes.is_empty() {
        return result;
    }

    if out.last() == Some(&b'?') {
        result.optional = true;
        out.pop();
    }

    result.value = String::from_utf8_lossy(&out).into_owned();
    result.valid = true;
    result
}

/// `App::MeshExtension::ExpandResourcePath` (`PENDENCIAS-UNIFICADAS.md` ArchiveXL item #48,
/// `Mesh/Extension.cpp:900-925`, RE 2026-08-11) — resolve o path de um MATERIAL DINÂMICO,
/// reusando a MESMA engine de templating do garment (`process_dynamic_string`/`is_dynamic_value`,
/// item #23, já portados/testados) — confirma o achado já registrado no catálogo ("a implementação
/// avança os 2 itens de uma vez"). Diferença de `process_dynamic_string` cru: injeta 1 atributo
/// LOCAL pré-computado, `material` (`Red::CName("material")`, `Mesh/Extension.cpp:15` —
/// `MaterialAttr`, hash = `fnv1a64("material")`, confirmado que `Red::CName`/`FNV1a64` são a
/// mesma função por leitura direta de `Dynamic.cpp:494` — `aLocalAttrs.find(attr)` com `attr` um
/// `uint64_t` FNV1a64 cru contra um mapa chaveado por `CName`), mapeado pro NOME do material
/// sendo expandido; o resto dos atributos vem de `aState->GetContextAttrs()` (`global_attrs`,
/// fornecido pelo chamador).
///
/// Semântica fiel ao par `(ResourcePath, bool)` real:
/// - `path_str` não é dinâmico (`!IsDynamicValue`): devolve `(Some(path_str inalterado), false)`
///   — mesmo par `{aPath, false}` da fonte.
/// - processamento inválido (chave sem fechar etc.): `(None, false)` — mesmo `{{}, false}`.
/// - atributo faltando (`missed`): `(None, optional)` — mesmo `{{}, result.optional}`.
/// - sucesso: `(Some(path_expandido), optional)`.
///
/// Divergência de escopo consciente: a fonte real INTERNA o resultado via
/// `ResourcePathRegistry::RegisterPath` (produz um `ResourcePath` hash novo, side-effect num
/// registro global nativo); aqui devolvemos a STRING final — o chamador registra o hash pelo
/// mecanismo já provado do BWMS (`resource_path_hash`/`install_reslink`).
pub fn expand_resource_path(
    path_str: &str,
    material_name: &str,
    context_attrs: &std::collections::HashMap<u64, String>,
) -> (Option<String>, bool) {
    if !is_dynamic_value(path_str) {
        return (Some(path_str.to_string()), false);
    }
    let mut local_attrs = std::collections::HashMap::new();
    local_attrs.insert(bwms_hashes::fnv1a64(b"material"), material_name.to_string());
    let result = process_dynamic_string(&local_attrs, context_attrs, path_str);
    if !result.valid {
        return (None, false);
    }
    if result.missed {
        return (None, result.optional);
    }
    (Some(result.value), result.optional)
}

/// `App::ExtractName(aName, aOffset, aSize)` — porte da versão HASH-ONLY (`aRegister=false`,
/// `Garment/Dynamic.cpp:81-92`) — o ÚNICO caminho que `DynamicAppearanceRef` usa (nenhuma das
/// chamadas dentro do ctor real passa `aRegister=true`, confirmado lendo `Dynamic.cpp:194-266`
/// linha a linha). Substring vazia devolve `0` (equivalente ao `Red::CName` default), senão
/// FNV1a64 do trecho.
fn extract_name_hash(s: &str) -> u64 {
    if s.is_empty() {
        0
    } else {
        bwms_hashes::fnv1a64(s.as_bytes())
    }
}

/// `App::DynamicAppearanceRef` (`PENDENCIAS-UNIFICADAS.md` ArchiveXL itens #22/#23,
/// `Garment/Dynamic.cpp:194-266`, RE 2026-08-11) — parseia o NOME de um componente/aparência
/// (`aComponent->name`/`definition->name`, alimentado por `DynamicAppearanceController::
/// ParseReference`) que pode embutir uma lista de VARIANTES (`nome!v1!v2`) e/ou CONDIÇÕES
/// (`nome&c1&c2`, ou `nome!v1!v2&c1&c2` combinando os 2) — a sintaxe que `EntityState::
/// ToggleConditionalComponents`/`ApplyDynamicAppearance`/`ApplyAppearanceOverride`/
/// `ApplyChunkMaskOverride` (item #22) usam pra decidir qual componente/aparência condicional
/// está ativo dado o contexto do personagem. 100% hash puro (FNV1a64 via `extract_name_hash`,
/// zero registro no `CNamePool` — confirmado ser o único caminho usado aqui). `value` = hash da
/// string INTEIRA de entrada (equivalente ao `Red::CName` construído implicitamente pelo ctor
/// real, `value(aReference)` — a mesma convenção "`CName`'s hash == FNV1a64 da string" já
/// confirmada e usada em `process_dynamic_string`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DynamicAppearanceRef {
    pub value: u64,
    pub name: u64,
    pub variants: std::collections::BTreeSet<u64>,
    pub conditions: std::collections::BTreeSet<u64>,
    pub is_dynamic: bool,
    pub is_conditional: bool,
    pub weight: i8,
}

impl DynamicAppearanceRef {
    /// Porte byte-a-byte do ctor `DynamicAppearanceRef(Red::CName aReference)`.
    pub fn parse(reference: &str) -> Self {
        let mut r = DynamicAppearanceRef {
            value: extract_name_hash(reference),
            ..Default::default()
        };

        let Some(marker_pos) = reference.find(['!', '&']) else {
            // sem marcador nenhum -> não-dinâmico, `name` = a própria referência.
            r.name = r.value;
            return r;
        };

        r.is_dynamic = true;
        r.name = extract_name_hash(&reference[..marker_pos]);
        let mut rest = &reference[marker_pos..];

        if rest.len() > 1 {
            if rest.as_bytes()[0] == b'!' {
                rest = &rest[1..];
                loop {
                    if rest.is_empty() {
                        break;
                    }
                    match rest.find(['!', '&']) {
                        // marcador de condição logo no início -> para SEM consumir (deixa pro
                        // bloco de condições abaixo tratar), sem inserir variante nenhuma.
                        Some(0) if rest.as_bytes()[0] == b'&' => break,
                        None => {
                            r.variants.insert(extract_name_hash(rest));
                            break;
                        }
                        Some(p) => {
                            r.variants.insert(extract_name_hash(&rest[..p]));
                            if rest.as_bytes()[p] != b'!' {
                                // achou '&' -> encerra a lista de variantes, preserva o '&' pro
                                // bloco de condições.
                                rest = &rest[p..];
                                break;
                            }
                            // outro '!' -> mais uma variante segue.
                            rest = &rest[p + 1..];
                        }
                    }
                }
            }

            if rest.as_bytes().first() == Some(&b'&') {
                rest = &rest[1..];
                loop {
                    if rest.is_empty() {
                        break;
                    }
                    match rest.find('&') {
                        None => {
                            r.conditions.insert(extract_name_hash(rest));
                            break;
                        }
                        Some(p) => {
                            r.conditions.insert(extract_name_hash(&rest[..p]));
                            rest = &rest[p + 1..];
                        }
                    }
                }
            }

            r.weight = (if r.variants.is_empty() { 0 } else { 100 }) + (r.conditions.len() as i8);
            r.is_conditional = r.weight > 0;
        }

        r
    }

    /// `Match(aVariant)`.
    pub fn matches_variant(&self, variant_hash: u64) -> bool {
        self.variants.contains(&variant_hash)
    }

    /// `Match(aConditions)` — TODAS as condições próprias precisam estar presentes.
    pub fn matches_conditions(&self, conditions_present: &std::collections::BTreeSet<u64>) -> bool {
        self.conditions.iter().all(|c| conditions_present.contains(c))
    }

    /// `Match(aConditions, aOverrides)` — condição satisfeita se estiver em QUALQUER um dos 2 sets.
    pub fn matches_conditions_with_overrides(
        &self,
        conditions_present: &std::collections::BTreeSet<u64>,
        overrides: &std::collections::BTreeSet<u64>,
    ) -> bool {
        self.conditions.iter().all(|c| conditions_present.contains(c) || overrides.contains(c))
    }
}

/// `App::DynamicAppearanceName` (ArchiveXL item `#23`, `Garment/Dynamic.hpp:12-26`+
/// `Dynamic.cpp:96-192`, RE 2026-08-15 — a "struct irmã, não lida ainda" que a nota anterior do
/// item deixava pendente) — o VALOR real de uma aparência/componente que o motor está tentando
/// casar contra um `DynamicAppearanceRef` (o CRITÉRIO, já portado acima). Sintaxe:
/// `nome!variante[+parte[=valor]]...[%contexto][&condicao...]` — papel espelhado do `Ref`
/// (`nome!v1!v2&c1&c2`), mas aqui o que vem depois de `!` é UMA variante só (não uma lista),
/// opcionalmente composta de sub-partes (`+chave=valor` ou `+valorPosicional`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DynamicAppearanceName {
    pub value: u64,
    pub name: u64,
    pub variant: u64,
    /// `DynamicPartList = Map<CName,CName>` — chave de parte → valor de parte (hash de cada um).
    /// A chave `hash("variant")` é sempre inserida com o valor = `variant` inteiro (mesmo com
    /// sub-partes presentes — fiel à fonte, que nunca sobrescreve essa entrada específica).
    pub parts: std::collections::BTreeMap<u64, u64>,
    pub overrides: std::collections::BTreeSet<u64>,
    pub context: u64,
    pub is_dynamic: bool,
}

impl DynamicAppearanceName {
    /// Porte do ctor `DynamicAppearanceName(Red::CName aAppearance)`. Única divergência
    /// consciente do C++: aqui usamos um "end" (índice, nunca reatribui o ponteiro base) em vez
    /// de `std::string_view::remove_suffix`/`remove_prefix` cru — evita o UB que o C++ teria se
    /// `%`/`&` aparecessem ANTES do `!` (entrada malformada vinda de redscript ao vivo não pode
    /// derrubar o processo; C++ confiaria cegamente no invariante "sempre bem-formado").
    pub fn parse(appearance: &str) -> Self {
        let mut n = DynamicAppearanceName { value: extract_name_hash(appearance), ..Default::default() };

        let Some(marker_pos) = appearance.find('!') else {
            // sem `!` -> não-dinâmico, `name` = a própria aparência (== `value`).
            n.name = n.value;
            return n;
        };

        // 1) contexto: ÚLTIMO '%' na string ORIGINAL INTEIRA (busca antes de qualquer corte,
        // fiel ao C++ que faz `find_last_of` sobre o `str` ainda intacto). ACHADO DE RE: como o
        // parse de inteiro exige que TUDO até o fim da string seja numérico, o contexto só
        // resolve com sucesso se `%numero` for o FINAL LITERAL da string — a ordem válida é
        // "nome!variante&condicao%numero" (condição ANTES do contexto), não o inverso. Ver
        // testes `dynamicappearancename_condicao_antes_do_contexto_e_a_ordem_valida`/
        // `_contexto_antes_da_condicao_e_ordem_invalida_fica_zero`.
        let mut end = appearance.len();
        if let Some(context_pos) = appearance.rfind('%') {
            if let Ok(v) = appearance[context_pos + 1..].parse::<u64>() {
                n.context = v;
            }
            end = end.min(context_pos);
        }

        // 2) condição: PRIMEIRO '&' dentro do que sobrou após o corte de contexto.
        if let Some(cond_pos) = appearance[..end].find('&') {
            end = end.min(cond_pos);
        }

        // guarda de segurança (sem equivalente no C++, que confia no invariante de sempre vir
        // bem-formado): se o corte comeu até ANTES do próprio `!`, degrada com segurança em vez
        // de panicar num slice fora dos limites.
        if end <= marker_pos {
            n.name = n.value;
            return n;
        }

        n.is_dynamic = true;
        n.name = extract_name_hash(&appearance[..marker_pos]);

        let mut rest = &appearance[marker_pos + 1..end];

        if !rest.is_empty() {
            // `variant` = a string INTEIRA restante (não só até o 1º '+') — fiel ao C++, que
            // registra o trecho completo como CName ANTES de sequer olhar pro separador '+'.
            n.variant = extract_name_hash(rest);
            let variant_attr = bwms_hashes::fnv1a64(b"variant");
            n.parts.insert(variant_attr, n.variant);

            let mut part_counter: u8 = b'1';

            loop {
                if rest.is_empty() {
                    break;
                }
                match rest.find('+') {
                    Some(0) => {
                        // separador logo no início -> parte vazia, pula.
                        rest = &rest[1..];
                        continue;
                    }
                    found => {
                        let (marker2, skip) = match found {
                            Some(p) => (p, p + 1),
                            None => (rest.len(), rest.len()),
                        };

                        let assign_pos = rest.find('=').filter(|&ap| ap < marker2);
                        if let Some(ap) = assign_pos {
                            let part_name = extract_name_hash(&rest[..ap]);
                            let part_value = extract_name_hash(&rest[ap + 1..marker2]);
                            n.parts.insert(part_name, part_value);
                            // porte FIEL de `Dynamic.cpp:168` — `Red::FNV1a64(str.data(),
                            // markerPos - assignPos)`: offset a partir do INÍCIO de `rest` (não
                            // de `ap`), comprimento `marker2-ap`. NÃO é o hash de "chave=valor"
                            // nem de "chave" nem de "valor" isolados — é um trecho arbitrário
                            // `rest[0..marker2-ap]`. Cross-checado byte-a-byte contra a fonte
                            // real: parece anomalia genuína do próprio ArchiveXL (mesma classe
                            // de bug já documentada no projeto pro Journal/Animation do
                            // Codeware) — replicada aqui DE PROPÓSITO, não corrigida.
                            let quirky_len = (marker2 - ap).min(rest.len());
                            n.overrides.insert(bwms_hashes::fnv1a64(rest[..quirky_len].as_bytes()));
                        } else {
                            // forma posicional: chave sintética "variant.N" via hash de
                            // CONTINUAÇÃO (mesmo idioma de `tweak_db_id_derive` já usado no
                            // projeto) — `Red::FNV1a64(".{N}", seed=hash("variant"))`.
                            let seed = bwms_hashes::fnv1a64(b"variant");
                            let part_name = bwms_hashes::fnv1a64_seeded(&[b'.', part_counter], seed);
                            let part_value = extract_name_hash(&rest[..marker2]);
                            n.parts.insert(part_name, part_value);
                            part_counter = part_counter.wrapping_add(1);
                        }

                        rest = &rest[skip..];
                    }
                }
            }
        }

        n
    }
}

/// `DynamicAppearanceController::GetBaseAppearanceName` (`Dynamic.cpp:768-779`) — a substring
/// ANTES do primeiro marcador de QUALQUER tipo (`!`/`%`/`&`, `AllMarkers` na fonte real — mais
/// amplo que só `!`, diferente do campo `name` de `DynamicAppearanceName::parse`, que só corta
/// no `!`). String inteira devolvida se não houver nenhum marcador. Zero endereço nativo, zero
/// campo cru, implementação genuinamente nova (item `#23`).
pub fn get_base_appearance_name(name: &str) -> &str {
    match name.find(['!', '%', '&']) {
        Some(pos) => &name[..pos],
        None => name,
    }
}

/// `DynamicAppearanceController::MatchReference` (`Dynamic.cpp:316-339`) — composição PURA sobre
/// os 2 estruturas já portadas acima, mais um set opcional de "condições presentes" (equivalente
/// a `EntityState.conditions`, item `#22`, já fechado — o chamador extrai isso de lá; aqui fica
/// desacoplado de propósito, pra ser testável sem nenhum estado de entidade real). Zero endereço
/// nativo, zero campo cru — literalmente o corpo do método real.
pub fn appearance_matches_reference(
    reference: &DynamicAppearanceRef,
    appearance: &DynamicAppearanceName,
    entity_conditions: Option<&std::collections::BTreeSet<u64>>,
) -> bool {
    if !reference.variants.is_empty() && !reference.matches_variant(appearance.variant) {
        return false;
    }
    if !reference.conditions.is_empty() {
        let Some(present) = entity_conditions else {
            return false;
        };
        if !reference.matches_conditions_with_overrides(present, &appearance.overrides) {
            return false;
        }
    }
    true
}

/// Uma conexão de nó do quest graph (`node`+`socket`, ou só `node` — `QuestPhase/Config.cpp::
/// FillConnection`): forma mapa `{node:[..], socket: nome}` OU sequência nua (só node path, sem
/// socket).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestPhaseConnection {
    pub node_path: Vec<u16>,
    pub socket: Option<String>,
}

/// Uma phase de quest (`quest.phases[]`, `QuestPhase/Config.cpp`): `path`+`parent` obrigatórios
/// (senão a entrada é descartada); `connection`/`input` alimentam o MESMO campo `input` (o C++
/// chama `FillConnection` duas vezes seguidas pro mesmo destino — "connection" é sinônimo/alias
/// de "input", o 2º chamado sobrescreve se ambos existirem); `output` e `intercept` (bool) opcionais.
/// `parent` é uma STRING só (não lista): o C++ guarda num `Set` mas cada parse de UM phase-node só
/// insere UM scalar — o merge entre vários mods pro mesmo `path` acontece rio-acima, fora do escopo
/// de parsear um `.xl` isolado.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestPhaseMod {
    pub phase_path: String,
    pub parent: String,
    pub input: QuestPhaseConnection,
    pub output: QuestPhaseConnection,
    pub intercept: bool,
}

/// Um override de tag de garment (`overrides.tags.<tag>.<component>` → máscara de chunks;
/// `Garment/Config.cpp::GarmentOverrideConfig::LoadYAML` — a chave de TOPO real é `overrides`,
/// não `garment` — `GarmentOverrideConfig::LoadYAML` lê `aNode["overrides"]["tags"]` direto da
/// raiz do documento, confirmado lendo `ExtensionLoader.cpp::AddConfig`, que passa o documento
/// INTEIRO pra cada extensão decidir sua própria subchave).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GarmentOverrideTag {
    pub tag: String,
    pub components: Vec<(String, ChunkMask)>,
}

/// O `.xl` inteiro, tipado.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XlFile {
    pub factories: Vec<String>,
    pub patches: Vec<ResourcePatch>,
    pub links: Vec<ResourceLink>,
    pub scopes: Vec<ResourceScope>,
    pub copies: Vec<ResourceCopy>,
    pub fixes: Vec<ResourceFix>,
    pub localization: Vec<LocalizationGroup>,
    pub localization_extend: Option<String>,
    pub garment_overrides: Vec<GarmentOverrideTag>,
    pub quest_phases: Vec<QuestPhaseMod>,
    pub journals: Vec<String>,
    pub puppet_state: Option<PuppetStateConfig>,
    pub customization: Option<CustomizationConfig>,
    pub animations: Vec<AnimationEntry>,
    pub streaming: Option<WorldStreamingSection>,
    /// chaves de topo presentes mas ainda não tipadas (customNodes, ...). Nada some.
    pub other_sections: Vec<String>,
}

/// Lê um `.xl` (texto) no modelo tipado.
pub fn parse_xl(input: &str) -> Result<XlFile, String> {
    let doc = parse_yaml(input)?;
    let map = doc.as_map().ok_or("documento .xl não é um mapa no topo")?;
    let mut xl = XlFile::default();
    for (key, val) in map {
        match key.as_str() {
            "factories" => xl.factories = val.to_str_list(),
            "resource" => parse_resource(val, &mut xl),
            "localization" => parse_localization(val, &mut xl),
            "overrides" => parse_garment(val, &mut xl),
            "quest" => parse_quest_phase(val, &mut xl),
            // `Journal/Config.cpp`: scalar OU seq de paths. Achado de RE: no C++ real, a forma
            // escalar lê `aNode.Scalar()` (a RAIZ do documento, não o nó "journal") — sempre
            // vazio na prática; implementamos correto aqui (lê o valor do próprio nó).
            "journal" => xl.journals = val.to_str_list(),
            "player" => parse_puppet_state(val, &mut xl),
            "customizations" => parse_customization(val, &mut xl),
            "animations" => parse_animations(val, &mut xl),
            "streaming" => parse_world_streaming(val, &mut xl),
            other => xl.other_sections.push(other.to_string()),
        }
    }
    Ok(xl)
}

/// map `chave → valor` (string→string), p/ names/paths/context do `resource.fix`.
fn str_pairs(node: Option<&Yaml>) -> Vec<(String, String)> {
    node.and_then(|n| n.as_map())
        .map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
        .unwrap_or_default()
}

fn parse_resource(node: &Yaml, xl: &mut XlFile) {
    const TYPED: [&str; 5] = ["patch", "link", "scope", "copy", "fix"];
    // sub-chaves de `resource` ainda não tipadas não somem em silêncio
    if let Some(map) = node.as_map() {
        for (k, _) in map {
            if !TYPED.contains(&k.as_str()) {
                xl.other_sections.push(format!("resource.{k}"));
            }
        }
    }
    // scope: recurso → alvos (scalar|seq)
    if let Some(scope) = node.get("scope").and_then(|s| s.as_map()) {
        for (path, targets) in scope {
            xl.scopes.push(ResourceScope { resource: path.clone(), targets: targets.to_str_list() });
        }
    }
    // copy: source → alvos (scalar|seq)
    if let Some(copy) = node.get("copy").and_then(|c| c.as_map()) {
        for (src, targets) in copy {
            xl.copies.push(ResourceCopy { source: src.clone(), targets: targets.to_str_list() });
        }
    }
    // fix: recurso → { names, paths, context } (cada um map string→string)
    if let Some(fix) = node.get("fix").and_then(|f| f.as_map()) {
        for (res, def) in fix {
            xl.fixes.push(ResourceFix {
                resource: res.clone(),
                names: str_pairs(def.get("names")),
                paths: str_pairs(def.get("paths")),
                context: str_pairs(def.get("context")),
            });
        }
    }
    if let Some(patch) = node.get("patch").and_then(|p| p.as_map()) {
        for (path, def) in patch {
            let mut p = ResourcePatch { patch: path.clone(), ..Default::default() };
            if def.as_map().is_some() {
                // forma mapa: { props: [..], targets: [..] }
                if let Some(props) = def.get("props") {
                    p.props = props.to_str_list();
                }
                if let Some(targets) = def.get("targets") {
                    let list = targets.to_str_list();
                    if targets.tag() == Some("exclude") {
                        p.excludes = list;
                    } else {
                        p.includes = list;
                    }
                }
            } else if def.as_seq().is_some() {
                // forma sequência: lista de alvos (include, ou exclude via tag)
                let list = def.to_str_list();
                if def.tag() == Some("exclude") {
                    p.excludes = list;
                } else {
                    p.includes = list;
                }
            } else if let Some(s) = def.as_str() {
                // forma scalar: um alvo
                p.includes.push(s.to_string());
            }
            if !p.includes.is_empty() || !p.excludes.is_empty() || !p.props.is_empty() {
                xl.patches.push(p);
            }
        }
    }
    if let Some(link) = node.get("link").and_then(|l| l.as_map()) {
        for (target, sources) in link {
            xl.links.push(ResourceLink {
                target: target.clone(),
                sources: sources.to_str_list(),
                is_sequence_form: sources.as_seq().is_some(),
            });
        }
    }
}

/// sequência de escalares → índices de chunk (u8); itens não-numéricos são ignorados.
fn to_u8_list(seq: &[Yaml]) -> Vec<u8> {
    seq.iter().filter_map(|n| n.as_str()).filter_map(|s| s.trim().parse::<u8>().ok()).collect()
}

/// `ParseInt` do C++ (`Num.hpp`): decimal, ou hex com prefixo `0x`/`0X`.
fn parse_u64(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u64>().ok()
    }
}

/// `overrides.tags.<tag>.<component>` → `ChunkMask` (`Garment/Config.cpp::LoadYAML`, 3 formas):
/// mapa `{hide:[..]}` ou `{show:[..]}` (hide checado primeiro, só um dos dois é lido — replica o
/// `for (op : {{"hide",false},{"show",true}}) ... break` do C++); sequência nua = hide implícito;
/// escalar = máscara já-computada, crua (sem inversão — `ChunkMask(uint64_t)` guarda `show=false`
/// e o valor tal como está).
fn parse_garment(node: &Yaml, xl: &mut XlFile) {
    let Some(tags) = node.get("tags").and_then(|t| t.as_map()) else { return };
    for (tag, components_node) in tags {
        let Some(components) = components_node.as_map() else { continue };
        let mut out = Vec::new();
        for (comp, chunks) in components {
            let mask = match chunks {
                Yaml::Map(_) => {
                    if let Some(seq) = chunks.get("hide").and_then(|n| n.as_seq()) {
                        ChunkMask::from_chunks(false, &to_u8_list(seq))
                    } else if let Some(seq) = chunks.get("show").and_then(|n| n.as_seq()) {
                        ChunkMask::from_chunks(true, &to_u8_list(seq))
                    } else {
                        continue;
                    }
                }
                Yaml::Seq(seq) => ChunkMask::from_chunks(false, &to_u8_list(seq)),
                Yaml::Scalar(s) => match parse_u64(s) {
                    Some(m) => ChunkMask { show: false, mask: m },
                    None => continue,
                },
                _ => continue,
            };
            out.push((comp.clone(), mask));
        }
        if !out.is_empty() {
            xl.garment_overrides.push(GarmentOverrideTag { tag: tag.clone(), components: out });
        }
    }
}

/// sequência de escalares → índices u16 (node path do quest graph).
fn to_u16_list(seq: &[Yaml]) -> Vec<u16> {
    seq.iter().filter_map(|n| n.as_str()).filter_map(|s| s.trim().parse::<u16>().ok()).collect()
}

/// `FillConnection` (`QuestPhase/Config.cpp`): mapa `{node:[..], socket: nome}` OU sequência nua.
fn fill_connection(node: Option<&Yaml>) -> QuestPhaseConnection {
    let Some(node) = node else { return QuestPhaseConnection::default() };
    if let Some(map) = node.as_map() {
        QuestPhaseConnection {
            node_path: node.get("node").and_then(|n| n.as_seq()).map(to_u16_list).unwrap_or_default(),
            socket: map.iter().find(|(k, _)| k == "socket").and_then(|(_, v)| v.as_str()).map(String::from),
        }
    } else if let Some(seq) = node.as_seq() {
        QuestPhaseConnection { node_path: to_u16_list(seq), socket: None }
    } else {
        QuestPhaseConnection::default()
    }
}

/// `quest.phases[]` (`QuestPhase/Config.cpp::LoadYAML`): cada item precisa de `path`+`parent`
/// escalares (senão é descartado); `connection`/`input` alimentam o MESMO campo (input chamado
/// por último, sobrescreve); `intercept: true` opcional.
fn parse_quest_phase(node: &Yaml, xl: &mut XlFile) {
    let Some(phases) = node.get("phases").and_then(|p| p.as_seq()) else { return };
    for phase in phases {
        let Some(path) = phase.get("path").and_then(|p| p.as_str()) else { continue };
        let Some(parent) = phase.get("parent").and_then(|p| p.as_str()) else { continue };
        let mut input = fill_connection(phase.get("connection"));
        if phase.get("input").is_some() {
            input = fill_connection(phase.get("input"));
        }
        let output = fill_connection(phase.get("output"));
        let intercept = phase.get("intercept").and_then(|i| i.as_str()).map(|s| s == "true").unwrap_or(false);
        xl.quest_phases.push(QuestPhaseMod {
            phase_path: path.to_string(),
            parent: parent.to_string(),
            input,
            output,
            intercept,
        });
    }
}

/// `player.bodyTypes` (`PuppetState/Config.cpp`).
fn parse_puppet_state(node: &Yaml, xl: &mut XlFile) {
    let Some(body_types) = node.get("bodyTypes") else { return };
    let list = body_types.to_str_list();
    if !list.is_empty() {
        xl.puppet_state = Some(PuppetStateConfig { body_types: list });
    }
}

/// `customizations.{male,female}` (`Customization/Config.cpp`).
fn parse_customization(node: &Yaml, xl: &mut XlFile) {
    let male = node.get("male").map(|n| n.to_str_list()).unwrap_or_default();
    let female = node.get("female").map(|n| n.to_str_list()).unwrap_or_default();
    if !male.is_empty() || !female.is_empty() {
        xl.customization = Some(CustomizationConfig { male_options: male, female_options: female });
    }
}

/// `animations[]` (`Animation/Config.cpp::LoadYAML` — ver doc de `AnimationEntry` pros 2 bugs
/// upstream que corrigimos aqui: `entity` como sequência e `vars` nunca funcionam no C++ real).
fn parse_animations(node: &Yaml, xl: &mut XlFile) {
    let Some(entries) = node.as_seq() else { return };
    for entry in entries {
        let Some(entity_node) = entry.get("entity") else { continue };
        let entities = entity_node.to_str_list();
        if entities.is_empty() {
            continue;
        }
        let Some(set) = entry.get("set").and_then(|s| s.as_str()) else { continue };
        let variables = entry.get("vars").map(|v| v.to_str_list()).unwrap_or_default();
        let priority = entry
            .get("priority")
            .and_then(|p| p.as_str())
            .and_then(|s| s.trim().parse::<u8>().ok())
            .unwrap_or(128);
        let component = entry.get("component").and_then(|c| c.as_str()).unwrap_or("root").to_string();
        xl.animations.push(AnimationEntry { entities, set: set.to_string(), variables, priority, component });
    }
}

// ===== streaming.sectors (WorldStreaming/Config.cpp) =====

/// sequência de escalares → f32; `None` se QUALQUER item não for um número (fiel ao C++: um único
/// item inválido invalida a lista inteira — `as<std::vector<float>>()` do yaml-cpp lançaria).
fn to_f32_list(seq: &[Yaml]) -> Option<Vec<f32>> {
    let mut out = Vec::with_capacity(seq.len());
    for item in seq {
        out.push(item.as_str()?.trim().parse::<f32>().ok()?);
    }
    Some(out)
}

fn parse_vec3(node: Option<&Yaml>) -> Option<Vec3> {
    let v = to_f32_list(node?.as_seq()?)?;
    (v.len() == 3).then(|| Vec3 { x: v[0], y: v[1], z: v[2] })
}

/// `position` no nível de NODE: aceita 3 OU 4 valores; o `.w` final é SEMPRE 0 (RE, ver doc de `Vec4`).
fn parse_position_node(node: Option<&Yaml>) -> Option<Vec4> {
    let v = to_f32_list(node?.as_seq()?)?;
    (v.len() == 3 || v.len() == 4).then(|| Vec4 { x: v[0], y: v[1], z: v[2], w: 0.0 })
}

/// `position` no nível de SUB-node: exige EXATAMENTE 4 valores; `.w` também sempre 0.
fn parse_position_subnode(node: Option<&Yaml>) -> Option<Vec4> {
    let v = to_f32_list(node?.as_seq()?)?;
    (v.len() == 4).then(|| Vec4 { x: v[0], y: v[1], z: v[2], w: 0.0 })
}

fn parse_quat(node: Option<&Yaml>) -> Option<Quat> {
    let v = to_f32_list(node?.as_seq()?)?;
    (v.len() == 4).then(|| Quat { i: v[0], j: v[1], k: v[2], r: v[3] })
}

fn parse_i64(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<i64>().ok()
    }
}

/// "última chave PRESENTE vence" — replica a sequência de `ParseResource`/`ParseName`/
/// `ParseRecordID` chamadas nas várias chaves-sinônimo (cada chamada só sobrescreve se o
/// SEU próprio nó estiver definido; ausência não apaga o que já foi lido por um sinônimo anterior).
fn last_present_str(node: &Yaml, keys: &[&str]) -> Option<String> {
    keys.iter().filter_map(|k| node.get(k).and_then(|n| n.as_str())).last().map(String::from)
}

/// `ParseSubDeletions`: exige a lista de índices E a contagem esperada, ambas presentes e válidas
/// (senão não faz nada). Índice fora de `[0, count)` OU item não-numérico invalida a lista
/// INTEIRA (`aDeletions.clear()` no C++ — inclusive apagando entradas já lidas por um sinônimo
/// anterior nesta mesma chamada de node). `expected` é sobrescrito pelo count desta chamada.
fn parse_sub_deletions(node: &Yaml, list_key: &str, count_key: &str, out: &mut Vec<i64>, expected: &mut i64) {
    let Some(seq) = node.get(list_key).and_then(|n| n.as_seq()) else { return };
    let Some(count) = node.get(count_key).and_then(|n| n.as_str()).and_then(parse_i64) else { return };
    if count <= 0 {
        return;
    }
    let mut collected = Vec::with_capacity(seq.len());
    for item in seq {
        match item.as_str().and_then(parse_i64) {
            Some(idx) if (0..count).contains(&idx) => collected.push(idx),
            _ => {
                out.clear();
                return;
            }
        }
    }
    out.extend(collected);
    *expected = count;
}

/// `ParseSubMutations`: exige a lista E a contagem esperada válidas; itens malformados ou sem
/// NENHUM de position/orientation/scale são simplesmente PULADOS (≠ `parse_sub_deletions`, que
/// invalida a lista inteira — assimetria real do C++, replicada fielmente).
fn parse_sub_mutations(node: &Yaml, list_key: &str, count_key: &str, out: &mut Vec<WorldSubNodeMutation>, expected: &mut i64) {
    let Some(seq) = node.get(list_key).and_then(|n| n.as_seq()) else { return };
    let Some(count) = node.get(count_key).and_then(|n| n.as_str()).and_then(parse_i64) else { return };
    if count <= 0 {
        return;
    }
    for item in seq {
        if item.as_map().is_none() {
            continue;
        }
        let Some(idx) = item.get("index").and_then(|n| n.as_str()).and_then(parse_i64) else { continue };
        if !(0..count).contains(&idx) {
            continue;
        }
        let position = parse_position_subnode(item.get("position"));
        let orientation = parse_quat(item.get("orientation"));
        let scale = parse_vec3(item.get("scale"));
        if position.is_none() && orientation.is_none() && scale.is_none() {
            continue;
        }
        out.push(WorldSubNodeMutation { sub_node_index: idx, position, orientation, scale });
    }
    *expected = count;
}

const RESOURCE_KEYS: [&str; 6] = ["resource", "mesh", "meshRef", "material", "effect", "entityTemplate"];
const APPEARANCE_KEYS: [&str; 3] = ["appearance", "appearanceName", "meshAppearance"];
const RECORD_ID_KEYS: [&str; 3] = ["recordID", "recordId", "objectRecordId"];

fn parse_world_streaming(node: &Yaml, xl: &mut XlFile) {
    let blocks = node.get("blocks").map(|b| b.to_str_list()).unwrap_or_default();
    let mut sectors = Vec::new();
    if let Some(sector_seq) = node.get("sectors").and_then(|s| s.as_seq()) {
        for sector in sector_seq {
            let Some(path) = sector.get("path").and_then(|p| p.as_str()).filter(|s| !s.is_empty()) else { continue };
            let Some(expected_nodes) = sector.get("expectedNodes").and_then(|n| n.as_str()).and_then(parse_i64) else { continue };
            if expected_nodes <= 0 {
                continue;
            }
            let mut node_deletions = Vec::new();
            if let Some(seq) = sector.get("nodeDeletions").and_then(|n| n.as_seq()) {
                for del in seq {
                    let Some(node_type) = del.get("type").and_then(|t| t.as_str()) else { continue };
                    let Some(idx) = del.get("index").and_then(|i| i.as_str()).and_then(parse_i64) else { continue };
                    if !(0..expected_nodes).contains(&idx) {
                        continue;
                    }
                    let mut d = WorldNodeDeletion { node_index: idx, node_type: node_type.to_string(), ..Default::default() };
                    parse_sub_deletions(del, "actorDeletions", "expectedActors", &mut d.sub_node_deletions, &mut d.expected_sub_nodes);
                    parse_sub_deletions(del, "instanceDeletions", "expectedInstances", &mut d.sub_node_deletions, &mut d.expected_sub_nodes);
                    node_deletions.push(d);
                }
            }
            let mut node_mutations = Vec::new();
            if let Some(seq) = sector.get("nodeMutations").and_then(|n| n.as_seq()) {
                for mu in seq {
                    let Some(node_type) = mu.get("type").and_then(|t| t.as_str()) else { continue };
                    let Some(idx) = mu.get("index").and_then(|i| i.as_str()).and_then(parse_i64) else { continue };
                    if !(0..expected_nodes).contains(&idx) {
                        continue;
                    }
                    let nb_diff = mu
                        .get("nbNodesUnderProxyDiff")
                        .and_then(|n| n.as_str())
                        .and_then(|s| parse_i64(s))
                        .and_then(|v| i32::try_from(v).ok());
                    let mut m = WorldNodeMutation {
                        node_index: idx,
                        node_type: node_type.to_string(),
                        position: parse_position_node(mu.get("position")),
                        orientation: parse_quat(mu.get("orientation")),
                        scale: parse_vec3(mu.get("scale")),
                        resource_path: last_present_str(mu, &RESOURCE_KEYS),
                        appearance_name: last_present_str(mu, &APPEARANCE_KEYS),
                        record_id: last_present_str(mu, &RECORD_ID_KEYS),
                        nb_nodes_under_proxy_diff: nb_diff,
                        ..Default::default()
                    };
                    parse_sub_mutations(mu, "actorMutations", "expectedActors", &mut m.sub_node_mutations, &mut m.expected_sub_nodes);
                    parse_sub_mutations(mu, "instanceMutations", "expectedInstances", &mut m.sub_node_mutations, &mut m.expected_sub_nodes);
                    node_mutations.push(m);
                }
            }
            if node_deletions.is_empty() && node_mutations.is_empty() {
                continue;
            }
            sectors.push(WorldSectorMod { path: path.to_string(), expected_nodes, node_deletions, node_mutations });
        }
    }
    if !blocks.is_empty() || !sectors.is_empty() {
        xl.streaming = Some(WorldStreamingSection { blocks, sectors });
    }
}

fn parse_localization(node: &Yaml, xl: &mut XlFile) {
    for kind in ["onscreens", "subtitles", "lipmaps", "vomaps"] {
        if let Some(grp) = node.get(kind).and_then(|g| g.as_map()) {
            let entries: Vec<(String, Vec<String>)> =
                grp.iter().map(|(lang, paths)| (lang.clone(), paths.to_str_list())).collect();
            if !entries.is_empty() {
                xl.localization.push(LocalizationGroup { kind: kind.to_string(), entries });
            }
        }
    }
    if let Some(ext) = node.get("extend").and_then(|e| e.as_str()) {
        xl.localization_extend = Some(ext.to_string());
    }
}

impl XlFile {
    /// Resumo legível pra CLI.
    pub fn summary(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("factories: {}\n", self.factories.len()));
        for f in &self.factories {
            s.push_str(&format!("  - {f}\n"));
        }
        s.push_str(&format!("resource.patch: {}\n", self.patches.len()));
        for p in &self.patches {
            s.push_str(&format!(
                "  {} → +{} alvo(s), -{} excluído(s), props: [{}]\n",
                p.patch, p.includes.len(), p.excludes.len(), p.props.join(", ")
            ));
        }
        s.push_str(&format!("resource.link: {}\n", self.links.len()));
        for l in &self.links {
            s.push_str(&format!("  {} → {} fonte(s)\n", l.target, l.sources.len()));
        }
        s.push_str(&format!("resource.scope: {}\n", self.scopes.len()));
        for sc in &self.scopes {
            s.push_str(&format!("  {} → {} alvo(s)\n", sc.resource, sc.targets.len()));
        }
        s.push_str(&format!("resource.copy: {}\n", self.copies.len()));
        for c in &self.copies {
            s.push_str(&format!("  {} → {} cópia(s)\n", c.source, c.targets.len()));
        }
        s.push_str(&format!("resource.fix: {}\n", self.fixes.len()));
        for f in &self.fixes {
            s.push_str(&format!(
                "  {} → {} nome(s), {} path(s), {} contexto(s)\n",
                f.resource, f.names.len(), f.paths.len(), f.context.len()
            ));
        }
        s.push_str(&format!("localization: {} grupo(s)\n", self.localization.len()));
        for g in &self.localization {
            s.push_str(&format!("  {}: {} idioma(s)\n", g.kind, g.entries.len()));
        }
        if let Some(ext) = &self.localization_extend {
            s.push_str(&format!("localization.extend: {ext}\n"));
        }
        if let Some(st) = &self.streaming {
            s.push_str(&format!("streaming.blocks: {}\n", st.blocks.len()));
            s.push_str(&format!("streaming.sectors: {}\n", st.sectors.len()));
            for sec in &st.sectors {
                s.push_str(&format!(
                    "  {} (expectedNodes={}) → {} deleção(ões), {} mutação(ões)\n",
                    sec.path, sec.expected_nodes, sec.node_deletions.len(), sec.node_mutations.len()
                ));
            }
        }
        s.push_str(&format!("journal: {}\n", self.journals.len()));
        if let Some(ps) = &self.puppet_state {
            s.push_str(&format!("player.bodyTypes: {}\n", ps.body_types.join(", ")));
        }
        if let Some(c) = &self.customization {
            s.push_str(&format!("customizations: {} male, {} female\n", c.male_options.len(), c.female_options.len()));
        }
        s.push_str(&format!("animations: {}\n", self.animations.len()));
        for a in &self.animations {
            s.push_str(&format!("  {} → set={} priority={}\n", a.entities.join(","), a.set, a.priority));
        }
        s.push_str(&format!("quest.phases: {}\n", self.quest_phases.len()));
        for p in &self.quest_phases {
            s.push_str(&format!("  {} (parent={}, intercept={})\n", p.phase_path, p.parent, p.intercept));
        }
        s.push_str(&format!("overrides.tags: {}\n", self.garment_overrides.len()));
        for t in &self.garment_overrides {
            s.push_str(&format!("  {} → {} componente(s)\n", t.tag, t.components.len()));
            for (comp, m) in &t.components {
                s.push_str(&format!("    {comp}: show={} mask={:#018x}\n", m.show, m.mask));
            }
        }
        if !self.other_sections.is_empty() {
            s.push_str(&format!("⚠ seções ainda não processadas: {}\n", self.other_sections.join(", ")));
        }
        s
    }
}

// ============================ testes (a PROVA, offline) ============================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_basico_mapa_seq_flow() {
        let y = parse_yaml("a: 1\nb:\n  - x\n  - y\nc: [ p, q ]\n").unwrap();
        assert_eq!(y.get("a").unwrap().as_str(), Some("1"));
        assert_eq!(y.get("b").unwrap().to_str_list(), vec!["x", "y"]);
        assert_eq!(y.get("c").unwrap().to_str_list(), vec!["p", "q"]);
    }

    #[test]
    fn comentarios_e_brancas() {
        let y = parse_yaml("# topo\nfactories:\n  - a.csv   # inline\n\n  - b.csv\n").unwrap();
        assert_eq!(y.get("factories").unwrap().to_str_list(), vec!["a.csv", "b.csv"]);
    }

    // ---- exemplos REAIS do ArchiveXL (cp2077-archive-xl/.../resources) ----

    #[test]
    fn template_factories_e_localization() {
        let src = "factories:\n  - mymod\\factories\\clothing.csv\n  - mymod\\factories\\weapons.csv\nlocalization:\n  onscreens:\n    en-us: mymod\\localization\\en-us.json\n    de-de: mymod\\localization\\de-de.json\n";
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.factories, vec!["mymod\\factories\\clothing.csv", "mymod\\factories\\weapons.csv"]);
        assert_eq!(xl.localization.len(), 1);
        let g = &xl.localization[0];
        assert_eq!(g.kind, "onscreens");
        assert_eq!(g.entries.len(), 2);
        assert_eq!(g.entries[0], ("en-us".to_string(), vec!["mymod\\localization\\en-us.json".to_string()]));
        assert!(xl.other_sections.is_empty());
    }

    #[test]
    fn patch_real_hairpatch() {
        // PlayerCustomizationHairPatch.xl (forma mapa: props + targets)
        let src = "resource:\n  patch:\n    archive_xl\\characters\\common\\hair\\h1_base_color_patch.mesh:\n      props: [ appearances ]\n      targets: [ player_ma_hair.mesh, player_wa_hair.mesh ]\n";
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.patches.len(), 1);
        let p = &xl.patches[0];
        assert_eq!(p.patch, "archive_xl\\characters\\common\\hair\\h1_base_color_patch.mesh");
        assert_eq!(p.props, vec!["appearances"]);
        assert_eq!(p.includes, vec!["player_ma_hair.mesh", "player_wa_hair.mesh"]);
        assert!(p.excludes.is_empty());
    }

    #[test]
    fn link_real_migration() {
        // Migration.xl (resource.link: alvo → [fontes])
        let src = "resource:\n  link:\n    archive_xl\\a\\h1.mesh:\n      - archive_xl\\a\\base.mesh\n    archive_xl\\b\\head.app:\n      - archive_xl\\b\\lashes.app\n";
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.links.len(), 2);
        assert_eq!(xl.links[0].target, "archive_xl\\a\\h1.mesh");
        assert_eq!(xl.links[0].sources, vec!["archive_xl\\a\\base.mesh"]);
        assert_eq!(xl.links[1].sources, vec!["archive_xl\\b\\lashes.app"]);
    }

    // ---- formas alternativas da fonte C++ ----

    #[test]
    fn patch_scalar_e_seq_e_exclude() {
        // scalar: 1 alvo
        let xl = parse_xl("resource:\n  patch:\n    a.mesh: b.mesh\n").unwrap();
        assert_eq!(xl.patches[0].includes, vec!["b.mesh"]);

        // sequência com tag !exclude → vai pra excludes
        let xl2 = parse_xl("resource:\n  patch:\n    a.mesh: !exclude [ x.mesh, y.mesh ]\n").unwrap();
        assert_eq!(xl2.patches[0].excludes, vec!["x.mesh", "y.mesh"]);
        assert!(xl2.patches[0].includes.is_empty());
    }

    #[test]
    fn factories_scalar_unico() {
        let xl = parse_xl("factories: mymod\\one.csv\n").unwrap();
        assert_eq!(xl.factories, vec!["mymod\\one.csv"]);
    }

    #[test]
    fn secao_desconhecida_vai_pra_other_sections() {
        // "streaming" agora É tipado (RE 2026-07-15 cont.60) — usa 2 chaves genuinamente
        // desconhecidas (Attachment/Mesh/Transmog/InkSpawner não têm seção .xl própria nenhuma).
        let xl = parse_xl("attachment:\n  x: y\ncustomNode: x\n").unwrap();
        assert!(xl.other_sections.contains(&"attachment".to_string()));
        assert!(xl.other_sections.contains(&"customNode".to_string()));
    }

    #[test]
    fn ancora_e_alias() {
        // padrão real do EyesFix/BrowsFix: define com &nome, reusa com *nome
        let src = "resource:\n  fix:\n    a.ink: &Fix\n      paths:\n        x.app: y.app\n    b.ink: *Fix\n";
        // o YAML genérico resolve o alias pro MESMO nó da âncora
        let doc = parse_yaml(src).unwrap();
        let fix = doc.get("resource").unwrap().get("fix").unwrap();
        let a = fix.get("a.ink").unwrap();
        let b = fix.get("b.ink").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.get("paths").unwrap().get("x.app").unwrap().as_str(), Some("y.app"));
        // e o modelo tipado captura os dois fixes (alias expandido), via resource.fix.paths
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.fixes.len(), 2);
        assert_eq!(xl.fixes[0].paths, vec![("x.app".to_string(), "y.app".to_string())]);
        assert_eq!(xl.fixes[1].paths, vec![("x.app".to_string(), "y.app".to_string())]);
        assert!(!xl.other_sections.contains(&"resource.fix".to_string()));
    }

    #[test]
    fn scope_copy_fix_tipados() {
        // scope: recurso → alvos (scalar e seq)
        let xl = parse_xl("resource:\n  scope:\n    a.app: b.app\n    c.app:\n      - d.app\n      - e.app\n").unwrap();
        assert_eq!(xl.scopes.len(), 2);
        assert_eq!(xl.scopes[0].targets, vec!["b.app"]);
        assert_eq!(xl.scopes[1].targets, vec!["d.app", "e.app"]);

        // copy: source → alvos
        let xl2 = parse_xl("resource:\n  copy:\n    src.mesh:\n      - t1.mesh\n      - t2.mesh\n").unwrap();
        assert_eq!(xl2.copies.len(), 1);
        assert_eq!(xl2.copies[0].source, "src.mesh");
        assert_eq!(xl2.copies[0].targets, vec!["t1.mesh", "t2.mesh"]);

        // fix: names + paths + context
        let src = "resource:\n  fix:\n    target.mesh:\n      names:\n        old_mat: new_mat\n      paths:\n        old.app: new.app\n      context:\n        param: value\n";
        let xl3 = parse_xl(src).unwrap();
        assert_eq!(xl3.fixes.len(), 1);
        let f = &xl3.fixes[0];
        assert_eq!(f.resource, "target.mesh");
        assert_eq!(f.names, vec![("old_mat".to_string(), "new_mat".to_string())]);
        assert_eq!(f.paths, vec![("old.app".to_string(), "new.app".to_string())]);
        assert_eq!(f.context, vec![("param".to_string(), "value".to_string())]);
        assert!(xl3.other_sections.is_empty());
    }

    #[test]
    fn localization_multi_grupo_e_extend() {
        let src = "localization:\n  onscreens:\n    en-us: a.json\n  subtitles:\n    en-us: b.json\n  extend: base\n";
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.localization.len(), 2);
        assert_eq!(xl.localization_extend, Some("base".to_string()));
    }

    // ===== overrides.tags (Garment ChunkMask, RE 2026-07-15) =====

    #[test]
    fn garment_overrides_forma_sequencia_e_hide_implicito() {
        // sequência nua = hide implícito (ChunkMask(vector<uint8_t>), sem `set` explícito).
        let src = "overrides:\n  tags:\n    my_tag:\n      my_component:\n        - 0\n        - 1\n";
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.garment_overrides.len(), 1);
        let t = &xl.garment_overrides[0];
        assert_eq!(t.tag, "my_tag");
        assert_eq!(t.components.len(), 1);
        let (comp, mask) = &t.components[0];
        assert_eq!(comp, "my_component");
        assert!(!mask.show);
        assert_eq!(mask.mask, !0b11u64); // tudo, exceto os chunks 0 e 1
        assert!(xl.other_sections.is_empty());
    }

    #[test]
    fn garment_overrides_forma_mapa_hide_e_show() {
        let src = "overrides:\n  tags:\n    t1:\n      c_hide:\n        hide:\n          - 2\n      c_show:\n        show:\n          - 2\n";
        let xl = parse_xl(src).unwrap();
        let t = &xl.garment_overrides[0];
        assert_eq!(t.components.len(), 2);
        let hide = &t.components.iter().find(|(n, _)| n == "c_hide").unwrap().1;
        let show = &t.components.iter().find(|(n, _)| n == "c_show").unwrap().1;
        assert!(!hide.show);
        assert_eq!(hide.mask, !0b100u64); // hide: complemento do bit 2
        assert!(show.show);
        assert_eq!(show.mask, 0b100u64); // show: seleção positiva, SEM inversão
    }

    // ===== builtin_tag_overrides (ArchiveXL #21/GetTagManager, RE 2026-08-11) =====

    #[test]
    fn builtin_tag_hide_torso_bate_com_a_fonte_real() {
        let overrides = super::builtin_tag_overrides("hide_Torso").unwrap();
        assert_eq!(overrides.len(), 8);
        let (_, mask) = overrides.iter().find(|(n, _)| *n == "t0_000_pma_base__full").unwrap();
        assert!(!mask.show);
        assert_eq!(mask.mask, !0b1111u64); // hide([0,1,2,3])
        let (_, mask_n0) = overrides.iter().find(|(n, _)| *n == "n0_").unwrap();
        assert_eq!(mask_n0.mask, 0); // Hide() sem chunks = mask 0 (nunca invertido, regra do from_chunks)
    }

    #[test]
    fn builtin_tag_highheels_tem_show_positivo() {
        let overrides = super::builtin_tag_overrides("HighHeels").unwrap();
        let (_, mask) = overrides.iter().find(|(n, _)| *n == "l0_000_pma_base__high_heels").unwrap();
        assert!(mask.show);
        assert_eq!(mask.mask, 0b111); // show([0,1,2]), SEM inversão
        let (_, mask_full) = overrides.iter().find(|(n, _)| *n == "t0_000_pma_base__full").unwrap();
        assert!(!mask_full.show);
        assert_eq!(mask_full.mask, !0b1110_0000u64); // hide([5,6,7])
    }

    #[test]
    fn builtin_tag_flatshoes_espelha_highheels_com_slot_diferente() {
        let overrides = super::builtin_tag_overrides("FlatShoes").unwrap();
        assert_eq!(overrides.len(), 5);
        let (_, mask) = overrides.iter().find(|(n, _)| *n == "l0_000_pwa_base__flat_shoes").unwrap();
        assert!(mask.show);
        assert_eq!(mask.mask, 0b111);
    }

    #[test]
    fn builtin_tag_hide_head_12_partes_todas_hide_all() {
        let overrides = super::builtin_tag_overrides("hide_Head").unwrap();
        assert_eq!(overrides.len(), 12);
        assert!(overrides.iter().all(|(_, m)| !m.show && m.mask == 0));
    }

    #[test]
    fn builtin_tag_nome_desconhecido_devolve_none() {
        assert!(super::builtin_tag_overrides("tag_que_nao_existe").is_none());
    }

    #[test]
    fn builtin_tag_todas_as_14_tags_reais_resolvem() {
        let tags = [
            "hide_Head", "hide_Arms", "hide_Torso", "hide_LowerAbdomen", "hide_UpperAbdomen",
            "hide_CollarBone", "hide_Chest", "hide_Legs", "hide_Thighs", "hide_Calves",
            "hide_Ankles", "hide_Feet", "HighHeels", "FlatShoes",
        ];
        for t in tags {
            assert!(super::builtin_tag_overrides(t).is_some(), "tag '{t}' deveria resolver");
        }
    }

    // ===== ComponentState (ArchiveXL #22/OverrideStateManager, RE 2026-08-11) =====

    #[test]
    fn componentstate_hiding_e_and_cumulativo_por_hash() {
        let mut s = super::ComponentState::new();
        s.add_hiding_override(0xAAAA, 0b1111_0000); // mod A: esconde chunks 4-7
        s.add_hiding_override(0xBBBB, 0b0011_1111); // mod B: esconde chunks 0-5 (INTERSEÇÃO com A no hiding)
        // AND cumulativo: só os bits que os DOIS hides preservam sobrevivem — aqui nenhum bit
        // comum entre "não-4-7" e "não-0-5" no universo de 8 bits, mas testo a fórmula exata:
        // original=0xFF, hiding_final = 0b1111_0000 & 0b0011_1111 = 0b0011_0000
        let mask = s.overridden_chunk_mask(0xFF);
        assert_eq!(mask, 0b0011_0000);
    }

    #[test]
    fn componentstate_showing_e_or_cumulativo_por_hash() {
        let mut s = super::ComponentState::new();
        s.add_showing_override(0xAAAA, 0b0000_0001);
        s.add_showing_override(0xBBBB, 0b0000_0010);
        // original=0 (nada visível), showing OR cumulativo soma os 2 mods
        assert_eq!(s.overridden_chunk_mask(0), 0b0000_0011);
    }

    #[test]
    fn componentstate_remove_por_hash_nao_afeta_outros_mods() {
        // cenário exato do item: "quando um mod é desinstalado, só as mudanças DAQUELE hash saem"
        let mut s = super::ComponentState::new();
        s.add_hiding_override(0xAAAA, 0b1111_0000); // mod A
        s.add_showing_override(0xBBBB, 0b0000_0001); // mod B
        assert!(s.has_overridden_chunk_mask());
        s.remove_chunk_mask_override(0xAAAA); // desinstala só o mod A
        assert!(s.has_overridden_chunk_mask()); // mod B ainda ativo
        // hiding do A sumiu (volta a `~0`, sem restrição); showing do B permanece
        let mask = s.overridden_chunk_mask(0xF0);
        assert_eq!(mask, 0xF0 | 0b0000_0001);
        s.remove_chunk_mask_override(0xBBBB); // desinstala o mod B também
        assert!(!s.has_overridden_chunk_mask()); // nenhum override resta
    }

    #[test]
    fn componentstate_appearance_override_1_valor_por_hash_default_se_vazio() {
        let mut s = super::ComponentState::new();
        assert_eq!(s.appearance_override(), "default");
        assert!(!s.has_appearance_overrides());
        s.add_appearance_override(0xAAAA, "casual_v1");
        assert!(s.has_appearance_overrides());
        assert_eq!(s.appearance_override(), "casual_v1");
        assert!(s.remove_appearance_override(0xAAAA));
        assert_eq!(s.appearance_override(), "default");
    }

    #[test]
    fn componentstate_is_overridden_reflete_qualquer_tipo_de_override() {
        let mut s = super::ComponentState::new();
        assert!(!s.is_overridden());
        s.add_appearance_override(1, "x");
        assert!(s.is_overridden());
    }

    // ===== EntityState / OverrideStateManager (ArchiveXL #22, RE 2026-08-11) =====

    #[test]
    fn entitystate_get_or_create_component_state_isolado_por_hash() {
        let mut e = super::EntityState::new();
        assert_eq!(e.component_count(), 0);
        e.component_state(0xC0DE).add_hiding_override(1, 0xFF);
        e.component_state(0xBEEF).add_hiding_override(1, 0x0F);
        assert_eq!(e.component_count(), 2);
        // cada componente mantém seu próprio estado, sem vazar pro outro
        assert_eq!(e.find_component_state(0xC0DE).unwrap().overridden_chunk_mask(!0), 0xFF);
        assert_eq!(e.find_component_state(0xBEEF).unwrap().overridden_chunk_mask(!0), 0x0F);
        assert!(e.find_component_state(0xDEAD).is_none());
    }

    #[test]
    fn entitystate_remove_all_overrides_so_afeta_o_hash_daquele_mod() {
        // cenário exato do #22: 2 mods sobrepõem o MESMO componente; desinstalar 1 mod (hash)
        // não deve afetar o override do outro.
        let mut e = super::EntityState::new();
        e.add_chunk_mask_override(/*hash mod A*/ 1, /*componente*/ 100, 0xF0, false);
        e.add_appearance_override(/*hash mod A*/ 1, 100, "modA_look");
        e.add_offset_override(/*hash mod A*/ 1, /*recurso*/ 200, 5);
        e.add_chunk_mask_override(/*hash mod B*/ 2, 100, 0x0F, false);
        e.add_appearance_override(/*hash mod B*/ 2, 100, "modB_look");
        e.add_offset_override(/*hash mod B*/ 2, 200, 9);

        e.remove_all_overrides(1); // "desinstala" o mod A

        let cs = e.find_component_state(100).unwrap();
        assert_eq!(cs.overridden_chunk_mask(!0), 0x0F); // só o hiding do mod B sobrou
        assert_eq!(cs.appearance_override(), "modB_look");
        assert_eq!(e.offset_override(200), 9); // offset do mod B intacto
    }

    #[test]
    fn entitystate_remove_chunk_mask_overrides_nao_toca_appearance() {
        let mut e = super::EntityState::new();
        e.add_chunk_mask_override(1, 100, 0xF0, false);
        e.add_appearance_override(1, 100, "look");
        e.remove_chunk_mask_overrides(1);
        let cs = e.find_component_state(100).unwrap();
        assert!(!cs.has_overridden_chunk_mask());
        assert!(cs.has_appearance_overrides()); // appearance não foi tocada
    }

    #[test]
    fn overridestatemanager_indexa_por_4_chaves_diferentes() {
        let mut mgr = super::OverrideStateManager::new();
        let entity = 0x1000u64;
        mgr.entity_state(entity).component_state(1).add_hiding_override(9, 0xAB);

        mgr.link_path(entity, 0x2000);
        assert!(mgr.link_processor(entity, 0x3000)); // entidade já existe -> linka
        mgr.link_pointer(entity, 0x4000);

        // as 4 vias resolvem pro MESMO EntityState (mesmo dado)
        for lookup in [
            mgr.find_entity_state(entity),
            mgr.find_entity_state_by_path(0x2000),
            mgr.find_entity_state_by_processor(0x3000),
            mgr.find_entity_state_by_pointer(0x4000),
        ] {
            let cs = lookup.unwrap().find_component_state(1).unwrap();
            assert_eq!(cs.overridden_chunk_mask(!0), 0xAB);
        }
        assert_eq!(mgr.entity_count(), 1);
    }

    #[test]
    fn overridestatemanager_link_processor_nao_cria_entidade_desconhecida() {
        // fiel a `LinkEntityToAssembler`: só linka se a entidade JÁ existir, nunca cria.
        let mut mgr = super::OverrideStateManager::new();
        assert!(!mgr.link_processor(0xDEAD, 0x9999));
        assert!(mgr.find_entity_state_by_processor(0x9999).is_none());
        assert_eq!(mgr.entity_count(), 0);
    }

    #[test]
    fn overridestatemanager_link_pointer_cria_entidade_se_precisar() {
        // fiel a `LinkEntityToPointer`: o `else` cria via GetEntityState (get-or-create).
        let mut mgr = super::OverrideStateManager::new();
        mgr.link_pointer(0xABCD, 0x1111);
        assert!(mgr.find_entity_state(0xABCD).is_some());
        assert!(mgr.find_entity_state_by_pointer(0x1111).is_some());
        assert_eq!(mgr.entity_count(), 1);
    }

    #[test]
    fn overridestatemanager_clear_states_esvazia_as_4_tabelas() {
        let mut mgr = super::OverrideStateManager::new();
        mgr.entity_state(1);
        mgr.link_path(1, 10);
        mgr.link_pointer(1, 20);
        mgr.link_processor(1, 30);
        mgr.clear_states();
        assert_eq!(mgr.entity_count(), 0);
        assert!(mgr.find_entity_state_by_path(10).is_none());
        assert!(mgr.find_entity_state_by_pointer(20).is_none());
        assert!(mgr.find_entity_state_by_processor(30).is_none());
    }

    #[test]
    fn resourcestateoffsets_overridden_offset_devolve_0_se_vazio() {
        let rs = super::ResourceStateOffsets::new();
        assert_eq!(rs.overridden_offset(), 0);
        assert!(!rs.is_overridden());
    }

    // ===== process_dynamic_string / is_dynamic_value (ArchiveXL #23, RE 2026-08-11) =====

    #[test]
    fn is_dynamic_value_checa_prefixo_asterisco() {
        assert!(super::is_dynamic_value("*foo"));
        assert!(!super::is_dynamic_value("foo"));
        assert!(!super::is_dynamic_value(""));
    }

    #[test]
    fn process_dynamic_string_substitui_atributo_local() {
        let mut local = std::collections::HashMap::new();
        local.insert(bwms_hashes::fnv1a64(b"gender"), "male".to_string());
        let global = std::collections::HashMap::new();
        let r = super::process_dynamic_string(&local, &global, "item_{gender}_v1");
        assert!(r.valid);
        assert_eq!(r.value, "item_male_v1");
        assert!(r.attributes.contains(&bwms_hashes::fnv1a64(b"gender")));
        assert!(!r.missed);
    }

    #[test]
    fn process_dynamic_string_local_tem_prioridade_sobre_global() {
        let mut local = std::collections::HashMap::new();
        local.insert(bwms_hashes::fnv1a64(b"x"), "LOCAL".to_string());
        let mut global = std::collections::HashMap::new();
        global.insert(bwms_hashes::fnv1a64(b"x"), "GLOBAL".to_string());
        let r = super::process_dynamic_string(&local, &global, "{x}");
        assert_eq!(r.value, "LOCAL");
    }

    #[test]
    fn process_dynamic_string_pula_asterisco_inicial() {
        let mut local = std::collections::HashMap::new();
        local.insert(bwms_hashes::fnv1a64(b"attr"), "X".to_string());
        let global = std::collections::HashMap::new();
        let r = super::process_dynamic_string(&local, &global, "*{attr}");
        assert!(r.valid);
        assert_eq!(r.value, "X");
    }

    #[test]
    fn process_dynamic_string_atributo_faltando_marca_missed_mas_nao_crasha() {
        let local = std::collections::HashMap::new();
        let global = std::collections::HashMap::new();
        let r = super::process_dynamic_string(&local, &global, "item_{unknown}_v1");
        assert!(r.missed);
        assert!(r.valid); // ainda válido — só o VALOR daquele atributo não foi resolvido
        assert_eq!(r.value, "item__v1"); // {unknown} vira string vazia
    }

    #[test]
    fn process_dynamic_string_sem_chaves_e_invalido() {
        let local = std::collections::HashMap::new();
        let global = std::collections::HashMap::new();
        let r = super::process_dynamic_string(&local, &global, "plain_string_no_markers");
        assert!(!r.valid); // attributes vazio -> inválido, mesmo sem erro de parsing
    }

    #[test]
    fn process_dynamic_string_chave_nao_fechada_e_invalido() {
        let local = std::collections::HashMap::new();
        let global = std::collections::HashMap::new();
        let r = super::process_dynamic_string(&local, &global, "item_{attr_sem_fechar");
        assert!(!r.valid);
    }

    #[test]
    fn process_dynamic_string_marcador_opcional_final_e_removido() {
        let mut local = std::collections::HashMap::new();
        local.insert(bwms_hashes::fnv1a64(b"x"), "Y".to_string());
        let global = std::collections::HashMap::new();
        let r = super::process_dynamic_string(&local, &global, "{x}?");
        assert!(r.valid);
        assert!(r.optional);
        assert_eq!(r.value, "Y"); // '?' final removido do valor
    }

    #[test]
    fn process_dynamic_string_string_vazia_e_invalida() {
        let local = std::collections::HashMap::new();
        let global = std::collections::HashMap::new();
        let r = super::process_dynamic_string(&local, &global, "");
        assert!(!r.valid);
    }

    // ===== expand_resource_path (ArchiveXL #48, RE 2026-08-11 — reusa #23) =====

    #[test]
    fn expand_resource_path_nao_dinamico_devolve_inalterado() {
        let ctx = std::collections::HashMap::new();
        let (path, optional) = super::expand_resource_path("materials/base.mi", "skin01", &ctx);
        assert_eq!(path, Some("materials/base.mi".to_string()));
        assert!(!optional);
    }

    #[test]
    fn expand_resource_path_substitui_atributo_material_local() {
        let ctx = std::collections::HashMap::new();
        let (path, optional) =
            super::expand_resource_path("*materials/{material}_diff.mi", "skin01", &ctx);
        assert_eq!(path, Some("materials/skin01_diff.mi".to_string()));
        assert!(!optional);
    }

    #[test]
    fn expand_resource_path_combina_material_local_com_atributo_global() {
        let mut ctx = std::collections::HashMap::new();
        ctx.insert(bwms_hashes::fnv1a64(b"region"), "head".to_string());
        let (path, optional) =
            super::expand_resource_path("*materials/{region}_{material}.mi", "skin01", &ctx);
        assert_eq!(path, Some("materials/head_skin01.mi".to_string()));
        assert!(!optional);
    }

    #[test]
    fn expand_resource_path_atributo_faltando_devolve_none() {
        let ctx = std::collections::HashMap::new();
        let (path, optional) = super::expand_resource_path("*materials/{unknown}.mi", "skin01", &ctx);
        assert_eq!(path, None);
        assert!(!optional);
    }

    #[test]
    fn expand_resource_path_atributo_faltando_com_marcador_opcional() {
        let ctx = std::collections::HashMap::new();
        let (path, optional) = super::expand_resource_path("*{unknown}?", "skin01", &ctx);
        assert_eq!(path, None);
        assert!(optional);
    }

    #[test]
    fn expand_resource_path_chave_nao_fechada_e_invalido() {
        let ctx = std::collections::HashMap::new();
        let (path, optional) = super::expand_resource_path("*materials/{material", "skin01", &ctx);
        assert_eq!(path, None);
        assert!(!optional);
    }

    // ===== DynamicAppearanceRef::parse/Match (ArchiveXL #22/#23, RE 2026-08-11) =====

    #[test]
    fn dynamicappearanceref_sem_marcador_nao_e_dinamico() {
        let r = super::DynamicAppearanceRef::parse("torso");
        assert!(!r.is_dynamic);
        assert!(!r.is_conditional);
        assert_eq!(r.name, r.value);
        assert!(r.variants.is_empty());
        assert!(r.conditions.is_empty());
        assert_eq!(r.weight, 0);
    }

    #[test]
    fn dynamicappearanceref_variantes_e_condicoes_combinadas() {
        let r = super::DynamicAppearanceRef::parse("torso!v1!v2&c1&c2");
        assert!(r.is_dynamic);
        assert!(r.is_conditional);
        assert_eq!(r.name, bwms_hashes::fnv1a64(b"torso"));
        assert_eq!(r.variants.len(), 2);
        assert!(r.variants.contains(&bwms_hashes::fnv1a64(b"v1")));
        assert!(r.variants.contains(&bwms_hashes::fnv1a64(b"v2")));
        assert_eq!(r.conditions.len(), 2);
        assert!(r.conditions.contains(&bwms_hashes::fnv1a64(b"c1")));
        assert!(r.conditions.contains(&bwms_hashes::fnv1a64(b"c2")));
        assert_eq!(r.weight, 102); // 100 (tem variante) + 2 condições
    }

    #[test]
    fn dynamicappearanceref_so_variantes_sem_condicao() {
        let r = super::DynamicAppearanceRef::parse("hair!short!long");
        assert!(r.is_dynamic);
        assert_eq!(r.variants.len(), 2);
        assert!(r.conditions.is_empty());
        assert_eq!(r.weight, 100);
        assert!(r.is_conditional);
    }

    #[test]
    fn dynamicappearanceref_so_condicoes_sem_variante() {
        let r = super::DynamicAppearanceRef::parse("eyes&male");
        assert!(r.is_dynamic);
        assert!(r.variants.is_empty());
        assert_eq!(r.conditions.len(), 1);
        assert!(r.conditions.contains(&bwms_hashes::fnv1a64(b"male")));
        assert_eq!(r.weight, 1); // 0 (sem variante) + 1 condição
    }

    #[test]
    fn dynamicappearanceref_comeca_direto_com_condicao_sem_nome() {
        let r = super::DynamicAppearanceRef::parse("&cond1");
        assert!(r.is_dynamic);
        assert_eq!(r.name, 0); // ExtractName("") = 0
        assert_eq!(r.conditions.len(), 1);
        assert!(r.conditions.contains(&bwms_hashes::fnv1a64(b"cond1")));
    }

    #[test]
    fn dynamicappearanceref_match_variant_e_conditions() {
        let r = super::DynamicAppearanceRef::parse("torso!v1&c1&c2");
        assert!(r.matches_variant(bwms_hashes::fnv1a64(b"v1")));
        assert!(!r.matches_variant(bwms_hashes::fnv1a64(b"v2")));

        let mut present = std::collections::BTreeSet::new();
        present.insert(bwms_hashes::fnv1a64(b"c1"));
        present.insert(bwms_hashes::fnv1a64(b"c2"));
        assert!(r.matches_conditions(&present));

        present.remove(&bwms_hashes::fnv1a64(b"c2"));
        assert!(!r.matches_conditions(&present)); // falta c2 -> não bate mais

        // mas se c2 vier via OVERRIDE, volta a bater
        let mut overrides = std::collections::BTreeSet::new();
        overrides.insert(bwms_hashes::fnv1a64(b"c2"));
        assert!(r.matches_conditions_with_overrides(&present, &overrides));
    }

    #[test]
    fn dynamicappearanceref_marcadores_consecutivos_geram_variante_vazia() {
        // "a!!b" -> variantes {hash(""), hash("b")} = {0, hash("b")}, sem crashar/travar.
        let r = super::DynamicAppearanceRef::parse("a!!b");
        assert!(r.is_dynamic);
        assert!(r.variants.contains(&0));
        assert!(r.variants.contains(&bwms_hashes::fnv1a64(b"b")));
    }

    // ===== DynamicAppearanceName::parse/MatchReference (ArchiveXL #23, RE 2026-08-15) =====

    #[test]
    fn dynamicappearancename_sem_marcador_nao_e_dinamico() {
        let n = super::DynamicAppearanceName::parse("torso");
        assert!(!n.is_dynamic);
        assert_eq!(n.name, n.value);
        assert_eq!(n.name, bwms_hashes::fnv1a64(b"torso"));
        assert_eq!(n.variant, 0);
        assert!(n.parts.is_empty());
        assert!(n.overrides.is_empty());
        assert_eq!(n.context, 0);
    }

    #[test]
    fn dynamicappearancename_variante_simples() {
        let n = super::DynamicAppearanceName::parse("torso!v1");
        assert!(n.is_dynamic);
        assert_eq!(n.name, bwms_hashes::fnv1a64(b"torso"));
        assert_eq!(n.variant, bwms_hashes::fnv1a64(b"v1"));
        // parts[hash("variant")] == variant inteiro
        assert_eq!(n.parts.get(&bwms_hashes::fnv1a64(b"variant")), Some(&n.variant));
        // + 1 entrada posicional sintética "variant.1"
        let seed = bwms_hashes::fnv1a64(b"variant");
        let synth_key = bwms_hashes::fnv1a64_seeded(&[b'.', b'1'], seed);
        assert_eq!(n.parts.get(&synth_key), Some(&bwms_hashes::fnv1a64(b"v1")));
        assert_eq!(n.parts.len(), 2);
        assert!(n.overrides.is_empty());
    }

    #[test]
    fn dynamicappearancename_condicao_antes_do_contexto_e_a_ordem_valida() {
        // ACHADO DE RE (não-óbvio, confirmado por leitura linha-a-linha do C++ real): a busca de
        // contexto (`find_last_of('%')`) roda sobre a string ORIGINAL, e o `ParseInt` exige que
        // TUDO da posição do '%' até o FIM da string seja um inteiro puro — então contexto só
        // parseia com sucesso se o `%numero` for LITERALMENTE o final da string. A ordem válida
        // é CONDIÇÃO antes de CONTEXTO: "nome!variante&condicao%numero" (não o inverso).
        let n = super::DynamicAppearanceName::parse("torso!v1&cond1%42");
        assert!(n.is_dynamic);
        assert_eq!(n.context, 42);
        assert_eq!(n.variant, bwms_hashes::fnv1a64(b"v1"));
    }

    #[test]
    fn dynamicappearancename_contexto_antes_da_condicao_e_ordem_invalida_fica_zero() {
        // A ordem INVERSA ("nome!variante%numero&condicao") faz o ParseInt falhar de propósito
        // (o trecho pós-'%' vira "42&cond1", não puramente numérico) — `context` fica 0. Não é
        // bug do nosso port: é o comportamento REAL do ArchiveXL original, confirmado replicando
        // o algoritmo exato (`ParseInt` sobre a string NÃO-truncada). Documentado aqui pra não
        // ser confundido com um bug nosso numa sessão futura.
        let n = super::DynamicAppearanceName::parse("torso!v1%42&cond1");
        assert!(n.is_dynamic);
        assert_eq!(n.context, 0);
        assert_eq!(n.variant, bwms_hashes::fnv1a64(b"v1"));
    }

    #[test]
    fn dynamicappearancename_contexto_invalido_fica_zero() {
        // "torso!v1%abc" -> "abc" não é um u64 válido -> ParseInt falha -> context permanece 0
        // (não seta lixo), mas a truncação da string ('%' em diante) AINDA acontece.
        let n = super::DynamicAppearanceName::parse("torso!v1%abc");
        assert_eq!(n.context, 0);
        assert_eq!(n.variant, bwms_hashes::fnv1a64(b"v1"));
    }

    #[test]
    fn dynamicappearancename_partes_posicionais_e_nomeadas_misturadas() {
        // "torso!v1+color=red" -> variant = STRING INTEIRA "v1+color=red" (fiel à fonte: o
        // trecho completo após '!' vira `variant`/parts[hash("variant")] ANTES de o loop
        // sequer separar por '+'); depois o loop também extrai parts["variant.1"]="v1" e
        // parts["color"]="red" + 1 entrada em `overrides` (índice "quirky", ver comentário no
        // código de produção).
        let n = super::DynamicAppearanceName::parse("torso!v1+color=red");
        assert!(n.is_dynamic);
        assert_eq!(n.variant, bwms_hashes::fnv1a64(b"v1+color=red"));
        assert_eq!(n.parts.get(&bwms_hashes::fnv1a64(b"variant")), Some(&n.variant));
        let seed = bwms_hashes::fnv1a64(b"variant");
        let synth_key = bwms_hashes::fnv1a64_seeded(&[b'.', b'1'], seed);
        assert_eq!(n.parts.get(&synth_key), Some(&bwms_hashes::fnv1a64(b"v1")));
        assert_eq!(n.parts.get(&bwms_hashes::fnv1a64(b"color")), Some(&bwms_hashes::fnv1a64(b"red")));
        assert_eq!(n.parts.len(), 3);
        assert_eq!(n.overrides.len(), 1);
        // valor exato do índice "quirky" (replicado fielmente, não é hash("color") nem
        // hash("red") nem hash("color=red")) — trava a regressão contra o comportamento real.
        assert!(n.overrides.contains(&bwms_hashes::fnv1a64(b"colo")));
    }

    #[test]
    fn dynamicappearancename_separador_inicial_e_pulado() {
        // "torso!+v1" -> '+' logo no início da parte -> pulado sem crashar, vira igual a "v1"
        // pro resto do parse.
        let n = super::DynamicAppearanceName::parse("torso!+v1");
        assert!(n.is_dynamic);
        // variant = string inteira pós-'!' ("+v1", incluindo o '+' — só o LOOP pula o separador
        // vazio, o campo `variant` já foi calculado antes do loop rodar).
        assert_eq!(n.variant, bwms_hashes::fnv1a64(b"+v1"));
    }

    #[test]
    fn dynamicappearancename_entrada_malformada_nao_panica() {
        // '%'/'&' aparecendo ANTES do '!' seria UB no C++ real (string_view fora dos limites);
        // aqui degrada com segurança pra não-dinâmico, sem crashar — divergência consciente
        // documentada no código.
        let n = super::DynamicAppearanceName::parse("%1&c!v1");
        assert!(!n.is_dynamic);
        assert_eq!(n.name, n.value);
    }

    #[test]
    fn dynamicappearancename_match_reference_variante_bate() {
        let reference = super::DynamicAppearanceRef::parse("torso!v1!v2");
        let appearance = super::DynamicAppearanceName::parse("torso!v1");
        assert!(super::appearance_matches_reference(&reference, &appearance, None));

        let appearance_no_match = super::DynamicAppearanceName::parse("torso!v9");
        assert!(!super::appearance_matches_reference(&reference, &appearance_no_match, None));
    }

    #[test]
    fn dynamicappearancename_match_reference_condicao_sem_estado_de_entidade_falha() {
        // ref com condições exige estado de entidade encontrado (`m_states.find`) — sem
        // conditions_present (equivalente a "entidade não registrada"), sempre falha, mesmo que
        // a variante bata.
        let reference = super::DynamicAppearanceRef::parse("torso!v1&c1");
        let appearance = super::DynamicAppearanceName::parse("torso!v1");
        assert!(!super::appearance_matches_reference(&reference, &appearance, None));
    }

    #[test]
    fn get_base_appearance_name_corta_no_primeiro_marcador_de_qualquer_tipo() {
        assert_eq!(super::get_base_appearance_name("torso!v1"), "torso");
        assert_eq!(super::get_base_appearance_name("torso%42"), "torso");
        assert_eq!(super::get_base_appearance_name("torso&cond"), "torso");
        assert_eq!(super::get_base_appearance_name("torso"), "torso"); // sem marcador -> inteira
        assert_eq!(super::get_base_appearance_name(""), "");
    }

    #[test]
    fn dynamicappearancename_match_reference_condicao_via_override_da_appearance() {
        let reference = super::DynamicAppearanceRef::parse("torso&c1");
        let appearance = super::DynamicAppearanceName::parse("torso!v1+c1=1"); // gera override "c1?"-like
        // as condições da referência precisam estar em `conditions_present` OU em
        // `appearance.overrides` — testamos os 2 ramos: sem nada presente, falha (o override
        // "quirky" desta appearance não é hash("c1") exato, ver teste anterior); com o hash
        // exato do override JÁ CONHECIDO inserido manualmente em `conditions_present`, passa.
        let mut present = std::collections::BTreeSet::new();
        present.insert(bwms_hashes::fnv1a64(b"c1"));
        assert!(super::appearance_matches_reference(&reference, &appearance, Some(&present)));
    }

    #[test]
    fn garment_overrides_forma_escalar() {
        // escalar = máscara já-computada, crua (sem inversão — ChunkMask(uint64_t): show=false).
        let src = "overrides:\n  tags:\n    t1:\n      c1: 0xff\n      c2: 15\n";
        let xl = parse_xl(src).unwrap();
        let t = &xl.garment_overrides[0];
        let c1 = &t.components.iter().find(|(n, _)| n == "c1").unwrap().1;
        let c2 = &t.components.iter().find(|(n, _)| n == "c2").unwrap().1;
        assert!(!c1.show);
        assert_eq!(c1.mask, 0xff);
        assert_eq!(c2.mask, 15);
    }

    #[test]
    fn garment_overrides_bate_com_chunkmask_real_do_full_body() {
        // Fecha o loop com o achado da RE do full-body (2026-07-15, cont.56): o chunkMask
        // 0xFFFFFFFFFFFFFF1F achado em `t0_000_pwa_fpp__01_ca_pale` (componente
        // t0_000_pwa_fpp__torso) é EXATAMENTE `hide: [5, 6, 7]` por esta fórmula.
        let src = "overrides:\n  tags:\n    t:\n      t0_000_pwa_fpp__torso:\n        hide:\n          - 5\n          - 6\n          - 7\n";
        let xl = parse_xl(src).unwrap();
        let mask = xl.garment_overrides[0].components[0].1.mask;
        assert_eq!(mask, 0xFFFFFFFFFFFFFF1Fu64);
    }

    // ===== quest.phases (QuestPhase/Config.cpp) =====

    #[test]
    fn quest_phases_path_parent_e_conexoes() {
        let src = "quest:\n  phases:\n    - path: my_phase\n      parent: root_graph\n      input:\n        node: [1, 2]\n        socket: In\n      output:\n        - 3\n        - 4\n      intercept: true\n";
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.quest_phases.len(), 1);
        let p = &xl.quest_phases[0];
        assert_eq!(p.phase_path, "my_phase");
        assert_eq!(p.parent, "root_graph");
        assert_eq!(p.input.node_path, vec![1, 2]);
        assert_eq!(p.input.socket, Some("In".to_string()));
        assert_eq!(p.output.node_path, vec![3, 4]);
        assert_eq!(p.output.socket, None);
        assert!(p.intercept);
        assert!(xl.other_sections.is_empty());
    }

    #[test]
    fn quest_phases_sem_path_ou_parent_e_descartada() {
        let src = "quest:\n  phases:\n    - path: only_path\n    - parent: only_parent\n    - path: ok\n      parent: ok_parent\n";
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.quest_phases.len(), 1);
        assert_eq!(xl.quest_phases[0].phase_path, "ok");
    }

    #[test]
    fn quest_phases_input_sobrescreve_connection() {
        // "connection" e "input" alimentam o MESMO campo; input (chamado por último) vence.
        let src = "quest:\n  phases:\n    - path: p\n      parent: r\n      connection: [9]\n      input: [1]\n";
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.quest_phases[0].input.node_path, vec![1]);
    }

    // ===== journal / player.bodyTypes / customizations / animations =====

    #[test]
    fn journal_scalar_e_seq() {
        let xl = parse_xl("journal: a.journal\n").unwrap();
        assert_eq!(xl.journals, vec!["a.journal"]);
        let xl2 = parse_xl("journal:\n  - a.journal\n  - b.journal\n").unwrap();
        assert_eq!(xl2.journals, vec!["a.journal", "b.journal"]);
        assert!(xl2.other_sections.is_empty());
    }

    #[test]
    fn puppet_state_body_types() {
        let xl = parse_xl("player:\n  bodyTypes:\n    - Male\n    - Female\n").unwrap();
        assert_eq!(xl.puppet_state.unwrap().body_types, vec!["Male", "Female"]);
    }

    #[test]
    fn customizations_male_female() {
        let src = "customizations:\n  male:\n    - a.app\n  female: b.app\n";
        let xl = parse_xl(src).unwrap();
        let c = xl.customization.unwrap();
        assert_eq!(c.male_options, vec!["a.app"]);
        assert_eq!(c.female_options, vec!["b.app"]);
    }

    #[test]
    fn animations_entrada_completa() {
        let src = "animations:\n  - entity:\n      - npc_a\n      - npc_b\n    set: my_set.animset\n    vars:\n      - v1\n      - v2\n    priority: 200\n    component: torso\n";
        let xl = parse_xl(src).unwrap();
        assert_eq!(xl.animations.len(), 1);
        let a = &xl.animations[0];
        assert_eq!(a.entities, vec!["npc_a", "npc_b"]);
        assert_eq!(a.set, "my_set.animset");
        assert_eq!(a.variables, vec!["v1", "v2"]);
        assert_eq!(a.priority, 200);
        assert_eq!(a.component, "torso");
    }

    #[test]
    fn animations_defaults_e_entity_scalar() {
        let src = "animations:\n  - entity: solo_npc\n    set: s.animset\n";
        let xl = parse_xl(src).unwrap();
        let a = &xl.animations[0];
        assert_eq!(a.entities, vec!["solo_npc"]);
        assert_eq!(a.priority, 128);
        assert_eq!(a.component, "root");
        assert!(a.variables.is_empty());
    }

    #[test]
    fn animations_sem_set_e_descartada() {
        let xl = parse_xl("animations:\n  - entity: x\n").unwrap();
        assert!(xl.animations.is_empty());
    }

    // ===== streaming.sectors (WorldStreaming/Config.cpp) =====

    #[test]
    fn streaming_blocks_e_sector_com_delecao() {
        let src = "streaming:\n  blocks:\n    - a.streamingblock\n  sectors:\n    - path: s.streamingsector\n      expectedNodes: 10\n      nodeDeletions:\n        - type: worldStaticMeshNode\n          index: 3\n";
        let xl = parse_xl(src).unwrap();
        let st = xl.streaming.unwrap();
        assert_eq!(st.blocks, vec!["a.streamingblock"]);
        assert_eq!(st.sectors.len(), 1);
        let sec = &st.sectors[0];
        assert_eq!(sec.path, "s.streamingsector");
        assert_eq!(sec.expected_nodes, 10);
        assert_eq!(sec.node_deletions.len(), 1);
        assert_eq!(sec.node_deletions[0].node_index, 3);
        assert_eq!(sec.node_deletions[0].node_type, "worldStaticMeshNode");
        assert!(xl.other_sections.is_empty());
    }

    #[test]
    fn streaming_indice_fora_do_range_e_descartado() {
        // expectedNodes=5, index=9 (fora) → deleção inteira descartada → sector some (0 del + 0 mut)
        let src = "streaming:\n  sectors:\n    - path: s.streamingsector\n      expectedNodes: 5\n      nodeDeletions:\n        - type: t\n          index: 9\n";
        let xl = parse_xl(src).unwrap();
        assert!(xl.streaming.is_none());
    }

    #[test]
    fn streaming_mutacao_position_3_e_4_valores_w_sempre_zero() {
        let src = "streaming:\n  sectors:\n    - path: s.streamingsector\n      expectedNodes: 5\n      nodeMutations:\n        - type: t\n          index: 0\n          position: [1.0, 2.0, 3.0]\n";
        let xl = parse_xl(src).unwrap();
        let m = &xl.streaming.unwrap().sectors[0].node_mutations[0];
        let pos = m.position.unwrap();
        assert_eq!((pos.x, pos.y, pos.z, pos.w), (1.0, 2.0, 3.0, 0.0));

        // forma com 4 valores: o 4º é aceito mas IGNORADO (w continua 0, fiel ao C++)
        let src2 = "streaming:\n  sectors:\n    - path: s.streamingsector\n      expectedNodes: 5\n      nodeMutations:\n        - type: t\n          index: 0\n          position: [1.0, 2.0, 3.0, 9.0]\n";
        let xl2 = parse_xl(src2).unwrap();
        let m2 = &xl2.streaming.unwrap().sectors[0].node_mutations[0];
        assert_eq!(m2.position.unwrap().w, 0.0);
    }

    #[test]
    fn streaming_mutacao_recursos_e_sinonimos_ultimo_presente_vence() {
        let src = "streaming:\n  sectors:\n    - path: s.streamingsector\n      expectedNodes: 5\n      nodeMutations:\n        - type: t\n          index: 0\n          resource: a.mesh\n          meshRef: b.mesh\n          appearance: default\n          recordID: Items.X\n";
        let xl = parse_xl(src).unwrap();
        let m = &xl.streaming.unwrap().sectors[0].node_mutations[0];
        // "meshRef" vem depois de "resource" na ordem de chaves-sinônimo → vence
        assert_eq!(m.resource_path.as_deref(), Some("b.mesh"));
        assert_eq!(m.appearance_name.as_deref(), Some("default"));
        assert_eq!(m.record_id.as_deref(), Some("Items.X"));
    }

    #[test]
    fn streaming_submutacoes_actor_com_expected_count() {
        let src = "streaming:\n  sectors:\n    - path: s.streamingsector\n      expectedNodes: 5\n      nodeMutations:\n        - type: t\n          index: 0\n          expectedActors: 3\n          actorMutations:\n            - index: 1\n              scale: [2.0, 2.0, 2.0]\n";
        let xl = parse_xl(src).unwrap();
        let m = &xl.streaming.unwrap().sectors[0].node_mutations[0];
        assert_eq!(m.expected_sub_nodes, 3);
        assert_eq!(m.sub_node_mutations.len(), 1);
        assert_eq!(m.sub_node_mutations[0].sub_node_index, 1);
        assert_eq!(m.sub_node_mutations[0].scale.unwrap().x, 2.0);
    }

    #[test]
    fn streaming_subdelecoes_sem_expected_count_e_ignorada() {
        // actorDeletions presente mas SEM expectedActors → parse_sub_deletions não faz nada
        let src = "streaming:\n  sectors:\n    - path: s.streamingsector\n      expectedNodes: 5\n      nodeDeletions:\n        - type: t\n          index: 0\n          actorDeletions:\n            - 1\n";
        let xl = parse_xl(src).unwrap();
        let d = &xl.streaming.unwrap().sectors[0].node_deletions[0];
        assert!(d.sub_node_deletions.is_empty());
        assert_eq!(d.expected_sub_nodes, 0);
    }

    #[test]
    fn streaming_sector_sem_delecao_ou_mutacao_e_omitido() {
        let src = "streaming:\n  sectors:\n    - path: s.streamingsector\n      expectedNodes: 5\n";
        let xl = parse_xl(src).unwrap();
        assert!(xl.streaming.is_none());
    }
}
