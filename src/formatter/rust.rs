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

use super::output::OutputOps;
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
    /// A new line one level deeper, continuing the expression around it.
    Continuation,
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

/// tree-sitter-rust reads a prefix inclusive range `..=x` as `..` assigned
/// `x`; spacing that `=` out would change the code.
fn is_misread_prefix_range(node: Node) -> bool {
    let left = node.child_by_field_name("left");
    let op = left.and_then(|l| l.next_sibling());
    left.zip(op).is_some_and(|(l, op)| {
        l.kind() == "range_expression"
            && l.child_count() == 1
            && op.kind() == "="
            && l.end_byte() == op.start_byte()
    })
}

/// tree-sitter-rust reads the trailing comma of closure parameters
/// (`|a, b,| a + b`) as starting an or-pattern `| a + b ...`, which runs on
/// into the code that follows. The statement holding it is kept as written.
/// Nested blocks are statements of their own and are checked separately.
fn has_misread_closure_comma(node: Node) -> bool {
    if node.kind() == "or_pattern"
        && node.child(0).is_some_and(|c| c.kind() == "|")
        && node.prev_sibling().is_some_and(|p| p.kind() == ",")
        && node
            .parent()
            .is_some_and(|p| matches!(p.kind(), "closure_parameters" | "ERROR"))
    {
        return true;
    }
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|child| {
        !matches!(child.kind(), "block" | "declaration_list" | "match_block")
            && has_misread_closure_comma(child)
    });
    found
}

/// Whether `node` holds a syntax error that its emitter would format over:
/// an `ERROR` or a token the parser invented, outside the blocks whose
/// items are checked one by one. Such an item is kept as written, since
/// formatting it could invent, drop or join tokens.
fn damaged(node: Node) -> bool {
    if !node.has_error() {
        return false;
    }
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    let mut children = node.children(&mut cursor);
    if matches!(node.kind(), "block" | "declaration_list") {
        return children.any(|c| c.is_missing());
    }
    children.any(damaged)
}

/// Whether an item in a sequence can be laid out on its own. Where the
/// grammar could not read a whole file, its top level is an `ERROR` node
/// whose children mix complete items with loose tokens; only the complete
/// ones are laid out.
fn is_whole_item(node: Node, in_error: bool) -> bool {
    if has_misread_closure_comma(node) || damaged(node) {
        return false;
    }
    let kind = node.kind();
    !in_error
        || (node.is_named()
            && (kind.ends_with("_item")
                || kind.ends_with("_declaration")
                || kind.ends_with("_statement")
                || matches!(kind, "line_comment" | "block_comment")))
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

    // ── Item / statement sequencing (shared by source_file, blocks, and
    //    impl/trait/mod bodies — all are just "a sequence of things, each on
    //    its own line, blank lines preserved from source"). ────────────────

    fn emit_item_sequence(&mut self, items: &[Node]) {
        let in_error = items
            .first()
            .and_then(|n| n.parent())
            .is_some_and(|p| p.kind() == "ERROR");
        let mut prev_end: Option<usize> = None;
        let mut i = 0;
        while i < items.len() {
            let item = items[i];
            if let Some(pe) = prev_end {
                let blanks = self.source_blanks(pe, item.start_byte());
                self.emit_blank_lines(blanks);
            }
            self.ensure_nl();
            self.emit_indent();
            if is_whole_item(item, in_error) {
                self.emit_node(item);
                prev_end = Some(item.end_byte());
                i += 1;
            } else {
                // Keep the item, and in an unread file the run of loose
                // tokens around it, as written.
                let mut j = i + 1;
                while in_error && j < items.len() && !is_whole_item(items[j], in_error) {
                    j += 1;
                }
                let end = items[j - 1].end_byte();
                self.raw(self.src[item.start_byte()..end].trim_end_matches('\n'));
                prev_end = Some(end);
                i = j;
            }
            self.nl();
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
                // A comment after code on its line in the source describes
                // that code: it stays on the line the code ended, where the
                // caller started a new line for it.
                let start = node.start_byte();
                let line_start = self.src[..start].rfind('\n').map_or(0, |at| at + 1);
                let out_line = self.out.rfind('\n').map_or(0, |at| at + 1);
                if !self.src[line_start..start].trim().is_empty()
                    && out_line > 0
                    && self.out[out_line..].trim().is_empty()
                    && !self.out[..out_line - 1].ends_with('\n')
                {
                    self.out.truncate(out_line - 1);
                    self.raw(" ");
                }
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
            // What the grammar could not read, and `..=x` read as an
            // assignment to `..`, are kept as written.
            "ERROR" => self.raw(self.node_text(node)),
            "assignment_expression" if is_misread_prefix_range(node) => {
                self.raw(self.node_text(node));
            }
            "block" | "declaration_list" => self.emit_brace_block(node),
            "field_declaration_list" | "enum_variant_list" => self.emit_multiline_field_list(node),
            "match_block" => self.emit_brace_block(node),
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
                Ws::Space => self.space(),
                Ws::Newline => {
                    self.ensure_nl();
                    self.emit_indent();
                }
                Ws::Continuation => {
                    self.ensure_nl();
                    self.depth += 1;
                    self.emit_indent();
                    self.depth -= 1;
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
        // A line comment runs to the end of its line, so what follows it
        // starts a new one. One that had a line of its own keeps it.
        if pk == "line_comment"
            || (ck == "line_comment" && cur.start_position().row > prev.end_position().row)
        {
            return Ws::Continuation;
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

    /// `{ ... }` bodies with no other syntax of their own: blocks,
    /// impl/trait/mod bodies (`declaration_list`), and match arm lists
    /// (`match_block`).
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
        // A line comment runs to the end of its line, so a list holding
        // one can only be laid out one item per line.
        let has_comment = groups.iter().flatten().any(|n| n.kind() == "line_comment");
        // `(a,)` is a one-element tuple; without its comma, `(a)` is `a`.
        // The grammar reads a parenthesized type or pattern as a tuple too,
        // so the comma is kept where the source has one.
        let one_tuple = groups.len() == 1
            && matches!(
                node.kind(),
                "tuple_expression" | "tuple_type" | "tuple_pattern"
            )
            && children.iter().any(|n| n.kind() == ",");
        let render_inline = |s: &mut Self| {
            s.render_scratch(|s| {
                if pad {
                    s.raw(" ");
                }
                for (i, group) in groups.iter().enumerate() {
                    if i > 0 {
                        s.raw(", ");
                    }
                    s.emit_group(group, node);
                }
                if one_tuple {
                    s.raw(",");
                }
                if pad {
                    s.raw(" ");
                }
            })
        };
        let needs_break = if has_comment {
            true
        } else if groups.len() <= 1 {
            false
        } else if self.config.rust.width_based_wrapping {
            let current_col = self.current_column();
            let inline = render_inline(self);
            let total_width = current_col + open.len() + inline.chars().count() + close.len();
            inline.contains('\n') || total_width > self.config.rust.max_width as usize
        } else {
            // A list that renders over several lines is split now, as the
            // next run would split it for spanning lines.
            self.src[node.start_byte()..node.end_byte()].contains('\n')
                || render_inline(self).contains('\n')
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
            if one_tuple {
                self.raw(",");
            }
            if pad {
                self.raw(" ");
            }
            self.raw(close);
        } else {
            // Comments take no comma: one that started on the previous
            // item's line stays there, any other gets its own line.
            self.nl();
            self.depth += 1;
            let is_comment = |n: &Node| matches!(n.kind(), "line_comment" | "block_comment");
            let mut last_row = node.start_position().row;
            for (gi, group) in groups.iter().enumerate() {
                let first = group.iter().position(|n| !is_comment(n));
                let last = group.iter().rposition(|n| !is_comment(n));
                let (lead, core, trail) = match (first, last) {
                    (Some(f), Some(l)) => (&group[..f], &group[f..=l], &group[l + 1..]),
                    _ => (&group[..], &group[..0], &group[..0]),
                };
                for &comment in lead {
                    last_row = self.emit_list_comment(comment, last_row);
                }
                if let Some(end) = core.last() {
                    self.emit_indent();
                    self.emit_group(core, node);
                    // `..base` takes no comma, and neither does a closure's
                    // last parameter: tree-sitter-rust misreads `a, |`.
                    let no_comma = end.kind() == "base_field_initializer"
                        || (node.kind() == "closure_parameters" && gi + 1 == groups.len());
                    if !no_comma {
                        self.raw(",");
                    }
                    self.nl();
                    last_row = end.end_position().row;
                }
                for &comment in trail {
                    last_row = self.emit_list_comment(comment, last_row);
                }
            }
            self.depth -= 1;
            self.emit_indent();
            self.raw(close);
        }
    }

    /// Emits a comment in a one-item-per-line list, after the item it
    /// trailed in the source or on its own line, and returns its row.
    fn emit_list_comment(&mut self, comment: Node, last_row: usize) -> usize {
        if comment.start_position().row == last_row && self.out.ends_with(",\n") {
            self.out.pop();
            self.raw(" ");
        } else {
            self.emit_indent();
        }
        self.emit_node(comment);
        self.nl();
        comment.end_position().row
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
    fn an_item_with_a_syntax_error_is_kept_as_written() {
        let src = "fn f(a: u32 {\n    a\n}\nfn g()  ->  u32 { 1 }\n";
        let out = fmt_with(&Config::default(), src);
        assert_eq!(out, "fn f(a: u32 {\n    a\n}\nfn g() -> u32 {\n    1\n}\n");
    }

    #[test]
    fn a_trailing_comment_stays_on_its_line() {
        let src = "enum E {\n    A = 1, // one\n    B,     // two\n}\n\n#[test] // why\nfn f() -> u32 {\n    g(); // call\n    1 // one\n}\n";
        let out = fmt_with(&Config::default(), src);
        assert_eq!(
            out,
            "enum E {\n    A = 1, // one\n    B, // two\n}\n\n#[test] // why\nfn f() -> u32 {\n    g(); // call\n    1 // one\n}\n"
        );
    }

    #[test]
    fn a_one_element_tuple_keeps_its_comma() {
        let src = "fn f() {\n    let t: (u8,) = (1,);\n}\n";
        assert_eq!(fmt_with(&Config::default(), src), src);
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
