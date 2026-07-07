// Python formatter: CST recursive-descent, same architecture as rust.rs —
// a generic `emit_node`/`ws_before` pairwise-spacing walk for the bulk of
// the grammar, with dedicated handlers only for constructs that need real
// structural logic (statement sequences with PEP8 blank-line rules,
// indented blocks, bracketed comma lists that explode by width).
//
// Unlike C (funky) and Rust (rustfmt), there is no single formatter to
// diff against for parity: PEP8 is a style guide, not a tool. The
// structural target is `ruff format` (Black-compatible); `flake8` is used
// as a lint gate to confirm output is PEP8-clean. See
// tests/python_corpus_test.rs and `Config.python`.
//
// Python's grammar is whitespace-significant, but tree-sitter-python does
// not expose NEWLINE/INDENT/DEDENT as CST nodes — a `block` is just an
// ordered list of statement nodes, so indentation is purely something we
// track ourselves (`self.depth`), exactly like Rust's `{ ... }` blocks
// minus the braces.
//
// Known gaps (documented rather than half-implemented):
//   - Strings (including f-strings) are treated fully opaque: quote style
//     and interpolation contents are passed through verbatim, unlike
//     Black's quote normalization.
//   - Comprehensions, subscripts/slices, and multi-target assignments fall
//     through the generic path with no width-based wrapping.
//   - Magic-trailing-comma preservation (Black keeps a list exploded if the
//     source already has a trailing comma) is not implemented — the break
//     decision is width-only.

use super::output::OutputOps;
use crate::config::Config;
use crate::error::MoldyError;
use tree_sitter::Node;

pub fn format(source: &str, config: &Config) -> Result<String, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key("python")
        .ok_or_else(|| MoldyError::UnsupportedLanguage("python".to_string()))?;

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
    let items: Vec<Node> = root
        .children(&mut cursor)
        .filter(|n| n.kind() != ";")
        .collect();
    fmt.emit_stmt_sequence(&items, false);
    Ok(fmt.finish())
}

enum Ws {
    None,
    Space,
    /// Start a new line at the current indentation depth (used when
    /// resuming a header after a nested `block` — e.g. `else`/`except`).
    Newline,
    /// Start a new line with no indentation of its own — used right before
    /// a `block`, whose statement sequence indents its own first line at
    /// the deeper depth.
    NewlineBare,
}

/// Bracketed comma lists that get real explode-by-width handling. Other
/// bracketed forms (subscripts, comprehensions, generator expressions) fall
/// through the generic path — see module docs.
fn bracket_delims(kind: &str) -> Option<(&'static str, &'static str)> {
    match kind {
        "parameters" => Some(("(", ")")),
        "argument_list" => Some(("(", ")")),
        "tuple" => Some(("(", ")")),
        "list" => Some(("[", "]")),
        "set" => Some(("{", "}")),
        "dictionary" => Some(("{", "}")),
        _ => None,
    }
}

/// Black "hugs" `**` (no surrounding spaces) when both operands are simple:
/// a name, a numeric/bool/None literal, an attribute-access chain of those,
/// or one of those with a leading unary sign. Anything else (calls,
/// subscripts, parenthesized expressions, ...) keeps the spaces.
fn is_simple_power_operand(node: Node) -> bool {
    match node.kind() {
        "identifier" | "integer" | "float" | "true" | "false" | "none" => true,
        "attribute" => node
            .child_by_field_name("object")
            .is_some_and(is_simple_power_operand),
        "unary_operator" => node
            .child_by_field_name("argument")
            .is_some_and(is_simple_power_operand),
        _ => false,
    }
}

/// A bare string expression statement — used to detect a leading docstring.
fn is_docstring(node: Node) -> bool {
    node.kind() == "expression_statement"
        && node.named_child(0).is_some_and(|c| c.kind() == "string")
}

/// Statement/definition kinds that PEP8 surrounds with forced blank lines
/// (2 at module level, 1 when nested).
fn is_def_like(node: Node) -> bool {
    match node.kind() {
        "function_definition" | "class_definition" => true,
        "decorated_definition" => node
            .named_child(node.named_child_count().saturating_sub(1))
            .is_some_and(|c| matches!(c.kind(), "function_definition" | "class_definition")),
        _ => false,
    }
}

struct Fmt<'a> {
    src: &'a str,
    config: &'a Config,
    out: String,
    depth: u32,
}

impl<'a> OutputOps<'a> for Fmt<'a> {
    fn src(&self) -> &'a str {
        self.src
    }

    fn config(&self) -> &Config {
        self.config
    }

    fn depth(&self) -> u32 {
        self.depth
    }

    fn out(&self) -> &str {
        &self.out
    }

    fn out_mut(&mut self) -> &mut String {
        &mut self.out
    }
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

    /// Blank lines allowed between statements at the current depth: PEP8
    /// caps preserved blank lines at 2 in module scope, 1 inside any
    /// indented block.
    fn blank_line_cap(&self) -> usize {
        if self.depth == 0 {
            2
        } else {
            1
        }
    }

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
        blanks.min(self.blank_line_cap())
    }

    // ── Statement sequencing (module body and block bodies) ────────────────

    /// PEP8 forces exactly `blank_line_cap()` blank lines around a
    /// def/class-like statement (unless it's the first statement in the
    /// sequence); otherwise preserved source blank lines are used, capped.
    fn blanks_before(&self, prev: Node, cur: Node) -> usize {
        if is_def_like(cur) || is_def_like(prev) {
            self.blank_line_cap()
        } else {
            self.source_blanks(prev.end_byte(), cur.start_byte())
        }
    }

    /// `items` is a module body or a `block`'s statements; `is_class_body`
    /// enables Black's rule of forcing exactly one blank line after a
    /// class's leading docstring, regardless of source or def-ness.
    fn emit_stmt_sequence(&mut self, items: &[Node], is_class_body: bool) {
        let mut force_blank_after_docstring =
            is_class_body && items.first().is_some_and(|n| is_docstring(*n));
        let mut prev: Option<Node> = None;
        let mut i = 0;
        while i < items.len() {
            let item = items[i];
            if let Some(p) = prev {
                let mut blanks = self.blanks_before(p, item);
                if force_blank_after_docstring {
                    blanks = blanks.max(1);
                    force_blank_after_docstring = false;
                }
                self.emit_blank_lines(blanks);
            }
            self.ensure_nl();
            self.emit_indent();
            self.emit_node(item);
            prev = Some(item);

            // A comment starting on the same source line as the statement
            // just emitted is a trailing comment — keep it on that line
            // instead of starting a new one.
            if let Some(next) = items.get(i + 1) {
                if next.kind() == "comment" && next.start_position().row == item.end_position().row
                {
                    self.raw("  ");
                    self.emit_node(*next);
                    prev = Some(*next);
                    i += 1;
                }
            }

            // A simple statement needs a fresh trailing newline; a compound
            // statement's own nested block already ended with one.
            self.ensure_nl();
            i += 1;
        }
    }

    // ── Core dispatch ────────────────────────────────────────────────────

    fn emit_node(&mut self, node: Node) {
        if node.child_count() == 0 {
            self.raw(self.node_text(node));
            return;
        }
        match node.kind() {
            "comment" => self.raw(self.node_text(node).trim_end()),
            // Opaque, verbatim — including f-string interpolations. See
            // module docs for the tradeoff.
            "string" => self.raw(self.node_text(node)),
            "block" => self.emit_block(node),
            k if bracket_delims(k).is_some() => self.emit_bracket_list(node),
            _ => self.emit_generic(node),
        }
    }

    fn emit_generic(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        self.emit_group(&children, node);
    }

    fn emit_group(&mut self, group: &[Node], parent: Node) {
        let mut prev: Option<Node> = None;
        for &child in group {
            match self.ws_before(child, prev, parent) {
                Ws::None => {}
                Ws::Space => self.space(),
                Ws::Newline => {
                    self.ensure_nl();
                    self.emit_indent();
                }
                Ws::NewlineBare => self.ensure_nl(),
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

        // A suite's block starts on its own line; its own statement
        // sequence handles indenting that first line at the deeper depth.
        if ck == "block" {
            return Ws::NewlineBare;
        }
        // Whatever follows a block (elif/else/except/finally, or the next
        // header entirely) resumes on a new line at the current depth.
        if pk == "block" {
            return Ws::Newline;
        }
        // Stacked decorators, and the def/class they annotate, each get
        // their own line.
        if pk == "decorator" {
            return Ws::Newline;
        }

        // No space after these "attaching" tokens.
        if matches!(pk, "(" | "[" | "." | "@" | "~") {
            return Ws::None;
        }
        if matches!(pk, "-" | "+" | "*" | "**")
            && matches!(
                parent_kind,
                "unary_operator"
                    | "list_splat"
                    | "list_splat_pattern"
                    | "dictionary_splat"
                    | "dictionary_splat_pattern"
            )
        {
            return Ws::None;
        }
        if pk == ":" && parent_kind == "slice" {
            return Ws::None;
        }
        if pk == "=" && matches!(parent_kind, "keyword_argument" | "default_parameter") {
            return Ws::None;
        }
        if (ck == "**" || pk == "**") && parent_kind == "binary_operator" {
            let simple = parent
                .child_by_field_name("operator")
                .is_some_and(|op| op.kind() == "**")
                && parent
                    .child_by_field_name("left")
                    .is_some_and(is_simple_power_operand)
                && parent
                    .child_by_field_name("right")
                    .is_some_and(is_simple_power_operand);
            if simple {
                return Ws::None;
            }
        }

        // No space before these "attached" tokens.
        if matches!(ck, ")" | "]" | "}" | "," | ":" | ";" | ".") {
            return Ws::None;
        }
        if ck == "[" && parent_kind == "subscript" {
            return Ws::None;
        }
        if ck == ":" && parent_kind == "slice" {
            return Ws::None;
        }
        if ck == "=" && matches!(parent_kind, "keyword_argument" | "default_parameter") {
            return Ws::None;
        }
        // Call/definition argument and parameter lists attach directly to
        // whatever names them; other bracketed forms (list/tuple/set/dict
        // literals) are values in their own right and take normal spacing.
        if matches!(ck, "parameters" | "argument_list") {
            return Ws::None;
        }

        Ws::Space
    }

    // ── Structural containers ────────────────────────────────────────────

    fn emit_block(&mut self, node: Node) {
        self.depth += 1;
        let is_class_body = node
            .parent()
            .is_some_and(|p| p.kind() == "class_definition");
        let mut cursor = node.walk();
        let items: Vec<Node> = node
            .children(&mut cursor)
            .filter(|n| n.kind() != ";")
            .collect();
        self.emit_stmt_sequence(&items, is_class_body);
        self.depth -= 1;
    }

    /// Comma-delimited call/definition argument and parameter lists,
    /// tuples, lists, sets, and dicts. Mirrors Black's "right-hand split":
    ///   1. single line, if it fits within `python.max_width`;
    ///   2. else the whole body hugged onto one indented line, no trailing
    ///      comma, if *that* fits;
    ///   3. else one item per line with a trailing comma.
    ///
    /// Magic-trailing-comma preservation (forcing tier 3 because the source
    /// already had one) is not implemented.
    fn emit_bracket_list(&mut self, node: Node) {
        let (open, close) = bracket_delims(node.kind()).expect("dispatched only for known kinds");

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

        let current_col = self.current_column();
        let inline = self.render_scratch(|s| {
            for (i, group) in groups.iter().enumerate() {
                if i > 0 {
                    s.raw(", ");
                }
                s.emit_group(group, node);
            }
        });
        let max_width = self.config.python.max_width as usize;

        // Tier 1: fits on the current line.
        let tier1_width = current_col + open.len() + inline.chars().count() + close.len();
        if !inline.contains('\n') && tier1_width <= max_width {
            self.raw(&inline);
            self.raw(close);
            return;
        }

        // Tier 2: fits hugged onto its own indented line, unsplit.
        let body_indent = self.indent_str_at(self.depth + 1).chars().count();
        let tier2_width = body_indent + inline.chars().count();
        if !inline.contains('\n') && (groups.len() == 1 || tier2_width <= max_width) {
            self.nl();
            self.depth += 1;
            self.emit_indent();
            self.raw(&inline);
            self.nl();
            self.depth -= 1;
            self.emit_indent();
            self.raw(close);
            return;
        }

        // Tier 3: one item per line, trailing comma.
        self.nl();
        self.depth += 1;
        for group in &groups {
            self.emit_indent();
            self.emit_group(group, node);
            self.raw(",");
            self.nl();
        }
        self.depth -= 1;
        self.emit_indent();
        self.raw(close);
    }
}

#[cfg(test)]
mod tests {
    use super::format;
    use crate::config::Config;

    fn fmt(src: &str) -> String {
        format(src, &Config::default()).unwrap_or_else(|e| panic!("format failed: {e}"))
    }

    fn fmt_black(src: &str) -> String {
        let mut config = Config::default();
        config.python.max_width = 88;
        format(src, &config).unwrap_or_else(|e| panic!("format failed: {e}"))
    }

    #[test]
    fn power_operator_hugs_simple_operands() {
        assert_eq!(fmt("x = a**2\n"), "x = a**2\n");
        assert_eq!(fmt("x = a ** 2\n"), "x = a**2\n");
        assert_eq!(fmt("x = 2 ** -1\n"), "x = 2**-1\n");
        assert_eq!(fmt("x = self.a ** 2\n"), "x = self.a**2\n");
    }

    #[test]
    fn power_operator_keeps_spaces_for_complex_operands() {
        assert_eq!(fmt("x = f(y) ** 2\n"), "x = f(y) ** 2\n");
        assert_eq!(fmt("x = (a + b) ** 2\n"), "x = (a + b) ** 2\n");
    }

    #[test]
    fn semicolon_separated_statements_split_onto_separate_lines() {
        assert_eq!(fmt("a = 1; b = 2\n"), "a = 1\nb = 2\n");
    }

    #[test]
    fn trailing_comment_stays_on_its_source_line() {
        assert_eq!(fmt("x = 1  # note\n"), "x = 1  # note\n");
    }

    #[test]
    fn pep8_forces_two_blank_lines_around_top_level_defs() {
        let src = "import os\ndef f():\n    pass\nx = 1\n";
        let out = fmt(src);
        assert_eq!(out, "import os\n\n\ndef f():\n    pass\n\n\nx = 1\n");
    }

    #[test]
    fn pep8_forces_one_blank_line_around_nested_defs() {
        let src = "class C:\n    x = 1\n    def m(self):\n        pass\n    y = 2\n";
        let out = fmt(src);
        assert_eq!(
            out,
            "class C:\n    x = 1\n\n    def m(self):\n        pass\n\n    y = 2\n"
        );
    }

    #[test]
    fn blank_line_forced_after_class_docstring() {
        let src = "class C:\n    \"\"\"doc\"\"\"\n    x = 1\n";
        assert_eq!(fmt(src), "class C:\n    \"\"\"doc\"\"\"\n\n    x = 1\n");
    }

    #[test]
    fn no_forced_blank_line_after_function_docstring() {
        let src = "def f():\n    \"\"\"doc\"\"\"\n    return 1\n";
        assert_eq!(fmt(src), "def f():\n    \"\"\"doc\"\"\"\n    return 1\n");
    }

    #[test]
    fn bracket_list_tier1_stays_single_line() {
        assert_eq!(fmt("foo(1, 2, 3)\n"), "foo(1, 2, 3)\n");
    }

    #[test]
    fn bracket_list_tier2_hugs_when_body_fits_alone() {
        let src = "long_signature(argument_one, argument_two, argument_three, argument_four, argument_five)\n";
        let out = fmt(src);
        assert_eq!(
            out,
            "long_signature(\n    argument_one, argument_two, argument_three, argument_four, argument_five\n)\n"
        );
    }

    #[test]
    fn bracket_list_tier3_explodes_when_body_alone_still_overflows() {
        let src = "f(argument_one=1, argument_two=2, argument_three=3, argument_four=4, argument_five=5)\n";
        let out = fmt(src);
        assert_eq!(
            out,
            "f(\n    argument_one=1,\n    argument_two=2,\n    argument_three=3,\n    argument_four=4,\n    argument_five=5,\n)\n"
        );
        // At the wider Black-compatible width, the same call fits on one line.
        assert_eq!(
            fmt_black(src),
            "f(argument_one=1, argument_two=2, argument_three=3, argument_four=4, argument_five=5)\n"
        );
    }

    #[test]
    fn idempotent_on_already_formatted_source() {
        let src = "def f(x):\n    return x**2\n";
        let pass1 = fmt(src);
        let pass2 = fmt(&pass1);
        assert_eq!(pass1, pass2);
    }
}
