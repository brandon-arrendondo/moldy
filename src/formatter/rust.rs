// Rust formatter: CST recursive-descent over tree-sitter-rust.
//
// Unlike the C/C++ formatter (which chases byte-for-byte parity with funky),
// there is no existing Rust formatter to match — funky has no Rust support at
// all. This formatter's job is to demonstrate that adding a new language on
// top of the substrate is cheap: idiomatic, rustfmt-flavored output, driven
// by a single recursive `emit_node` plus a handful of structural handlers for
// the constructs that need explicit newlines/indentation (blocks, item
// lists, match arms, struct/enum bodies, where-clauses).
//
// Rust's grammar is far more regular than C's declarator soup, so most nodes
// need no bespoke handler at all: `emit_generic` walks a node's direct
// children and inserts whitespace based on a small pairwise rule table
// (`ws_before`), recursing into `emit_node` for each child. Structural
// containers (anything that owns `{ ... }` or needs forced multi-line
// layout) get their own function; everything else — expressions, types,
// patterns — falls through the generic path.
//
// Attributes and macro invocations are treated as opaque, verbatim text —
// the same invariant this codebase already applies to C preprocessor lines.

use crate::config::Config;
use crate::error::MoldyError;
use tree_sitter::Node;

pub fn format(source: &str, config: &Config) -> Result<String, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key("rust")
        .ok_or_else(|| MoldyError::UnsupportedLanguage("rust".to_string()))?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))?;

    let mut fmt = Fmt::new(source, config);
    let root = tree.root_node();
    let mut cursor = root.walk();
    let items: Vec<Node> = root.children(&mut cursor).collect();
    fmt.emit_item_sequence(&items);
    Ok(fmt.finish())
}

enum Ws {
    None,
    Space,
    Newline,
}

/// Delimiter pair + whether an inline (single-line) rendering pads with a
/// space just inside the delimiters (struct literals: `Point { x: 1 }`) vs.
/// tight attachment (call args: `f(1, 2)`).
fn bracket_delims(kind: &str) -> Option<(&'static str, &'static str, bool)> {
    match kind {
        "arguments" => Some(("(", ")", false)),
        "parameters" => Some(("(", ")", false)),
        "tuple_expression" => Some(("(", ")", false)),
        "array_expression" => Some(("[", "]", false)),
        "tuple_type" => Some(("(", ")", false)),
        "tuple_pattern" => Some(("(", ")", false)),
        "closure_parameters" => Some(("|", "|", false)),
        "type_arguments" => Some(("<", ">", false)),
        "type_parameters" => Some(("<", ">", false)),
        "use_list" => Some(("{", "}", false)),
        "field_initializer_list" => Some(("{", "}", true)),
        "field_pattern_list" => Some(("{", "}", true)),
        "ordered_field_declaration_list" => Some(("(", ")", false)),
        _ => None,
    }
}

struct Fmt<'a> {
    src: &'a str,
    config: &'a Config,
    out: String,
    depth: u32,
}

impl<'a> Fmt<'a> {
    fn new(src: &'a str, config: &'a Config) -> Self {
        Fmt {
            src,
            config,
            out: String::with_capacity(src.len()),
            depth: 0,
        }
    }

    fn finish(mut self) -> String {
        let trimmed_len = self.out.trim_end_matches(['\n', '\r', ' ', '\t']).len();
        self.out.truncate(trimmed_len);
        if self.config.newlines.final_newline && !self.out.is_empty() {
            self.out.push('\n');
        }
        self.out
    }

    // ── Output primitives ─────────────────────────────────────────────────

    fn indent_str_at(&self, d: u32) -> String {
        use crate::config::IndentStyle;
        match self.config.indent.style {
            IndentStyle::Spaces => " ".repeat(self.config.indent.width as usize * d as usize),
            IndentStyle::Tabs => "\t".repeat(d as usize),
        }
    }

    fn raw(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.out.push_str(s);
    }

    fn nl(&mut self) {
        self.out.push('\n');
    }

    fn ensure_nl(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.nl();
        }
    }

    fn emit_indent(&mut self) {
        let s = self.indent_str_at(self.depth);
        self.raw(&s);
    }

    fn space_raw(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with(' ') && !self.out.ends_with('\n') {
            self.out.push(' ');
        }
    }

    fn node_text(&self, node: Node) -> &'a str {
        &self.src[node.start_byte()..node.end_byte()]
    }

    /// Column (in `char`s) of the current end of output, for width budgeting.
    fn current_column(&self) -> usize {
        match self.out.rfind('\n') {
            Some(i) => self.out[i + 1..].chars().count(),
            None => self.out.chars().count(),
        }
    }

    /// Render `f` into a scratch buffer instead of `self.out`, returning what
    /// it produced. Used to measure a candidate single-line rendering before
    /// committing to it (width-based wrapping, field-list collapsing).
    fn render_scratch<F: FnOnce(&mut Self)>(&mut self, f: F) -> String {
        let saved = std::mem::take(&mut self.out);
        f(self);
        std::mem::replace(&mut self.out, saved)
    }

    /// Count blank lines in source between `prev_end` and `start`, capped at
    /// `config.newlines.max_blank_lines`.
    fn source_blanks(&self, prev_end: usize, start: usize) -> usize {
        if start <= prev_end {
            return 0;
        }
        let gap = &self.src[prev_end..start];
        let newlines = gap.chars().filter(|&c| c == '\n').count();
        let prev_ends_with_nl =
            prev_end > 0 && self.src.as_bytes().get(prev_end - 1) == Some(&b'\n');
        let blanks = if prev_ends_with_nl {
            newlines
        } else {
            newlines.saturating_sub(1)
        };
        blanks.min(self.config.newlines.max_blank_lines as usize)
    }

    fn emit_blank_lines(&mut self, n: usize) {
        self.ensure_nl();
        for _ in 0..n {
            self.nl();
        }
    }

    // ── Item / statement sequencing (shared by source_file, blocks, and
    //    impl/trait/mod bodies — all are just "a sequence of things, each on
    //    its own line, blank lines preserved from source"). ────────────────

    fn emit_item_sequence(&mut self, items: &[Node]) {
        let mut prev_end: Option<usize> = None;
        for item in items {
            if let Some(pe) = prev_end {
                let blanks = self.source_blanks(pe, item.start_byte());
                self.emit_blank_lines(blanks);
            }
            self.ensure_nl();
            self.emit_indent();
            self.emit_node(*item);
            self.nl();
            prev_end = Some(item.end_byte());
        }
    }

    // ── Core dispatch ──────────────────────────────────────────────────────

    fn emit_node(&mut self, node: Node) {
        if node.child_count() == 0 {
            self.raw(self.node_text(node));
            return;
        }
        match node.kind() {
            "line_comment" | "block_comment" => {
                self.raw(self.node_text(node).trim_end_matches('\n'));
            }
            "string_literal" | "raw_string_literal" | "char_literal" => {
                // Never recurse into quote/content children generically —
                // there is no whitespace to insert inside a literal.
                self.raw(self.node_text(node));
            }
            "attribute_item"
            | "inner_attribute_item"
            | "macro_invocation"
            | "macro_definition"
            | "shebang" => {
                // Opaque, verbatim — the same invariant this codebase applies
                // to C preprocessor directives.
                self.raw(self.node_text(node).trim_end_matches('\n'));
            }
            "block" | "declaration_list" => self.emit_brace_block(node),
            "field_declaration_list" | "enum_variant_list" => self.emit_multiline_field_list(node),
            "match_block" => self.emit_match_block(node),
            "where_clause" => self.emit_where_clause(node),
            k if bracket_delims(k).is_some() => self.emit_bracket_list(node),
            _ => self.emit_generic(node),
        }
    }

    /// Fallback: walk direct children, inserting whitespace via `ws_before`,
    /// recursing into `emit_node`. Handles the bulk of Rust's expression,
    /// type, pattern, and item-signature grammar uniformly.
    fn emit_generic(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        self.emit_group(&children, node);
    }

    /// Emit an explicit sequence of sibling nodes (not necessarily all of a
    /// node's children — used for the comma-delimited groups inside bracket
    /// lists, where a single "item" can be more than one token, e.g. a tuple
    /// struct field's `pub String`). `parent` supplies context for the few
    /// `ws_before` rules that key off the enclosing node kind.
    fn emit_group(&mut self, group: &[Node], parent: Node) {
        let mut prev: Option<Node> = None;
        for &child in group {
            match self.ws_before(child, prev, parent) {
                Ws::None => {}
                Ws::Space => self.space_raw(),
                Ws::Newline => {
                    self.ensure_nl();
                    self.emit_indent();
                }
            }
            self.emit_node(child);
            prev = Some(child);
        }
    }

    fn ws_before(&self, cur: Node, prev: Option<Node>, parent: Node) -> Ws {
        let Some(prev) = prev else {
            return Ws::None;
        };
        let ck = cur.kind();
        let pk = prev.kind();
        let parent_kind = parent.kind();

        // A where-clause always starts on its own line, and whatever follows
        // it (the fn body, or `;`) does too.
        if ck == "where_clause" || pk == "where_clause" {
            return Ws::Newline;
        }
        // An attribute inside e.g. a match arm (`#[cfg(...)] Some(x) => ...`)
        // always sits on its own line above whatever it annotates. Item-level
        // attributes are already newline-separated by `emit_item_sequence`;
        // this covers the generic-fallback case (match arms, parameters).
        if matches!(pk, "attribute_item" | "inner_attribute_item") {
            return Ws::Newline;
        }

        // No space after these "attaching" tokens.
        if matches!(
            pk,
            "(" | "[" | "::" | "." | ".." | "..=" | "#" | "'" | "!" | "?"
        ) {
            return Ws::None;
        }
        if pk == "&"
            && matches!(
                parent_kind,
                "reference_expression" | "reference_type" | "self_parameter" | "reference_pattern"
            )
        {
            return Ws::None;
        }
        if matches!(pk, "-" | "!" | "*")
            && matches!(parent_kind, "unary_expression" | "negative_literal")
        {
            return Ws::None;
        }
        if pk == "*" && parent_kind == "pointer_type" {
            return Ws::None;
        }

        // No space before these "attached" tokens.
        if matches!(ck, ")" | "]" | "," | ";" | "::" | "." | ".." | "..=" | ":") {
            return Ws::None;
        }
        if ck == "?" && parent_kind == "try_expression" {
            return Ws::None;
        }
        if ck == "(" && matches!(parent_kind, "tuple_struct_pattern" | "visibility_modifier") {
            return Ws::None;
        }
        if ck == "[" && parent_kind == "index_expression" {
            return Ws::None;
        }
        if ck == ">" && matches!(parent_kind, "type_arguments" | "type_parameters") {
            return Ws::None;
        }
        // A bounds list (`T: Clone + Default`) is a child node whose own
        // first token is `:` — the space belongs *inside* it, not before it.
        if ck == "trait_bounds" {
            return Ws::None;
        }

        // These delimited lists attach directly to whatever names them
        // (function/call parens, generic argument lists, tuple-struct
        // fields, use-lists). Other bracketed forms — tuple/array literals,
        // tuple types and patterns — are ordinary values/types in their own
        // right and take normal spacing from whatever precedes them.
        if matches!(
            ck,
            "parameters"
                | "arguments"
                | "type_arguments"
                | "type_parameters"
                | "use_list"
                | "ordered_field_declaration_list"
        ) {
            return Ws::None;
        }

        Ws::Space
    }

    // ── Structural containers ─────────────────────────────────────────────

    /// `{ ... }` bodies with no other syntax of their own: blocks, and
    /// impl/trait/mod bodies (`declaration_list`).
    fn emit_brace_block(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let start_i = if children.first().map(|n| n.kind()) == Some("{") {
            1
        } else {
            0
        };
        let end_i = if children.last().map(|n| n.kind()) == Some("}") {
            children.len() - 1
        } else {
            children.len()
        };
        let items = &children[start_i..end_i];

        if items.is_empty() {
            self.raw("{}");
            return;
        }

        self.raw("{");
        self.nl();
        self.depth += 1;
        self.emit_item_sequence(items);
        self.depth -= 1;
        self.ensure_nl();
        self.emit_indent();
        self.raw("}");
    }

    /// Named-struct fields / enum variants: rustfmt always puts these one
    /// per line with a trailing comma, regardless of how the source wrote
    /// them. Leading doc comments and attributes ride along with no comma.
    fn emit_multiline_field_list(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let open = children.first().map(|n| n.kind()) == Some("{");
        let start_i = if open { 1 } else { 0 };
        let end_i = if children.last().map(|n| n.kind()) == Some("}") {
            children.len() - 1
        } else {
            children.len()
        };
        let items: Vec<Node> = children[start_i..end_i]
            .iter()
            .copied()
            .filter(|n| n.kind() != ",")
            .collect();

        if items.is_empty() {
            self.raw("{}");
            return;
        }

        if self.config.rust.collapse_field_lists && self.try_emit_collapsed_field_list(&items) {
            return;
        }

        self.raw("{");
        self.nl();
        self.depth += 1;
        let mut prev_end = children
            .first()
            .map(|n| n.end_byte())
            .unwrap_or(node.start_byte());
        for item in &items {
            let blanks = self.source_blanks(prev_end, item.start_byte());
            self.emit_blank_lines(blanks);
            self.ensure_nl();
            self.emit_indent();
            self.emit_node(*item);
            if !matches!(
                item.kind(),
                "line_comment" | "block_comment" | "attribute_item" | "inner_attribute_item"
            ) {
                self.raw(",");
            }
            self.nl();
            prev_end = item.end_byte();
        }
        self.depth -= 1;
        self.emit_indent();
        self.raw("}");
    }

    /// Try to render a struct/enum field list as `{ a: T, b: U }` on one
    /// line. Only valid when every field is plain (no doc comment or
    /// attribute riding along, which rustfmt always keeps on its own line)
    /// and the rendered form fits within `rust.max_width`. Returns `false`
    /// (emitting nothing) if collapsing isn't possible, so the caller can
    /// fall back to the one-per-line form.
    fn try_emit_collapsed_field_list(&mut self, items: &[Node]) -> bool {
        if items.iter().any(|n| {
            matches!(
                n.kind(),
                "line_comment" | "block_comment" | "attribute_item" | "inner_attribute_item"
            )
        }) {
            return false;
        }

        let current_col = self.current_column();
        let inline = self.render_scratch(|s| {
            s.raw(" ");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    s.raw(", ");
                }
                s.emit_node(*item);
            }
            s.raw(" ");
        });

        let total_width = current_col + 1 + inline.chars().count() + 1;
        if inline.contains('\n') || total_width > self.config.rust.max_width as usize {
            return false;
        }

        self.raw("{");
        self.raw(&inline);
        self.raw("}");
        true
    }

    /// `match` arms: forced one-per-line. The trailing comma is whatever the
    /// source's `match_arm` already carries (tree-sitter's grammar makes it
    /// optional exactly when the arm body is a block), so no comma logic is
    /// needed here — just indentation.
    fn emit_match_block(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let start_i = if children.first().map(|n| n.kind()) == Some("{") {
            1
        } else {
            0
        };
        let end_i = if children.last().map(|n| n.kind()) == Some("}") {
            children.len() - 1
        } else {
            children.len()
        };
        let items = &children[start_i..end_i];

        if items.is_empty() {
            self.raw("{}");
            return;
        }

        self.raw("{");
        self.nl();
        self.depth += 1;
        self.emit_item_sequence(items);
        self.depth -= 1;
        self.ensure_nl();
        self.emit_indent();
        self.raw("}");
    }

    /// `where` clauses always break: `where` on its own line, one bound per
    /// line indented, trailing comma on each — matching rustfmt's default
    /// style for any function that has one at all.
    fn emit_where_clause(&mut self, node: Node) {
        self.raw("where");
        self.nl();
        self.depth += 1;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "where" | "," => {}
                _ => {
                    self.emit_indent();
                    self.emit_node(child);
                    self.raw(",");
                    self.nl();
                }
            }
        }
        self.depth -= 1;
    }

    /// Parenthesized/bracketed comma lists: arguments, parameters, tuples,
    /// arrays, generic argument/parameter lists, use-lists, struct-literal
    /// field lists, etc. Single-line if the source had no newline in the
    /// span, otherwise one item per line with a trailing comma (except a
    /// trailing `..base` functional-update, which can't take one).
    fn emit_bracket_list(&mut self, node: Node) {
        let (open, close, pad) =
            bracket_delims(node.kind()).expect("dispatched only for known kinds");

        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let start_i = if children.first().map(|n| n.kind()) == Some(open) {
            1
        } else {
            0
        };
        let end_i = if children.last().map(|n| n.kind()) == Some(close) {
            children.len() - 1
        } else {
            children.len()
        };
        // Group by comma boundaries rather than treating every non-comma
        // child as its own item: some list kinds pack more than one token
        // into a single item with no wrapping node (e.g. an
        // `ordered_field_declaration_list` field like `pub String`).
        let mut groups: Vec<Vec<Node>> = vec![Vec::new()];
        for &child in &children[start_i..end_i] {
            if child.kind() == "," {
                groups.push(Vec::new());
            } else {
                groups.last_mut().unwrap().push(child);
            }
        }
        groups.retain(|g| !g.is_empty());

        self.raw(open);
        if groups.is_empty() {
            self.raw(close);
            return;
        }

        // Only explode into one-item-per-line when there's more than one
        // item and either the source already spanned multiple lines, or
        // (with `rust.width_based_wrapping`) the single-line rendering
        // wouldn't fit within `rust.max_width`. A single item (e.g. a
        // closure or struct literal as the sole call argument) stays hugged
        // against its own parens/brackets — its content handles its own
        // line breaks.
        let needs_break = if groups.len() <= 1 {
            false
        } else if self.config.rust.width_based_wrapping {
            let current_col = self.current_column();
            let inline = self.render_scratch(|s| {
                if pad {
                    s.raw(" ");
                }
                for (i, group) in groups.iter().enumerate() {
                    if i > 0 {
                        s.raw(", ");
                    }
                    s.emit_group(group, node);
                }
                if pad {
                    s.raw(" ");
                }
            });
            let total_width = current_col + open.len() + inline.chars().count() + close.len();
            inline.contains('\n') || total_width > self.config.rust.max_width as usize
        } else {
            self.src[node.start_byte()..node.end_byte()].contains('\n')
        };
        if !needs_break {
            if pad {
                self.raw(" ");
            }
            for (i, group) in groups.iter().enumerate() {
                if i > 0 {
                    self.raw(", ");
                }
                self.emit_group(group, node);
            }
            if pad {
                self.raw(" ");
            }
            self.raw(close);
        } else {
            self.nl();
            self.depth += 1;
            for group in &groups {
                self.emit_indent();
                self.emit_group(group, node);
                let is_base_update =
                    group.last().map(|n| n.kind()) == Some("base_field_initializer");
                if !is_base_update {
                    self.raw(",");
                }
                self.nl();
            }
            self.depth -= 1;
            self.emit_indent();
            self.raw(close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format;
    use crate::config::Config;

    fn fmt_with(config: &Config, src: &str) -> String {
        format(src, config).unwrap_or_else(|e| panic!("format failed: {e}"))
    }

    #[test]
    fn default_config_always_explodes_field_lists() {
        let src = "struct Point { x: i32, y: i32 }\n";
        let out = fmt_with(&Config::default(), src);
        assert_eq!(out, "struct Point {\n    x: i32,\n    y: i32,\n}\n");
    }

    #[test]
    fn collapse_field_lists_keeps_short_struct_on_one_line() {
        let mut config = Config::default();
        config.rust.collapse_field_lists = true;
        let src = "struct Point {\n    x: i32,\n    y: i32,\n}\n";
        let out = fmt_with(&config, src);
        assert_eq!(out, "struct Point { x: i32, y: i32 }\n");
    }

    #[test]
    fn collapse_field_lists_still_explodes_when_over_width() {
        let mut config = Config::default();
        config.rust.collapse_field_lists = true;
        config.rust.max_width = 20;
        let src = "struct Point {\n    x: i32,\n    y: i32,\n}\n";
        let out = fmt_with(&config, src);
        assert_eq!(out, "struct Point {\n    x: i32,\n    y: i32,\n}\n");
    }

    #[test]
    fn collapse_field_lists_skips_fields_with_comments() {
        let mut config = Config::default();
        config.rust.collapse_field_lists = true;
        let src = "struct Point {\n    // the x coordinate\n    x: i32,\n    y: i32,\n}\n";
        let out = fmt_with(&config, src);
        assert!(out.contains('\n'));
        assert!(out.starts_with("struct Point {\n"));
    }

    #[test]
    fn default_config_preserves_source_single_line_call() {
        let src = "fn call() {\n    bar(alpha, beta, gamma, delta, epsilon, zeta);\n}\n";
        let out = fmt_with(&Config::default(), src);
        assert_eq!(
            out,
            "fn call() {\n    bar(alpha, beta, gamma, delta, epsilon, zeta);\n}\n"
        );
    }

    #[test]
    fn width_based_wrapping_breaks_call_args_over_budget() {
        let mut config = Config::default();
        config.rust.width_based_wrapping = true;
        config.rust.max_width = 40;
        let src = "fn call() {\n    bar(alpha, beta, gamma, delta, epsilon, zeta);\n}\n";
        let out = fmt_with(&config, src);
        assert_eq!(
            out,
            "fn call() {\n    bar(\n        alpha,\n        beta,\n        gamma,\n        delta,\n        epsilon,\n        zeta,\n    );\n}\n"
        );
    }

    #[test]
    fn width_based_wrapping_keeps_short_call_inline() {
        let mut config = Config::default();
        config.rust.width_based_wrapping = true;
        config.rust.max_width = 40;
        let src = "fn call() {\n    foo(1, 2, 3);\n}\n";
        let out = fmt_with(&config, src);
        assert_eq!(out, "fn call() {\n    foo(1, 2, 3);\n}\n");
    }

    #[test]
    fn width_based_wrapping_is_idempotent() {
        let mut config = Config::default();
        config.rust.width_based_wrapping = true;
        config.rust.collapse_field_lists = true;
        config.rust.max_width = 40;
        let src = "struct Big {\n    name: String,\n    description: String,\n    created_at: u64,\n}\n\nfn call() {\n    bar(alpha, beta, gamma, delta, epsilon, zeta);\n}\n";
        let pass1 = format(src, &config).unwrap();
        let pass2 = format(&pass1, &config).unwrap();
        assert_eq!(pass1, pass2);
    }
}
