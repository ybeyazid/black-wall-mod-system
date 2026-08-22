//! Parser do formato-texto NATIVO `.tweak` do TweakXL (item TweakXL `#48`, catálogo
//! `CATALOGO-EXAUSTIVO-TWEAKXL.md`) — a 3ª via de autoria de mod, distinta do YAML já coberto
//! por `yaml.rs`/`tweakxl.rs`. Gramática espelhada 1:1 de `enablers/TweakXL/src/Red/TweakDB/
//! Source/Grammar.hpp` (PEG via `tao::pegtl` na fonte real) + semântica das `ParseAction` reais
//! de `Parser.cpp` (lido por completo, 2026-08-11 — nunca tinha sido lido antes desta sessão,
//! só a gramática/tokenização). Produz a MESMA `Vec<Op>` que o pipeline YAML já consome
//! (`tweakxl::Op`/`EditOp`) — reusa `apply_ops_runtime` existente sem duplicar nada runtime.
//!
//! Mapeamento semântico (confirmado lendo as `ParseAction` reais, não suposição):
//! - `TweakGroup{name, base}` (sem inline) → `base` não-vazio vira `Op::Clone{record:name,base}`
//!   (equivalente a `$base` do YAML); sem `base`, edita o record já existente direto (mesmo
//!   comportamento de um record YAML top-level sem `$base`/`$type`).
//! - `TweakFlat{name, operation, values, isArray}` → `group.name` vira o `flat` de 1+ `Op::Edit`.
//!   `Assign` sempre 1 op; `Append`/`Remove` com valor-array (`tags[] += [A,B,C];`) viram VÁRIOS
//!   `Op::Edit{EditOp::Append(X)}` (1 por elemento) — o `.tweak` real permite múltiplos elementos
//!   por statement (distinto do YAML, que tageia item-por-item); componível 1:1 na `EditOp`
//!   existente sem precisar de variante nova.
//! - `TweakValue` escalar (bool/number/string) → texto cru (sufixo `f` de float REMOVIDO, já
//!   que o resto do pipeline espera número puro parseável); struct (`Vector3` etc.) → componentes
//!   NUMÉRICOS comma-joined (mesmo formato `"f,f,…"` que `struct_map_to_scalar`/`encode_value`
//!   já esperam do lado YAML).
//!
//! Fora do escopo (erro claro, não parse silencioso errado — mesma filosofia de `tweakxl.rs`):
//! **expressões INLINE** (grupo aninhado como valor de flat, `flat_value: inline_expr`) e
//! **pacotes de schema (`RTDB`) / query (`Query`)** — nenhum mod real do Nexus usa `.tweak`
//! ainda (achado da triagem original, 2026-08-09: "não havia nenhum arquivo `.tweak` real
//! disponível pra testar"), então a prioridade foi cobrir o caso comum (mod regular: `package`
//! opcional, `using` opcional, groups com/sem herança, flats escalar/struct/array com `=`/`+=`/
//! `-=`) com fidelidade total, em vez de arriscar mapear semântica de schema/query sem exemplo
//! real pra validar contra (mesma disciplina já aplicada a `$dlc`/`$game`/`$instances`).

use crate::tweakxl::{EditOp, Op};

/// Entrada principal: parseia uma fonte `.tweak` inteira em uma lista de `Op`, na MESMA
/// representação que `tweakxl::interpret` produz a partir de YAML.
pub fn parse(src: &str) -> Result<Vec<Op>, String> {
    let mut p = P::new(src);
    p.skip_ws();

    if p.eat_keyword("package") {
        p.skip_ws1()?;
        let name = p.parse_package_id()?;
        p.skip_ws();
        if name == "RTDB" || name == "Query" {
            return Err(format!(
                "package '{name}' (schema/query) não suportado — fora do escopo desta implementação"
            ));
        }
    }
    if p.eat_keyword("using") {
        p.skip_ws1()?;
        p.parse_package_id()?;
        p.skip_ws();
        while p.eat_char(b',') {
            p.skip_ws();
            p.parse_package_id()?;
            p.skip_ws();
        }
    }

    let mut ops = Vec::new();
    loop {
        p.skip_ws();
        if p.at_end() {
            break;
        }
        p.parse_group(&mut ops)?;
    }
    Ok(ops)
}

#[derive(Clone, Copy, PartialEq)]
enum FlatOp {
    Assign,
    Append,
    Remove,
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

fn is_name_first(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}
fn is_name_other(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

impl<'a> P<'a> {
    fn new(s: &'a str) -> Self {
        P { s: s.as_bytes(), i: 0 }
    }
    fn at_end(&self) -> bool {
        self.i >= self.s.len()
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn rest_starts_with(&self, pat: &[u8]) -> bool {
        self.s[self.i..].starts_with(pat)
    }

    fn skip_ws(&mut self) {
        loop {
            while matches!(self.peek(), Some(b) if b.is_ascii_whitespace()) {
                self.i += 1;
            }
            if self.rest_starts_with(b"//") {
                while !self.at_end() && self.peek() != Some(b'\n') {
                    self.i += 1;
                }
                continue;
            }
            if self.rest_starts_with(b"/*") {
                self.i += 2;
                while !self.at_end() && !self.rest_starts_with(b"*/") {
                    self.i += 1;
                }
                self.i = (self.i + 2).min(self.s.len());
                continue;
            }
            break;
        }
    }

    /// Exige AO MENOS 1 espaço/comentário (separador obrigatório entre keyword e token, ex.
    /// `package Foo` — sem isso `packageFoo` seria mal-interpretado).
    fn skip_ws1(&mut self) -> Result<(), String> {
        let before = self.i;
        self.skip_ws();
        if self.i == before {
            return Err(format!("esperava espaço na posição {before}"));
        }
        Ok(())
    }

    fn eat_char(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect_char(&mut self, c: u8) -> Result<(), String> {
        if self.eat_char(c) {
            Ok(())
        } else {
            Err(format!("esperava '{}' na posição {}", c as char, self.i))
        }
    }

    fn eat_str(&mut self, s: &str) -> bool {
        if self.rest_starts_with(s.as_bytes()) {
            self.i += s.len();
            true
        } else {
            false
        }
    }

    /// Keyword só casa se NÃO for prefixo de um identificador maior (`package` não casa em
    /// `packageFoo`).
    fn eat_keyword(&mut self, kw: &str) -> bool {
        if self.rest_starts_with(kw.as_bytes()) {
            let next = self.s.get(self.i + kw.len()).copied();
            if !matches!(next, Some(b) if is_name_other(b)) {
                self.i += kw.len();
                return true;
            }
        }
        false
    }

    fn parse_ident(&mut self) -> Result<String, String> {
        let start = self.i;
        match self.peek() {
            Some(b) if is_name_first(b) => self.i += 1,
            _ => return Err(format!("esperava identificador na posição {start}")),
        }
        while matches!(self.peek(), Some(b) if is_name_other(b)) {
            self.i += 1;
        }
        Ok(std::str::from_utf8(&self.s[start..self.i]).unwrap().to_string())
    }

    /// `path`/`group_path`/`group_id` da gramática real são, na prática, todos "identificador
    /// com pontos permitidos no meio" — nunca precisamos distinguir package-qualificado de
    /// não-qualificado pra tradução em `Op` (o nome vira o `record`/`base` como está, cru,
    /// igual ao que `Parser.cpp::group_name`/`group_base` fazem — `in.string()` direto).
    fn parse_path(&mut self) -> Result<String, String> {
        let start = self.i;
        match self.peek() {
            Some(b) if is_name_first(b) => self.i += 1,
            _ => return Err(format!("esperava path na posição {start}")),
        }
        while matches!(self.peek(), Some(b) if is_name_other(b) || b == b'.') {
            self.i += 1;
        }
        Ok(std::str::from_utf8(&self.s[start..self.i]).unwrap().to_string())
    }

    /// `0x` + 9-10 dígitos hex (`hash` da gramática real).
    fn parse_hash(&mut self) -> Option<String> {
        if self.rest_starts_with(b"0x") {
            let start = self.i;
            self.i += 2;
            let hstart = self.i;
            while matches!(self.peek(), Some(b) if b.is_ascii_hexdigit()) {
                self.i += 1;
            }
            let n = self.i - hstart;
            if (9..=10).contains(&n) {
                return Some(std::str::from_utf8(&self.s[start..self.i]).unwrap().to_string());
            }
            self.i = start;
        }
        None
    }

    /// `package_id = sor<hash, name>` (sem pontos).
    fn parse_package_id(&mut self) -> Result<String, String> {
        if let Some(h) = self.parse_hash() {
            return Ok(h);
        }
        self.parse_ident()
    }

    /// `group_id`/`group_base`/`group_path` = `sor<hash, path>` (com pontos permitidos).
    fn parse_group_id(&mut self) -> Result<String, String> {
        if let Some(h) = self.parse_hash() {
            return Ok(h);
        }
        self.parse_path()
    }

    /// `tags = star<tag, _>`, `tag = [ _ tag_name _ ]` — coletadas e DESCARTADAS (sem via de
    /// aplicação em runtime ainda pra tags arbitrárias de record/flat, distinto das tags de
    /// OPERAÇÃO do YAML que viram `EditOp`).
    fn parse_tags(&mut self) -> Result<(), String> {
        loop {
            self.skip_ws();
            if !self.eat_char(b'[') {
                return Ok(());
            }
            self.skip_ws();
            self.parse_ident()?;
            self.skip_ws();
            self.expect_char(b']')?;
        }
    }

    fn parse_group(&mut self, ops: &mut Vec<Op>) -> Result<(), String> {
        self.parse_tags()?;
        self.skip_ws();
        let name = self.parse_group_id()?;
        self.skip_ws();
        let base = if self.eat_char(b':') {
            self.skip_ws();
            let b = self.parse_group_id()?;
            self.skip_ws();
            Some(b)
        } else {
            None
        };
        self.expect_char(b'{')?;
        if let Some(b) = &base {
            ops.push(Op::Clone { record: name.clone(), base: b.clone() });
        }
        loop {
            self.skip_ws();
            if self.eat_char(b'}') {
                break;
            }
            if self.at_end() {
                return Err(format!("'{name}': grupo sem fechar '}}'"));
            }
            self.parse_flat(&name, ops)?;
        }
        Ok(())
    }

    /// `fk<Name>` (foreign-key type) — só reconhecido, nunca vira valor de `Op` (o TIPO em si é
    /// metadado descartado, igual a todo `flat_type` — `Op::Edit` não carrega tipo, só valor).
    fn try_parse_fk(&mut self) -> Option<()> {
        let save = self.i;
        if !self.eat_str("fk<") {
            return None;
        }
        self.skip_ws();
        if self.parse_ident().is_err() {
            self.i = save;
            return None;
        }
        self.skip_ws();
        if !self.eat_char(b'>') {
            self.i = save;
            return None;
        }
        Some(())
    }

    /// `flat_stmt` começa com `tags, opt<flat_type>, flat_name` — `flat_type` (se presente) é
    /// SEMPRE seguido de espaço obrigatório antes do nome real (gramática: `flat_type = seq<
    /// type_expr, space>`). Distingue "token1 é o TIPO, token2 é o NOME" de "token1 já é o NOME
    /// (sem tipo declarado)" por LOOKAHEAD: se depois de token1 (+ opcional `[]`) + espaço vier
    /// OUTRO identificador, token1 era tipo; senão token1 já era o nome.
    fn parse_flat_name(&mut self) -> Result<String, String> {
        let save = self.i;
        // tenta consumir um token de TIPO candidato (fk<...> ou identificador simples).
        let consumed_fk = self.try_parse_fk().is_some();
        if !consumed_fk {
            if self.parse_ident().is_err() {
                self.i = save;
                return Err(format!("'{save}': esperava nome de flat"));
            }
        }
        // sufixo de array opcional, só faz sentido se token1 for tipo.
        let after_token1 = self.i;
        let _ = self.eat_str("[]");
        let after_array = self.i;
        let ws_before = self.i;
        self.skip_ws();
        let had_ws = self.i > ws_before;
        if had_ws && matches!(self.peek(), Some(b) if is_name_first(b)) {
            // token1 (+ opcional []) ERA o tipo — o nome de verdade vem agora.
            return self.parse_ident();
        }
        // token1 já era o nome — reverte qualquer `[]`/espaço que tenha comido de propósito
        // (não deveria ter comido nada de útil, já que nome nunca é seguido de `[]` antes do
        // operador em sintaxe válida, mas revertemos por segurança).
        self.i = after_token1;
        let _ = after_array;
        Ok(std::str::from_utf8(&self.s[save..self.i]).unwrap().to_string())
    }

    fn parse_flat(&mut self, group: &str, ops: &mut Vec<Op>) -> Result<(), String> {
        self.parse_tags()?;
        self.skip_ws();
        let name = self.parse_flat_name()?;
        self.skip_ws();
        let op = if self.eat_str("+=") {
            FlatOp::Append
        } else if self.eat_str("-=") {
            FlatOp::Remove
        } else if self.eat_char(b'=') {
            FlatOp::Assign
        } else {
            return Err(format!("'{group}.{name}': esperava '='/'+='/'-=' na posição {}", self.i));
        };
        self.skip_ws();
        let full = format!("{group}.{name}");
        self.parse_flat_value(&full, op, ops)?;
        self.skip_ws();
        self.expect_char(b';')?;
        Ok(())
    }

    /// `flat_value = sor<scalar_expr, struct_expr, array_expr, inline_expr>`.
    fn parse_flat_value(&mut self, flat: &str, op: FlatOp, ops: &mut Vec<Op>) -> Result<(), String> {
        match self.peek() {
            Some(b'(') => {
                let v = self.parse_struct_expr()?;
                push_scalar_op(ops, flat, op, v);
                Ok(())
            }
            Some(b'[') => self.parse_array_expr(flat, op, ops),
            Some(b'{') => Err(format!("'{flat}': expressão INLINE (grupo aninhado como valor) não suportada")),
            _ => {
                let v = self.parse_scalar_expr()?;
                push_scalar_op(ops, flat, op, v);
                Ok(())
            }
        }
    }

    /// `scalar_expr = sor<scalar_bool, scalar_number, scalar_string>`. Devolve o valor já
    /// FORMATADO pro resto do pipeline (`f` de float removido; string sem aspas).
    fn parse_scalar_expr(&mut self) -> Result<String, String> {
        if self.eat_str("true") {
            return Ok("true".into());
        }
        if self.eat_str("false") {
            return Ok("false".into());
        }
        if self.peek() == Some(b'"') {
            return self.parse_string_literal();
        }
        self.parse_number_literal()
    }

    fn parse_string_literal(&mut self) -> Result<String, String> {
        let start = self.i;
        self.expect_char(b'"')?;
        let content_start = self.i;
        while !self.at_end() && self.peek() != Some(b'"') {
            self.i += 1;
        }
        if self.at_end() {
            return Err(format!("string sem fechar (iniciada na posição {start})"));
        }
        let content = std::str::from_utf8(&self.s[content_start..self.i]).unwrap().to_string();
        self.i += 1; // consome a aspa final
        Ok(content)
    }

    /// `scalar_number = opt<'-'>, (.digits | digits(.digits)?), opt<'f'>` — devolve SEM o
    /// sufixo `f` (o resto do pipeline espera número puro parseável em `f32`/`i32`).
    fn parse_number_literal(&mut self) -> Result<String, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        let digits_start = self.i;
        if self.peek() == Some(b'.') {
            self.i += 1;
            let d0 = self.i;
            while matches!(self.peek(), Some(b) if b.is_ascii_digit()) {
                self.i += 1;
            }
            if self.i == d0 {
                self.i = start;
                return Err(format!("número inválido na posição {start}"));
            }
        } else {
            let d0 = self.i;
            while matches!(self.peek(), Some(b) if b.is_ascii_digit()) {
                self.i += 1;
            }
            if self.i == d0 {
                self.i = start;
                return Err(format!("número inválido na posição {start}"));
            }
            if self.peek() == Some(b'.') {
                self.i += 1;
                while matches!(self.peek(), Some(b) if b.is_ascii_digit()) {
                    self.i += 1;
                }
            }
        }
        let _ = digits_start;
        let text_end = self.i;
        let text = std::str::from_utf8(&self.s[start..text_end]).unwrap().to_string();
        self.eat_char(b'f'); // sufixo de float, descartado (não faz parte do valor numérico)
        Ok(text)
    }

    /// `struct_expr = ( num, num [, num [, num]] )` — 2 a 4 componentes numéricos. Devolve
    /// comma-joined SEM espaço, mesmo formato que `struct_map_to_scalar` já produz do lado YAML.
    fn parse_struct_expr(&mut self) -> Result<String, String> {
        self.expect_char(b'(')?;
        self.skip_ws();
        let mut parts = Vec::new();
        parts.push(self.parse_number_literal()?);
        self.skip_ws();
        self.expect_char(b',')?;
        self.skip_ws();
        parts.push(self.parse_number_literal()?);
        loop {
            self.skip_ws();
            if !self.eat_char(b',') {
                break;
            }
            self.skip_ws();
            parts.push(self.parse_number_literal()?);
            if parts.len() >= 4 {
                break;
            }
        }
        self.skip_ws();
        self.expect_char(b')')?;
        Ok(parts.join(","))
    }

    /// `array_expr = [ item (, item)* ]`, `item = sor<scalar_expr, struct_expr, inline_expr>`.
    /// `Assign` empacota tudo num `EditOp::Assign("[a, b, c]")` (formato já usado por
    /// `EditOp::Assign` de array no lado YAML); `Append`/`Remove` viram 1 `Op::Edit` POR
    /// ELEMENTO (mesmo efeito líquido, sem precisar de variante nova de `EditOp` "append-many").
    fn parse_array_expr(&mut self, flat: &str, op: FlatOp, ops: &mut Vec<Op>) -> Result<(), String> {
        self.expect_char(b'[')?;
        self.skip_ws();
        let mut items = Vec::new();
        while self.peek() != Some(b']') {
            if self.at_end() {
                return Err(format!("'{flat}': array sem fechar ']'"));
            }
            let v = match self.peek() {
                Some(b'(') => self.parse_struct_expr()?,
                Some(b'{') => return Err(format!("'{flat}': expressão INLINE dentro de array não suportada")),
                _ => self.parse_scalar_expr()?,
            };
            items.push(v);
            self.skip_ws();
            if self.eat_char(b',') {
                self.skip_ws();
            }
        }
        self.expect_char(b']')?;
        match op {
            FlatOp::Assign => {
                ops.push(Op::Edit { flat: flat.to_string(), op: EditOp::Assign(format!("[{}]", items.join(", "))) });
            }
            FlatOp::Append => {
                for v in items {
                    ops.push(Op::Edit { flat: flat.to_string(), op: EditOp::Append(v) });
                }
            }
            FlatOp::Remove => {
                for v in items {
                    ops.push(Op::Edit { flat: flat.to_string(), op: EditOp::Remove(v) });
                }
            }
        }
        Ok(())
    }
}

fn push_scalar_op(ops: &mut Vec<Op>, flat: &str, op: FlatOp, value: String) {
    let edit = match op {
        FlatOp::Assign => EditOp::Assign(value),
        FlatOp::Append => EditOp::Append(value),
        FlatOp::Remove => EditOp::Remove(value),
    };
    ops.push(Op::Edit { flat: flat.to_string(), op: edit });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Formata a lista de ops pra string legível, mesmo padrão de teste já usado em
    /// `tweakxl.rs` (evita precisar de `Debug`/`PartialEq` em `Op`/`EditOp`, que não derivam).
    fn fmt(ops: &[Op]) -> Vec<String> {
        ops.iter()
            .map(|o| match o {
                Op::Clone { record, base } => format!("clone {record} <- {base}"),
                Op::Create { record, class } => format!("create {record} : {class}"),
                Op::Edit { flat, op } => format!("edit {flat} {}", fmt_editop(op)),
            })
            .collect()
    }
    fn fmt_editop(op: &EditOp) -> String {
        match op {
            EditOp::Assign(v) => format!("= {v}"),
            EditOp::Append(v) => format!("append {v}"),
            EditOp::AppendOnce(v) => format!("append-once {v}"),
            EditOp::Prepend(v) => format!("prepend {v}"),
            EditOp::PrependOnce(v) => format!("prepend-once {v}"),
            EditOp::Remove(v) => format!("remove {v}"),
            EditOp::RemoveAll => "remove-all".to_string(),
            EditOp::AppendFrom(v) => format!("append-from {v}"),
            EditOp::PrependFrom(v) => format!("prepend-from {v}"),
        }
    }
    fn run(src: &str) -> Vec<String> {
        fmt(&parse(src).expect("parse falhou"))
    }
    /// `Op` não deriva `Debug` (mesma convenção de `tweakxl.rs`) — `.unwrap_err()` exigiria
    /// `Vec<Op>: Debug` pro branch Ok, então extrai o erro manualmente.
    fn err_of(src: &str) -> String {
        match parse(src) {
            Ok(_) => panic!("esperava erro, parse teve sucesso"),
            Err(e) => e,
        }
    }

    #[test]
    fn group_com_heranca_e_flats_escalares() {
        let s = run(
            "package MyMod\n\nItems.MyWeapon : Items.Preset_Lexington_Default {\n    int damage = 50;\n    float attackRadius = 3.5;\n    bool isMelee = false;\n}\n",
        );
        assert_eq!(
            s,
            vec![
                "clone Items.MyWeapon <- Items.Preset_Lexington_Default",
                "edit Items.MyWeapon.damage = 50",
                "edit Items.MyWeapon.attackRadius = 3.5",
                "edit Items.MyWeapon.isMelee = false",
            ]
        );
    }

    #[test]
    fn group_sem_heranca_edita_record_existente() {
        let s = run("Items.Existing {\n    damage = 99;\n}\n");
        assert_eq!(s, vec!["edit Items.Existing.damage = 99"]);
    }

    #[test]
    fn flat_sem_tipo_declarado() {
        // "damage = 99;" — sem tipo, `damage` já É o nome (achado do lookahead).
        let s = run("Items.A { damage = 1; }");
        assert_eq!(s, vec!["edit Items.A.damage = 1"]);
    }

    #[test]
    fn string_e_bool_e_float_com_sufixo() {
        let s = run("Items.A {\n  string label = \"Arma X\";\n  bool active = true;\n  float scale = 1.5f;\n}\n");
        assert_eq!(
            s,
            vec!["edit Items.A.label = Arma X", "edit Items.A.active = true", "edit Items.A.scale = 1.5"]
        );
    }

    #[test]
    fn numero_negativo_e_sem_parte_fracionaria() {
        let s = run("Items.A {\n  int offset = -3;\n  float half = .5;\n}\n");
        assert_eq!(s, vec!["edit Items.A.offset = -3", "edit Items.A.half = .5"]);
    }

    #[test]
    fn struct_vector3() {
        let s = run("Items.A {\n  Vector3 pos = (1.0, 2.5, -3.0);\n}\n");
        assert_eq!(s, vec!["edit Items.A.pos = 1.0,2.5,-3.0"]);
    }

    #[test]
    fn array_assign_vira_1_op_bracket() {
        let s = run("Items.A {\n  CName[] tags = [\"Tag1\", \"Tag2\", \"Tag3\"];\n}\n");
        assert_eq!(s, vec!["edit Items.A.tags = [Tag1, Tag2, Tag3]"]);
    }

    #[test]
    fn array_append_vira_1_op_por_elemento() {
        let s = run("Items.A {\n  tags += [\"Tag1\", \"Tag2\"];\n}\n");
        assert_eq!(s, vec!["edit Items.A.tags append Tag1", "edit Items.A.tags append Tag2"]);
    }

    #[test]
    fn array_remove_vira_1_op_por_elemento() {
        let s = run("Items.A {\n  tags -= [\"Tag1\", \"Tag2\"];\n}\n");
        assert_eq!(s, vec!["edit Items.A.tags remove Tag1", "edit Items.A.tags remove Tag2"]);
    }

    #[test]
    fn append_de_valor_escalar_unico_sem_colchetes() {
        let s = run("Items.A {\n  tags += \"SoloTag\";\n}\n");
        assert_eq!(s, vec!["edit Items.A.tags append SoloTag"]);
    }

    #[test]
    fn fk_type_e_using_statement_ignorados_sem_quebrar() {
        let s = run("package Mod\nusing Core, Items\n\nItems.A : Items.B {\n  fk<gamedataItem_Record> baseItem = \"Items.C\";\n}\n");
        assert_eq!(s, vec!["clone Items.A <- Items.B", "edit Items.A.baseItem = Items.C"]);
    }

    #[test]
    fn tags_de_record_e_de_flat_sao_ignoradas_sem_quebrar() {
        let s = run("[Vehicle] [Melee]\nItems.A : Items.B {\n  [Hidden] damage = 1;\n}\n");
        assert_eq!(s, vec!["clone Items.A <- Items.B", "edit Items.A.damage = 1"]);
    }

    #[test]
    fn comentarios_linha_e_bloco_ignorados() {
        let s = run("// comentário de linha\nItems.A { /* bloco\nmulti-linha */ damage = 1; // fim\n}\n");
        assert_eq!(s, vec!["edit Items.A.damage = 1"]);
    }

    #[test]
    fn multiplos_groups_no_mesmo_arquivo() {
        let s = run("Items.A : Items.X { damage = 1; }\nItems.B : Items.Y { damage = 2; }\n");
        assert_eq!(
            s,
            vec!["clone Items.A <- Items.X", "edit Items.A.damage = 1", "clone Items.B <- Items.Y", "edit Items.B.damage = 2"]
        );
    }

    #[test]
    fn hash_como_nome_de_group_e_flat_value() {
        // `hash = 0x + 9-10 hex digits` — forma alternativa de referenciar record por TweakDBID cru.
        let s = run("0x123456789 : Items.B { damage = 1; }\n");
        assert_eq!(s, vec!["clone 0x123456789 <- Items.B", "edit 0x123456789.damage = 1"]);
    }

    #[test]
    fn inline_expr_da_erro_claro_nao_parse_errado() {
        let err = err_of("Items.A {\n  baseItem = { damage = 1; };\n}\n");
        assert!(err.contains("INLINE"), "erro devia mencionar INLINE, veio: {err}");
    }

    #[test]
    fn pacote_schema_rtdb_da_erro_claro() {
        let err = err_of("package RTDB;\nItems.A { damage = 1; }\n");
        assert!(err.contains("RTDB"), "erro devia mencionar RTDB, veio: {err}");
    }

    #[test]
    fn pacote_query_da_erro_claro() {
        let err = err_of("package Query;\nItems.A { damage = 1; }\n");
        assert!(err.contains("Query"), "erro devia mencionar Query, veio: {err}");
    }

    #[test]
    fn grupo_sem_fechar_chave_da_erro() {
        assert!(parse("Items.A {\n  damage = 1;\n").is_err());
    }

    #[test]
    fn flat_sem_ponto_e_virgula_da_erro() {
        assert!(parse("Items.A {\n  damage = 1\n}\n").is_err());
    }
}
