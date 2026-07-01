// C and C++ formatter: CST recursive-descent with leaf-walker for expressions.
//
// Goal: produce output identical to funky for all C/C++ files.
// Reference: ../funky/src/formatter.rs  (config is intentionally identical)

use crate::config::{Config, IndentStyle, PointerAlign, SpaceOption};
use crate::error::MoldyError;
use tree_sitter::Node;

// ── Public entry point ────────────────────────────────────────────────────────

pub fn format(source: &str, lang_key: &str, config: &Config) -> Result<String, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key(lang_key)
        .ok_or_else(|| MoldyError::UnsupportedLanguage(lang_key.to_string()))?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))?;

    // Continue even if tree has errors; unknown nodes emit their source text.
    let mut fmt = Fmt::new(source, config);
    fmt.emit_translation_unit(tree.root_node());
    let output = fmt.finish();

    let output = if config.spacing.align_right_cmt_span > 0 {
        let normalize_single =
            config.spacing.align_right_cmt_style == crate::config::AlignCmtStyle::All;
        align_trailing_comments(
            &output,
            config.spacing.align_right_cmt_gap.max(1),
            normalize_single,
            config.spacing.align_on_tabstop,
            config.indent.width as usize,
            config.spacing.align_right_cmt_span,
        )
    } else {
        output
    };

    let output = if config.spacing.align_enum_equ_span > 0 {
        align_enum_equals(
            &output,
            config.spacing.align_on_tabstop,
            config.indent.width as usize,
        )
    } else {
        output
    };

    Ok(output)
}

// ── Formatter state ───────────────────────────────────────────────────────────

struct Fmt<'a> {
    src: &'a str,
    config: &'a Config,
    out: String,
    depth: u32,
    at_bol: bool,
    // blank_line_after_var_decl_block state
    decl_block_active: bool,
    decl_block_saw_decl: bool,
    decl_block_at_stmt_start: bool,
    // When a column-aligned lambda was just emitted, this holds the column so
    // that the closing `)` and `;` tokens can be placed on their own lines.
    last_lambda_col: Option<usize>,
    // Column after `= ` in an init-declarator, so that a compound-literal `{`
    // appearing on the next line in emit_leaves can align to it.
    assign_col_for_brace: Option<usize>,
}

impl<'a> Fmt<'a> {
    fn new(src: &'a str, config: &'a Config) -> Self {
        Fmt {
            src,
            config,
            out: String::with_capacity(src.len()),
            depth: 0,
            at_bol: true,
            decl_block_active: false,
            decl_block_saw_decl: false,
            decl_block_at_stmt_start: false,
            last_lambda_col: None,
            assign_col_for_brace: None,
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

    // ── Output primitives ─────────────────────────────────────────────────────

    fn indent_str_at(&self, d: u32) -> String {
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
        self.at_bol = s.ends_with('\n');
    }

    fn nl(&mut self) {
        self.out.push('\n');
        self.at_bol = true;
    }

    fn ensure_nl(&mut self) {
        if !self.at_bol {
            self.nl();
        }
    }

    fn emit_indent(&mut self) {
        let s = self.indent_str_at(self.depth);
        self.raw(&s);
    }

    fn emit_indent_at(&mut self, d: u32) {
        let s = self.indent_str_at(d);
        self.raw(&s);
    }

    fn space(&mut self) {
        if !self.at_bol && !self.out.ends_with(' ') && !self.out.ends_with('\n') {
            self.out.push(' ');
        }
    }

    /// Current column (0-indexed) in the output buffer.
    fn current_col(&self) -> usize {
        match self.out.rfind('\n') {
            Some(nl_pos) => self.out.len() - nl_pos - 1,
            None => self.out.len(),
        }
    }

    #[allow(dead_code)]
    fn blank_line(&mut self) {
        // Ensure exactly one blank line (two consecutive newlines) is present.
        self.ensure_nl();
        // Count trailing newlines.
        let trailing = self.out.chars().rev().take_while(|&c| c == '\n').count();
        for _ in 0..2usize.saturating_sub(trailing) {
            self.nl();
        }
    }

    // ── Source text ───────────────────────────────────────────────────────────

    fn node_text<'t>(&self, node: Node<'t>) -> &'a str {
        &self.src[node.start_byte()..node.end_byte()]
    }

    /// Count blank lines in source between `prev_end` and `start`.
    fn source_blanks(&self, prev_end: usize, start: usize) -> usize {
        if start <= prev_end {
            return 0;
        }
        let gap = &self.src[prev_end..start];
        let newlines = gap.chars().filter(|&c| c == '\n').count();
        // If the char just before prev_end is already '\n' (e.g. preproc includes
        // its trailing newline in the node), each '\n' in the gap is a blank line.
        // Otherwise the first '\n' merely ends the previous line — subtract one.
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

    // ── Translation unit ──────────────────────────────────────────────────────

    fn emit_translation_unit(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let mut prev_end: usize = 0;
        let mut i = 0;

        while i < children.len() {
            let child = children[i];
            let blanks = self.source_blanks(prev_end, child.start_byte());
            let kind = child.kind();

            // Check if the next sibling is a same-line trailing comment.
            let trailing_comment: Option<Node> = if kind != "comment" && i + 1 < children.len() {
                let next = children[i + 1];
                if next.kind() == "comment" {
                    let stmt_end_line = self.src[..child.end_byte()].lines().count();
                    let cmt_line = self.src[..next.start_byte()].lines().count();
                    if stmt_end_line == cmt_line {
                        Some(next)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Don't force newline before `;` — it may follow a type body `}` on same line.
            if kind != ";" {
                self.ensure_nl();
                self.emit_blank_lines(blanks);
            }

            match kind {
                k if is_preproc(k) => {
                    self.emit_preproc(child);
                }
                "comment" => {
                    self.emit_comment_standalone(child);
                }
                "function_definition" => {
                    self.emit_indent();
                    self.emit_function_definition(child);
                    self.ensure_nl();
                }
                "declaration" | "type_definition" => {
                    self.emit_toplevel_decl(child);
                    self.ensure_nl();
                }
                "enum_specifier" | "struct_specifier" | "union_specifier" | "class_specifier" => {
                    // At file scope, these are type declarations (not wrapped in `declaration`).
                    // The `;` is a separate sibling that follows.
                    self.emit_indent();
                    self.emit_expr_node(child);
                    // Don't call ensure_nl() — the sibling `;` will follow.
                }
                "linkage_specification" => {
                    self.emit_linkage_specification(child);
                }
                "namespace_definition" => {
                    self.emit_indent();
                    self.emit_namespace_definition(child);
                }
                "template_declaration" => {
                    self.emit_indent();
                    self.emit_template_declaration(child);
                }
                ";" => {
                    self.raw(";");
                    // If the `;` has a trailing same-line comment, emit it inline.
                    if let Some(cmt) = trailing_comment {
                        self.space();
                        self.raw(self.node_text(cmt).trim_end_matches('\n'));
                        prev_end = cmt.end_byte();
                        i += 2;
                        self.nl();
                        continue;
                    }
                    self.nl();
                }
                "expression_statement" if is_null_stmt(child) => {
                    // Null stmt at top level: always column 0 (funky invariant).
                    self.raw(";");
                    if let Some(cmt) = trailing_comment {
                        self.space();
                        self.raw(self.node_text(cmt).trim_end_matches('\n'));
                        prev_end = cmt.end_byte();
                        i += 2;
                        self.nl();
                        continue;
                    }
                    self.nl();
                }
                _ => {
                    self.emit_indent();
                    self.emit_statement(child);
                    self.ensure_nl();
                }
            }

            // For non-`;` nodes with a trailing comment, emit the comment inline
            // (after the node's own newline was suppressed by ensure_nl).
            if kind != ";" {
                if let Some(cmt) = trailing_comment {
                    // The statement was just emitted; strip the trailing \n and add comment.
                    if self.out.ends_with('\n') {
                        self.out.pop();
                        self.at_bol = false;
                    }
                    self.space();
                    self.raw(self.node_text(cmt).trim_end_matches('\n'));
                    self.nl();
                    prev_end = cmt.end_byte();
                    i += 2;
                    continue;
                }
            }

            prev_end = child.end_byte();
            i += 1;
        }
    }

    // ── Preprocessor ──────────────────────────────────────────────────────────

    fn emit_preproc(&mut self, node: Node) {
        let text = self.node_text(node).trim_end_matches('\n');
        self.raw(text);
        self.nl();
    }

    fn emit_comment_standalone(&mut self, node: Node) {
        self.emit_indent();
        let text = self.node_text(node).trim_end_matches('\n');
        self.raw(text);
        self.nl();
    }

    // ── Function definition ───────────────────────────────────────────────────

    fn emit_function_definition(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // `= delete;` / `= default;` — emit signature (which includes the clause)
        // and bail; no brace body follows.
        let has_specifier_clause = children
            .iter()
            .any(|n| matches!(n.kind(), "delete_method_clause" | "default_method_clause"));
        if has_specifier_clause {
            let sig_nodes: Vec<Node> = children
                .iter()
                .copied()
                .filter(|n| !matches!(n.kind(), "delete_method_clause" | "default_method_clause"))
                .collect();
            self.emit_signature(&sig_nodes);
            // Emit the clause itself (= delete; / = default;) directly.
            for child in &children {
                if matches!(
                    child.kind(),
                    "delete_method_clause" | "default_method_clause"
                ) {
                    let mut csr = child.walk();
                    // ` = delete;` / ` = default;`
                    self.space();
                    self.raw("=");
                    self.space();
                    for tok in child.children(&mut csr) {
                        // Skip `=`; emit only `delete`/`default` and `;`
                        let t = self.node_text(tok);
                        if t == "=" {
                            continue;
                        }
                        if t == ";" {
                            self.raw(";");
                        } else {
                            self.raw(t);
                        }
                    }
                    break;
                }
            }
            self.nl();
            return;
        }

        let body_idx = children
            .iter()
            .position(|n| n.kind() == "compound_statement");

        let sig_nodes = &children[..body_idx.unwrap_or(children.len())];

        // Caller is responsible for indentation.
        self.emit_signature(sig_nodes);

        // funky's infer_brace_ctx() classifies the brace after a plain `)` as
        // BraceCtx::Function, so fn_brace_newline always applies. When the
        // signature ends with a trailing qualifier keyword (const, override,
        // noexcept…) the brace falls under BraceCtx::Other instead, which
        // preserves whatever placement the source already used rather than
        // forcing one.
        let sig_ends_with_rparen = sig_nodes
            .last()
            .map(|&n| {
                let mut node = n;
                while node.child_count() > 0 {
                    node = node.child(node.child_count() - 1).unwrap();
                }
                node.kind() == ")"
            })
            .unwrap_or(false);
        let fn_brace_newline = if sig_ends_with_rparen {
            self.config.braces.fn_brace_newline
        } else {
            body_idx
                .map(|idx| {
                    let body = children[idx];
                    let last_sig_end = sig_nodes
                        .last()
                        .map(|n| n.end_byte())
                        .unwrap_or(node.start_byte());
                    self.src[last_sig_end..body.start_byte()].contains('\n')
                })
                .unwrap_or(false)
        };

        // Check if body is empty (compound_statement with no real children).
        let body_is_empty = body_idx
            .map(|idx| {
                let body = children[idx];
                let mut c = body.walk();
                body.children(&mut c)
                    .filter(|n| !matches!(n.kind(), "{" | "}"))
                    .count()
                    == 0
            })
            .unwrap_or(false);

        // funky's BraceCtx::Other (trailing-qualifier signatures whose source
        // already had the brace on its own line) emits `{` flush at column 0,
        // unlike BraceCtx::Function which indents to the current depth.
        let brace_at_col0 = fn_brace_newline && !sig_ends_with_rparen;

        if body_is_empty && self.config.braces.collapse_empty_body {
            if fn_brace_newline {
                self.ensure_nl();
                if !brace_at_col0 {
                    self.emit_indent();
                }
            } else {
                self.space();
            }
            self.raw("{}");
            self.nl();
            return;
        }

        if fn_brace_newline {
            self.ensure_nl();
            if !brace_at_col0 {
                self.emit_indent();
            }
        } else {
            self.space();
        }
        self.raw("{");

        if let Some(idx) = body_idx {
            let body = children[idx];
            // When the signature keeps its brace inline (fn_brace_newline=false),
            // check if the body fits on a single source line.  If so, emit it
            // inline (matching funky's behaviour for one-liner methods).
            let inline = !fn_brace_newline && body.start_position().row == body.end_position().row;
            if inline {
                self.emit_compound_body_inline(body);
            } else {
                self.nl();
                self.depth += 1;
                self.decl_block_enter();
                self.emit_compound_body(body);
                self.decl_block_exit();
                self.depth -= 1;
                self.ensure_nl();
                self.emit_indent();
                self.raw("}");
            }
        } else {
            self.ensure_nl();
            self.emit_indent();
            self.raw("}");
        }
        self.nl();
    }

    /// Emit a compound_statement body on the current line (no newlines injected).
    /// Used for one-liner inline method bodies: `bool f() const { return x; }`.
    /// Replicates funky's `render_inline_init` spacing: space before `;` and `}`.
    fn emit_compound_body_inline(&mut self, node: Node) {
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
        for child in &children[start_i..end_i] {
            self.space();
            self.emit_statement(*child);
            // funky's render_inline_init adds space before `;` (it's not in the
            // "no space before" exceptions for inline renders).
            if self.out.ends_with(';') {
                let pos = self.out.len() - 1;
                self.out.insert(pos, ' ');
            }
        }
        self.space();
        self.raw("}");
    }

    fn emit_signature(&mut self, nodes: &[Node<'_>]) {
        let mut prev: Option<Node> = None;
        for &child in nodes {
            let k = child.kind();
            match k {
                k if is_preproc(k) => {
                    self.ensure_nl();
                    self.emit_preproc(child);
                    self.emit_indent();
                    prev = None;
                }
                "comment" => {
                    self.space();
                    self.raw(self.node_text(child).trim_end_matches('\n'));
                    prev = Some(child);
                }
                "attribute_specifier"
                | "attribute_declaration"
                | "ms_declspec_modifier"
                | "__attribute__" => {
                    if prev.is_some() {
                        self.space();
                    }
                    self.emit_expr_node(child);
                    prev = Some(child);
                }
                _ => {
                    // field_initializer_list starts with `:` — no space before it.
                    // function_declarator with paren-first-child that isn't a simple
                    // fn-ptr (e.g. `int(fp)()`, `int(*f(params))(...)`) gets no space.
                    let skip_space = k == "field_initializer_list"
                        || (k == "function_declarator"
                            && fn_declarator_has_paren_first_child(child)
                            && !is_simple_fn_ptr_declarator(child));
                    if prev.is_some() && !self.at_bol && !skip_space {
                        // Preserve source newline between signature tokens (e.g. `int\nmain40()`).
                        let source_has_nl = prev
                            .map(|p| self.src[p.end_byte()..child.start_byte()].contains('\n'))
                            .unwrap_or(false);
                        if source_has_nl {
                            self.nl();
                            self.emit_indent();
                        } else {
                            self.space();
                        }
                    }
                    self.emit_expr_node(child);
                    prev = Some(child);
                }
            }
        }
    }

    // ── Compound body ─────────────────────────────────────────────────────────

    fn emit_compound_body(&mut self, node: Node) {
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

        let mut prev_end = if start_i > 0 {
            children[start_i - 1].end_byte()
        } else {
            node.start_byte()
        };

        let mut i = start_i;
        let mut first_stmt = true;
        while i < end_i {
            let child = children[i];
            let kind = child.kind();
            let blanks = self.source_blanks(prev_end, child.start_byte());

            // Check if the next sibling is a same-line trailing comment.
            let trailing_comment: Option<Node> = if kind != "comment" && i + 1 < end_i {
                let next = children[i + 1];
                if next.kind() == "comment" {
                    let stmt_end_line = self.src[..child.end_byte()].lines().count();
                    let cmt_line = self.src[..next.start_byte()].lines().count();
                    if stmt_end_line == cmt_line {
                        Some(next)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Braceless switch: `switch (cond)` without braces produces an ERROR
            // node in tree-sitter.  The following case_statement sibling is the
            // switch body.  To match funky's depth accumulation (indent_switch_case
            // permanently increments depth for each braceless case), we emit the
            // switch header, then the case statement, then permanently bump depth.
            if kind == "ERROR"
                && is_braceless_switch_error(child)
                && i + 1 < end_i
                && children[i + 1].kind() == "case_statement"
            {
                // Emit switch header.
                self.emit_blank_lines(blanks);
                self.ensure_nl();
                self.emit_indent();
                self.emit_braceless_switch_header(child);
                prev_end = child.end_byte();
                i += 1;

                // Emit the following case_statement.
                let case_node = children[i];
                let case_blanks = self.source_blanks(prev_end, case_node.start_byte());
                self.emit_blank_lines(case_blanks);
                self.ensure_nl();
                self.emit_indent();
                self.emit_case_statement(case_node);
                // Permanently raise depth to mirror funky's indent_switch_case
                // behaviour: the braceless case body stays open until something
                // closes it (which, for braceless switches, is nothing inside this
                // compound body — so depth leaks out, matching funky).
                if self.config.indent.indent_switch_case {
                    self.depth += 1;
                }
                prev_end = case_node.end_byte();
                i += 1;
                continue;
            }

            match kind {
                k if is_preproc(k) => {
                    self.emit_blank_lines(blanks);
                    self.ensure_nl();
                    self.emit_preproc(child);
                }
                "comment" => {
                    self.emit_blank_lines(blanks);
                    self.ensure_nl();
                    self.emit_indent();
                    let text = self.node_text(child).trim_end_matches('\n');
                    self.raw(text);
                    self.nl();
                }
                _ => {
                    // Apply blank_line_after_var_decl_block.
                    let mut force_blank = false;
                    if self.decl_block_active && self.decl_block_at_stmt_start {
                        self.decl_block_at_stmt_start = false;
                        let is_decl = is_declaration_node(child);
                        if is_decl {
                            self.decl_block_saw_decl = true;
                        } else if self.decl_block_saw_decl {
                            self.decl_block_active = false;
                            force_blank = true;
                        } else {
                            self.decl_block_active = false;
                        }
                    }

                    let emit_blanks = if force_blank { blanks.max(1) } else { blanks };

                    if is_null_stmt(child) && self.out.ends_with('}') {
                        // Null stmt right after `}`: emit `;` inline (funky: `};`).
                    } else {
                        self.emit_blank_lines(emit_blanks);
                        if !is_null_stmt(child) {
                            self.ensure_nl();
                            // Standalone `{ }` block as FIRST child of outer `{ }`:
                            // funky's BraceCtx::Other (prev=LBrace) writes `{` at col 0.
                            // Only suppress indent when this block is the first content.
                            let is_first_nested_block =
                                first_stmt && child.kind() == "compound_statement";
                            if !is_first_nested_block {
                                self.emit_indent();
                            }
                        }
                        // else: null stmt at BOL → column 0, no indent.
                    }
                    let is_first_block = first_stmt && child.kind() == "compound_statement";
                    if is_first_block {
                        self.emit_brace_ctx_other_compound(child);
                    } else {
                        self.emit_statement(child);
                    }

                    // After emitting, prepare for next statement in decl block.
                    if self.decl_block_active {
                        self.decl_block_at_stmt_start = true;
                    }

                    // Emit a same-line trailing comment inline, then skip it.
                    // Some statement kinds (e.g. namespace_definition) end by
                    // unconditionally emitting their own newline; strip it so
                    // the comment lands on the same line.
                    if let Some(cmt) = trailing_comment {
                        if self.out.ends_with('\n') {
                            self.out.pop();
                            self.at_bol = false;
                        }
                        self.space();
                        self.raw(self.node_text(cmt).trim_end_matches('\n'));
                        self.nl();
                        prev_end = cmt.end_byte();
                        first_stmt = false;
                        i += 2;
                        continue;
                    }
                }
            }

            prev_end = child.end_byte();
            first_stmt = false;
            i += 1;
        }

        // Preserve any blank line in the source just before the closing `}`.
        if end_i < children.len() {
            let closing_brace = children[end_i];
            let trailing_blanks = self.source_blanks(prev_end, closing_brace.start_byte());
            self.emit_blank_lines(trailing_blanks);
        }
    }

    // ── Statements ────────────────────────────────────────────────────────────

    /// Emit a `compound_statement` that is the first child of another `{...}`
    /// (funky BraceCtx::Other). Key difference from normal compound_statement:
    /// if the body ends with a case_statement, the case body's depth increment
    /// leaks into the closing `}` — matching funky's indent_level behavior.
    fn emit_brace_ctx_other_compound(&mut self, node: Node) {
        self.raw("{");
        self.nl();
        self.depth += 1;
        self.emit_compound_body(node);
        // Funky quirk: case body inside BraceCtx::Other leaks into `}`.
        if !compound_ends_with_case(node) {
            self.depth -= 1;
        }
        self.ensure_nl();
        self.emit_indent();
        // Always restore depth for callers.
        self.depth = self.depth.saturating_sub(1);
        self.raw("}");
    }

    fn emit_statement(&mut self, node: Node) {
        match node.kind() {
            "compound_statement" => {
                self.raw("{");
                self.nl();
                self.depth += 1;
                self.emit_compound_body(node);
                self.depth -= 1;
                self.ensure_nl();
                self.emit_indent();
                self.raw("}");
            }
            "if_statement" => self.emit_if_statement(node),
            "for_statement" => self.emit_for_statement(node),
            "for_range_loop" => self.emit_for_range_loop(node),
            "while_statement" => self.emit_while_statement(node),
            "do_statement" => self.emit_do_statement(node),
            "switch_statement" => self.emit_switch_statement(node),
            "case_statement" => self.emit_case_statement(node),
            "return_statement" => self.emit_return_statement(node),
            "break_statement" => {
                self.raw("break");
                self.raw(";");
            }
            "continue_statement" => {
                self.raw("continue");
                self.raw(";");
            }
            "goto_statement" => self.emit_goto_statement(node),
            "labeled_statement" => self.emit_labeled_statement(node),
            "expression_statement" => self.emit_expression_statement(node),
            "declaration" => self.emit_decl_node(node),
            "type_definition" => self.emit_typedef_node(node),
            "function_definition" => self.emit_function_definition(node),
            "template_declaration" => {
                self.emit_template_declaration(node);
            }
            "namespace_definition" => {
                self.emit_namespace_definition(node);
            }
            "class_specifier" => {
                self.emit_class_like(node);
                self.raw(";");
            }
            "struct_specifier" => {
                self.emit_struct_like(node);
                self.raw(";");
            }
            "try_statement" => self.emit_try_statement(node),
            k if is_preproc(k) => self.emit_preproc(node),
            "comment" => {
                let text = self.node_text(node).trim_end_matches('\n');
                self.raw(text);
            }
            _ => {
                self.emit_expr_node(node);
            }
        }
    }

    fn emit_return_statement(&mut self, node: Node) {
        self.raw("return");
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "return" => {}
                ";" => {
                    self.raw(";");
                }
                _ => {
                    self.space();
                    self.emit_expr_node(child);
                }
            }
        }
    }

    fn emit_goto_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let mut first_content = true;
        for child in node.children(&mut cursor) {
            match child.kind() {
                "goto" => {
                    self.raw("goto");
                    first_content = false;
                }
                ";" => {
                    self.raw(";");
                }
                _ => {
                    if !first_content {
                        self.space();
                    }
                    self.raw(self.node_text(child));
                    first_content = false;
                }
            }
        }
    }

    fn emit_labeled_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // When indent_goto_labels = false, labels are placed at column 0.
        // When true, labels are indented at the current depth.
        let label_depth = if self.config.indent.indent_goto_labels {
            self.depth
        } else {
            0
        };

        // Strip current-line indentation and re-emit at label depth.
        let last_nl = self.out.rfind('\n').map(|i| i + 1).unwrap_or(0);
        if self.out[last_nl..].chars().all(|c| c == ' ' || c == '\t') {
            self.out.truncate(last_nl);
            self.at_bol = true;
        }
        self.emit_indent_at(label_depth);

        let mut i = 0;
        if i < children.len() {
            // Label name
            self.raw(self.node_text(children[i]));
            i += 1;
        }
        if i < children.len() && children[i].kind() == ":" {
            self.raw(":");
            i += 1;
        }

        // Emit all body statements after the label colon.
        // A labeled_statement may have multiple body children (e.g. a braceless
        // switch ERROR node followed by a case_statement).
        if i >= children.len() {
            self.nl();
            return;
        }

        // Process the children like a mini compound body, with braceless switch
        // detection matching emit_compound_body's logic.
        let mut prev_end = if i > 0 {
            children[i - 1].end_byte()
        } else {
            node.start_byte()
        };
        while i < children.len() {
            let child = children[i];
            let blanks = self.source_blanks(prev_end, child.start_byte());
            let kind = child.kind();

            // Braceless switch inside a labeled statement body.
            if kind == "ERROR"
                && is_braceless_switch_error(child)
                && i + 1 < children.len()
                && children[i + 1].kind() == "case_statement"
            {
                self.emit_blank_lines(blanks);
                self.ensure_nl();
                self.emit_indent();
                self.emit_braceless_switch_header(child);
                prev_end = child.end_byte();
                i += 1;

                let case_node = children[i];
                let case_blanks = self.source_blanks(prev_end, case_node.start_byte());
                self.emit_blank_lines(case_blanks);
                self.ensure_nl();
                self.emit_indent();
                self.emit_case_statement(case_node);
                if self.config.indent.indent_switch_case {
                    self.depth += 1;
                }
                prev_end = case_node.end_byte();
                i += 1;
                continue;
            }

            self.emit_blank_lines(blanks);
            self.ensure_nl();
            if !is_null_stmt(child) {
                self.emit_indent();
            }
            self.emit_statement(child);
            prev_end = child.end_byte();
            i += 1;
        }
    }

    fn emit_expression_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        if children.is_empty() {
            self.raw(";");
            return;
        }

        // Null statement?
        if children.len() == 1 && children[0].kind() == ";" {
            self.raw(";");
            return;
        }

        for child in &children {
            match child.kind() {
                ";" => self.raw(";"),
                _ => self.emit_expr_node(*child),
            }
        }
    }

    // ── If / else ─────────────────────────────────────────────────────────────

    fn emit_if_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // if
        self.raw("if");

        let mut i = 1; // skip "if"

        // condition: C uses parenthesized_expression, C++ uses condition_clause
        while i < children.len()
            && !matches!(
                children[i].kind(),
                "parenthesized_expression" | "condition_clause"
            )
        {
            i += 1;
        }
        if i < children.len() {
            let cond = children[i];
            if self.config.spacing.space_before_keyword_paren {
                self.space();
            }
            // condition_clause wraps its own parens; emit it directly.
            // parenthesized_expression also wraps parens; emit it directly.
            self.emit_expr_node(cond);
            i += 1;
        }

        // body (first child that is not else_clause)
        while i < children.len() && children[i].kind() == "else_clause" {
            i += 1;
        }

        if i < children.len() && children[i].kind() != "else_clause" {
            let body = children[i];
            let inject = self.config.braces.add_braces_to_if;
            self.emit_control_body(body, inject);
            i += 1;
        }

        // else clause
        while i < children.len() {
            let child = children[i];
            if child.kind() == "else_clause" {
                self.emit_else_clause(child);
            }
            i += 1;
        }
    }

    fn emit_else_clause(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let body_idx = children
            .iter()
            .position(|n| n.kind() != "else")
            .unwrap_or(1);
        let body = children.get(body_idx);

        if self.config.braces.cuddle_else {
            self.space();
        } else {
            self.ensure_nl();
            self.emit_indent();
        }
        self.raw("else");

        if let Some(&b) = body {
            if b.kind() == "if_statement" {
                self.space();
                self.emit_if_statement(b);
            } else {
                let inject = self.config.braces.add_braces_to_if;
                self.emit_control_body(b, inject);
            }
        }
    }

    // ── For / while / do ──────────────────────────────────────────────────────

    fn emit_for_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        self.raw("for");
        if self.config.spacing.space_before_keyword_paren {
            self.space();
        }
        self.raw("(");

        let mut i = 0;
        // Skip "for" and "("
        while i < children.len() && children[i].kind() == "for" {
            i += 1;
        }
        while i < children.len() && children[i].kind() == "(" {
            i += 1;
        }

        // Collect items until ")"
        let in_for_header = true;
        let mut body_node: Option<Node> = None;
        let mut after_paren = false;

        while i < children.len() {
            let child = children[i];
            if after_paren {
                body_node = Some(child);
                break;
            }
            match child.kind() {
                ")" => {
                    self.raw(")");
                    after_paren = true;
                }
                ";" => {
                    self.raw(";");
                    // Space after `;` in for-header only when next clause is non-empty
                    // (i.e., next is neither `)` nor another `;`).
                    if i + 1 < children.len() && !matches!(children[i + 1].kind(), ")" | ";") {
                        self.space();
                    }
                }
                _ if in_for_header => {
                    // Before emitting: if previous output ends with `;`, add space
                    // (happens when init is a declaration that includes its own `;`).
                    if self.out.ends_with(';') {
                        self.space();
                    }
                    self.emit_expr_node(child);
                }
                _ => {}
            }
            i += 1;
        }

        if let Some(body) = body_node {
            let inject = self.config.braces.add_braces_to_for;
            self.emit_control_body(body, inject);
        }
        let _ = in_for_header;
    }

    /// C++ range-based for: `for (const auto &x : xs) { ... }`. Distinct
    /// tree-sitter node kind from a plain `for_statement`.
    fn emit_for_range_loop(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        self.raw("for");
        if self.config.spacing.space_before_keyword_paren {
            self.space();
        }
        self.raw("(");

        let mut i = 0;
        while i < children.len() && children[i].kind() == "for" {
            i += 1;
        }
        while i < children.len() && children[i].kind() == "(" {
            i += 1;
        }

        let mut header_leaves: Vec<Node> = Vec::new();
        while i < children.len() && children[i].kind() != ")" {
            collect_leaves(children[i], &mut header_leaves);
            i += 1;
        }
        self.emit_leaves(&header_leaves);

        let mut body_node = None;
        if i < children.len() && children[i].kind() == ")" {
            self.raw(")");
            i += 1;
            if i < children.len() {
                body_node = Some(children[i]);
            }
        }

        if let Some(body) = body_node {
            let inject = self.config.braces.add_braces_to_for;
            self.emit_control_body(body, inject);
        }
    }

    fn emit_while_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        self.raw("while");
        if self.config.spacing.space_before_keyword_paren {
            self.space();
        }

        let mut i = 0;
        while i < children.len() && children[i].kind() == "while" {
            i += 1;
        }

        if i < children.len()
            && matches!(
                children[i].kind(),
                "parenthesized_expression" | "condition_clause"
            )
        {
            self.emit_expr_node(children[i]);
            i += 1;
        }

        if i < children.len() {
            let inject = self.config.braces.add_braces_to_while;
            self.emit_control_body(children[i], inject);
        }
    }

    fn emit_do_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        self.raw("do");

        let mut body_opt: Option<Node> = None;
        let mut cond_opt: Option<Node> = None;

        let mut i = 0;
        while i < children.len() && children[i].kind() == "do" {
            i += 1;
        }

        // body
        if i < children.len() && children[i].kind() != "while" {
            body_opt = Some(children[i]);
            i += 1;
        }
        // while
        while i < children.len() && children[i].kind() == "while" {
            i += 1;
        }
        // condition
        if i < children.len()
            && matches!(
                children[i].kind(),
                "parenthesized_expression" | "condition_clause"
            )
        {
            cond_opt = Some(children[i]);
            i += 1;
        }
        let _ = i;

        // Emit body
        if let Some(body) = body_opt {
            if body.kind() == "compound_statement" {
                self.space();
                self.raw("{");
                self.nl();
                self.depth += 1;
                self.emit_compound_body(body);
                self.depth -= 1;
                self.ensure_nl();
                self.emit_indent();
                self.raw("}");
                self.space();
            } else {
                // No brace injection for do-while bodies; funky keeps body at same depth.
                self.nl();
                self.emit_indent();
                self.emit_statement(body);
                self.ensure_nl();
                self.emit_indent();
            }
        }

        self.raw("while");
        if self.config.spacing.space_before_keyword_paren {
            self.space();
        }
        if let Some(cond) = cond_opt {
            self.emit_expr_node(cond);
        }
        self.raw(";");
    }

    // ── Switch / case ─────────────────────────────────────────────────────────

    fn emit_switch_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        self.raw("switch");
        if self.config.spacing.space_before_keyword_paren {
            self.space();
        }

        let mut i = 0;
        while i < children.len() && children[i].kind() == "switch" {
            i += 1;
        }

        if i < children.len()
            && matches!(
                children[i].kind(),
                "parenthesized_expression" | "condition_clause"
            )
        {
            self.emit_expr_node(children[i]);
            i += 1;
        }

        // Switch body is always a compound_statement; treat it like KR style (same line brace).
        if i < children.len() && children[i].kind() == "compound_statement" {
            self.space();
            self.raw("{");
            self.nl();
            self.depth += 1;
            self.emit_switch_compound_body(children[i]);
            self.depth -= 1;
            self.ensure_nl();
            self.emit_indent();
            self.raw("}");
        }
    }

    /// Emit `switch (cond)` header for a braceless switch (tree-sitter ERROR node).
    fn emit_braceless_switch_header(&mut self, node: Node) {
        self.raw("switch");
        if self.config.spacing.space_before_keyword_paren {
            self.space();
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "parenthesized_expression" {
                self.emit_expr_node(child);
                break;
            }
        }
    }

    fn emit_switch_compound_body(&mut self, node: Node) {
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

        let mut prev_end = if start_i > 0 {
            children[0].end_byte()
        } else {
            node.start_byte()
        };

        for (idx, child) in children[start_i..end_i].iter().enumerate() {
            let child = *child;
            let i = start_i + idx;
            let kind = child.kind();
            let blanks = self.source_blanks(prev_end, child.start_byte());

            self.emit_blank_lines(blanks);

            match kind {
                "case_statement" => {
                    self.ensure_nl();
                    // indent_switch_case=true: case at same level as switch body (self.depth)
                    // indent_switch_case=false: case at one level above body (self.depth-1)
                    if self.config.indent.indent_switch_case {
                        self.emit_indent();
                    } else {
                        self.emit_indent_at(self.depth.saturating_sub(1));
                    }
                    self.emit_case_statement(child);
                }
                k if is_preproc(k) => {
                    self.ensure_nl();
                    self.emit_preproc(child);
                }
                "comment" => {
                    self.ensure_nl();
                    self.emit_indent();
                    let text = self.node_text(child).trim_end_matches('\n');
                    self.raw(text);
                    self.nl();
                }
                _ => {
                    self.ensure_nl();
                    // Standalone block `{...}` as first child in switch body:
                    // funky emits `{` at column 0 (BraceCtx::Other with no indent).
                    let is_first_block = child.kind() == "compound_statement" && i == start_i;
                    if !is_first_block {
                        self.emit_indent();
                    }
                    if is_first_block {
                        self.emit_brace_ctx_other_compound(child);
                    } else {
                        self.emit_statement(child);
                    }
                }
            }
            prev_end = child.end_byte();
        }
    }

    fn emit_case_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let mut i = 0;
        match children.get(i).map(|n| n.kind()) {
            Some("case") => {
                self.raw("case");
                i += 1;
                if i < children.len() && children[i].kind() != ":" {
                    self.space();
                    self.emit_expr_node(children[i]);
                    i += 1;
                }
            }
            Some("default") => {
                self.raw("default");
                i += 1;
            }
            _ => {}
        }

        if i < children.len() && children[i].kind() == ":" {
            self.raw(":");
            i += 1;
        }

        let mut prev_end = children
            .get(i.saturating_sub(1))
            .map(|n| n.end_byte())
            .unwrap_or(node.start_byte());

        // Case body is indented one level deeper than the case label.
        self.depth += 1;
        while i < children.len() {
            let child = children[i];
            let blanks = self.source_blanks(prev_end, child.start_byte());
            self.emit_blank_lines(blanks);
            self.ensure_nl();
            // Null statements are at column 0 (funky invariant).
            if !is_null_stmt(child) {
                self.emit_indent();
            }
            self.emit_statement(child);
            prev_end = child.end_byte();
            i += 1;
        }
        self.depth -= 1;
    }

    // ── Control body (with optional brace injection) ──────────────────────────

    fn emit_control_body(&mut self, node: Node, inject: bool) {
        match node.kind() {
            "compound_statement" => {
                // Collapse empty body when configured.
                let is_empty = {
                    let mut c = node.walk();
                    node.children(&mut c)
                        .filter(|n| !matches!(n.kind(), "{" | "}"))
                        .count()
                        == 0
                };
                if is_empty && self.config.braces.collapse_empty_body {
                    self.space();
                    self.raw("{}");
                    return;
                }
                self.space();
                self.raw("{");
                self.nl();
                self.depth += 1;
                self.emit_compound_body(node);
                self.depth -= 1;
                self.ensure_nl();
                self.emit_indent();
                self.raw("}");
            }
            _ if is_null_stmt(node) => {
                // `if (x)\n;` — funky emits `;` at column 0 (no indent).
                self.nl();
                self.raw(";");
            }
            _ => {
                if inject && should_inject(node) {
                    self.space();
                    self.raw("{");
                    self.nl();
                    self.depth += 1;
                    self.emit_indent();
                    self.emit_statement(node);
                    self.depth -= 1;
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw("}");
                } else {
                    self.nl();
                    self.depth += 1;
                    self.emit_indent();
                    self.emit_statement(node);
                    self.depth -= 1;
                }
            }
        }
    }

    // ── Top-level declarations ────────────────────────────────────────────────

    fn emit_toplevel_decl(&mut self, node: Node) {
        self.emit_indent();
        match node.kind() {
            "declaration" => {
                self.emit_decl_node(node);
                self.nl();
            }
            "type_definition" => {
                self.emit_typedef_node(node);
                self.nl();
            }
            _ => {
                self.emit_expr_node(node);
                self.nl();
            }
        }
    }

    fn emit_linkage_specification(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let mut prev_end = node.start_byte();

        self.emit_indent();
        let mut i = 0;
        while i < children.len() {
            let child = children[i];
            let blanks = self.source_blanks(prev_end, child.start_byte());
            self.emit_blank_lines(blanks);

            match child.kind() {
                "extern" => {
                    self.raw("extern");
                }
                "string_literal" | "raw_string_literal" => {
                    self.space();
                    self.emit_expr_node(child);
                }
                "declaration_list" | "compound_statement" => {
                    match self.config.braces.extern_c_brace {
                        crate::config::ExternCBrace::ForceSameLine => {
                            self.space();
                        }
                        _ => {
                            self.nl();
                            self.emit_indent();
                        }
                    }
                    self.raw("{");
                    self.nl();
                    self.depth += 1;
                    self.emit_compound_body(child);
                    self.depth -= 1;
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw("}");
                    self.nl();
                    return;
                }
                _ => {
                    self.space();
                    self.emit_statement(child);
                    return;
                }
            }
            prev_end = child.end_byte();
            i += 1;
        }
        self.nl();
    }

    // ── C++ namespace / class / template ─────────────────────────────────────

    fn emit_namespace_definition(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "namespace" => {
                    self.raw("namespace");
                }
                "identifier" | "namespace_identifier" => {
                    self.space();
                    self.raw(self.node_text(*child));
                }
                "declaration_list" | "compound_statement" => {
                    if self.config.braces.fn_brace_newline {
                        self.nl();
                        self.emit_indent();
                    } else {
                        self.space();
                    }
                    self.raw("{");
                    self.nl();
                    self.depth += 1;
                    self.emit_compound_body(*child);
                    self.depth -= 1;
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw("}");
                    self.nl();
                    return;
                }
                _ => {
                    self.space();
                    self.emit_expr_node(*child);
                }
            }
        }
        self.nl();
    }

    fn emit_template_declaration(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let mut i = 0;

        while i < children.len() {
            let child = children[i];
            match child.kind() {
                "template" => {
                    self.raw("template");
                    i += 1;
                }
                "template_parameter_list" => {
                    // No space between `template` and `<` (ws_before handles inner spacing).
                    self.emit_expr_node(child);
                    i += 1;
                }
                "function_definition" => {
                    self.nl();
                    self.emit_indent();
                    self.emit_function_definition(child);
                    self.ensure_nl();
                    return;
                }
                "class_specifier" | "struct_specifier" => {
                    self.nl();
                    self.emit_indent();
                    self.emit_class_like(child);
                    self.raw(";");
                    self.nl();
                    return;
                }
                "declaration" => {
                    self.nl();
                    self.emit_indent();
                    self.emit_decl_node(child);
                    self.nl();
                    return;
                }
                _ => {
                    self.space();
                    self.emit_expr_node(child);
                    i += 1;
                }
            }
        }
        self.nl();
    }

    fn emit_class_like(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        for child in &children {
            // No emit_blank_lines here — keywords are inline, body handles its own newlines.
            match child.kind() {
                "class" | "struct" | "union" => {
                    if !self.at_bol && !self.out.ends_with(' ') {
                        self.space();
                    }
                    self.raw(child.kind());
                }
                "type_identifier" | "identifier" => {
                    if !self.at_bol {
                        self.space();
                    }
                    self.raw(self.node_text(*child));
                }
                "base_class_clause" => {
                    // No space before the clause's leading `:` (funky: `class C: public Base`).
                    self.emit_expr_node(*child);
                }
                "field_declaration_list" | "declaration_list" => {
                    // Class/struct bodies always use same-line brace (KR).
                    self.space();
                    self.raw("{");
                    self.nl();
                    self.depth += 1;
                    self.emit_class_body(*child);
                    self.depth -= 1;
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw("}");
                }
                _ => {
                    if !self.at_bol && !self.out.ends_with(' ') {
                        self.space();
                    }
                    self.emit_expr_node(*child);
                }
            }
        }
    }

    fn emit_class_body(&mut self, node: Node) {
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
        let mut prev_end = if start_i > 0 {
            children[0].end_byte()
        } else {
            node.start_byte()
        };

        let mut i = start_i;
        while i < end_i {
            let child = children[i];
            let kind = child.kind();
            let blanks = self.source_blanks(prev_end, child.start_byte());
            self.emit_blank_lines(blanks);

            match kind {
                k if is_preproc(k) => {
                    self.ensure_nl();
                    self.emit_preproc(child);
                }
                "comment" => {
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw(self.node_text(child).trim_end_matches('\n'));
                    self.nl();
                }
                "access_specifier" => {
                    // access_specifier contains just the keyword; ":" is the next sibling.
                    self.ensure_nl();
                    if self.depth > 0 {
                        self.emit_indent_at(self.depth - 1);
                    }
                    self.raw(self.node_text(child));
                    // Consume the ":" sibling if present.
                    if i + 1 < end_i && children[i + 1].kind() == ":" {
                        self.raw(":");
                        i += 1;
                    }
                    self.nl();
                }
                ":" => {
                    // Standalone ":" after access_specifier — should have been consumed above.
                    // Emit defensively.
                    self.raw(":");
                    self.nl();
                }
                "function_definition" => {
                    self.ensure_nl();
                    self.emit_indent();
                    self.emit_function_definition(child);
                    self.ensure_nl();
                }
                _ => {
                    self.ensure_nl();
                    self.emit_indent();
                    self.emit_class_member(child);
                    self.nl();
                }
            }
            prev_end = children[i].end_byte();
            i += 1;
        }
    }

    #[allow(dead_code)]
    fn emit_access_specifier(&mut self, node: Node) {
        let text = self.node_text(node);
        self.raw(text.trim_end_matches(':'));
        self.raw(":");
    }

    fn emit_class_member(&mut self, node: Node) {
        match node.kind() {
            "declaration" => self.emit_decl_node(node),
            "type_definition" => self.emit_typedef_node(node),
            "field_declaration" => self.emit_field_declaration(node),
            _ => {
                let mut leaves = vec![];
                collect_leaves(node, &mut leaves);
                self.emit_leaves(&leaves);
            }
        }
    }

    fn emit_field_declaration(&mut self, node: Node) {
        let mut leaves = vec![];
        collect_leaves(node, &mut leaves);
        self.emit_leaves(&leaves);
    }

    fn emit_try_statement(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "try" => {
                    self.raw("try");
                }
                "compound_statement" => {
                    self.space();
                    self.raw("{");
                    self.nl();
                    self.depth += 1;
                    self.emit_compound_body(*child);
                    self.depth -= 1;
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw("}");
                }
                "catch_clause" => {
                    self.emit_catch_clause(*child);
                }
                _ => {
                    self.space();
                    self.emit_expr_node(*child);
                }
            }
        }
    }

    fn emit_catch_clause(&mut self, node: Node) {
        if self.config.braces.cuddle_catch {
            self.space();
        } else {
            self.nl();
            self.emit_indent();
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "catch" => {
                    self.raw("catch");
                }
                "parameter_list" | "parenthesized_expression" => {
                    if self.config.spacing.space_before_keyword_paren {
                        self.space();
                    }
                    self.emit_expr_node(child);
                }
                "compound_statement" => {
                    self.space();
                    self.raw("{");
                    self.nl();
                    self.depth += 1;
                    self.emit_compound_body(child);
                    self.depth -= 1;
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw("}");
                }
                _ => {
                    self.space();
                    self.emit_expr_node(child);
                }
            }
        }
    }

    // ── Declaration / typedef ─────────────────────────────────────────────────

    fn emit_decl_node(&mut self, node: Node) {
        self.emit_decl_children(node);
    }

    fn emit_typedef_node(&mut self, node: Node) {
        self.emit_decl_children(node);
    }

    fn emit_decl_children(&mut self, node: Node) {
        // Walk the declaration's children, expanding struct/union/enum bodies.
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let mut prev: Option<Node> = None;

        for &child in &children {
            let k = child.kind();
            match k {
                "struct_specifier" | "union_specifier" => {
                    if prev.is_some() && !self.at_bol {
                        self.space();
                    }
                    self.emit_struct_like(child);
                }
                "enum_specifier" => {
                    if prev.is_some() && !self.at_bol {
                        self.space();
                    }
                    self.emit_enum_like(child);
                }
                "class_specifier" => {
                    if prev.is_some() && !self.at_bol {
                        self.space();
                    }
                    self.emit_class_like(child);
                }
                ";" => {
                    self.raw(";");
                }
                "init_declarator" => {
                    // Emit init_declarator: declarator part + `=` + initializer.
                    if prev.is_some() && !self.at_bol {
                        let prev_kind = prev.map(|n| n.kind()).unwrap_or("");
                        if prev_kind == "," {
                            // `int a = x, b = y` — space after comma between declarators.
                            self.space();
                        } else if !matches!(
                            prev_kind,
                            "pointer_declarator"
                                | "abstract_pointer_declarator"
                                | "reference_declarator"
                        ) {
                            // Check if init_declarator starts with a function_declarator
                            // that has a non-simple paren first child (no space needed).
                            let first_fn_decl = {
                                let mut ic = child.walk();
                                let ch: Vec<Node> = child.children(&mut ic).collect();
                                ch.into_iter().find(|n| n.kind() == "function_declarator")
                            };
                            let suppress = first_fn_decl
                                .map(|fd| {
                                    fn_declarator_has_paren_first_child(fd)
                                        && !is_simple_fn_ptr_declarator(fd)
                                })
                                .unwrap_or(false);
                            if !suppress {
                                self.space();
                            }
                        }
                    }
                    self.emit_init_declarator(child);
                }
                _ => {
                    if prev.is_some() && !self.at_bol {
                        let prev_kind = prev.map(|n| n.kind()).unwrap_or("");
                        let mut needs_space = k != ","
                            && !matches!(
                                prev_kind,
                                "pointer_declarator"
                                    | "abstract_pointer_declarator"
                                    | "reference_declarator"
                            );
                        // `int(fp)()` — function_declarator whose parenthesized_declarator
                        // wraps only an identifier (not a pointer_declarator) needs no
                        // space before it (matching funky's `next_is_fn_ptr_declarator`
                        // returning false for non-pointer names).
                        if needs_space && k == "function_declarator" {
                            // Suppress space for `int(fp)()` and `int(*f(p))()` but
                            // keep space for simple fn-ptr form `int (*fp)()`.
                            if fn_declarator_has_paren_first_child(child)
                                && !is_simple_fn_ptr_declarator(child)
                            {
                                needs_space = false;
                            }
                        }
                        if needs_space {
                            self.space();
                        }
                    }
                    // Inline struct as part of declaration (no body).
                    // Otherwise emit leaves.
                    let mut leaves = vec![];
                    collect_leaves(child, &mut leaves);
                    self.emit_leaves(&leaves);
                }
            }
            prev = Some(child);
        }
    }

    fn emit_init_declarator(&mut self, node: Node) {
        // init_declarator: declarator `=` value
        // The value might be an initializer_list that needs expansion.
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        let mut prev: Option<Node> = None;
        for &child in &children {
            let ck = child.kind();
            if ck == "initializer_list" {
                self.space();
                self.emit_initializer_list(child);
            } else if ck == "=" {
                self.space();
                self.raw("=");
                // Record column after `= ` so that a compound-literal `{` on
                // the next line can align to it (matching funky's assign_col).
                self.assign_col_for_brace = Some(self.current_col() + 1);
            } else {
                if prev.is_some() && !self.at_bol {
                    // `Type name(args);` direct-initialization: the `(` is
                    // call-like, so it follows space_before_call_paren rather
                    // than always getting a space.
                    if ck == "argument_list" {
                        if self.config.spacing.space_before_call_paren {
                            self.space();
                        }
                    } else {
                        self.space();
                    }
                }
                let mut leaves = vec![];
                collect_leaves(child, &mut leaves);
                self.emit_leaves(&leaves);
            }
            prev = Some(child);
        }
    }

    // ── Struct / union / enum bodies ──────────────────────────────────────────

    fn emit_struct_like(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            // No blank-line logic here; keywords are inline, bodies handle their own.
            match child.kind() {
                "struct" | "union" => {
                    if !self.at_bol && !self.out.ends_with(' ') {
                        self.space();
                    }
                    self.raw(child.kind());
                }
                "type_identifier" | "identifier" => {
                    if !self.at_bol {
                        self.space();
                    }
                    self.raw(self.node_text(*child));
                }
                "field_declaration_list" => {
                    self.space();
                    self.raw("{");
                    self.nl();
                    self.depth += 1;
                    self.emit_field_list(*child);
                    self.depth -= 1;
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw("}");
                }
                _ => {
                    if !self.at_bol && !self.out.ends_with(' ') {
                        self.space();
                    }
                    self.emit_expr_node(*child);
                }
            }
        }
    }

    fn emit_enum_like(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "enum" => {
                    if !self.at_bol && !self.out.ends_with(' ') {
                        self.space();
                    }
                    self.raw("enum");
                }
                "type_identifier" | "identifier" => {
                    if !self.at_bol {
                        self.space();
                    }
                    self.raw(self.node_text(*child));
                }
                "enumerator_list" => {
                    self.space();
                    self.raw("{");
                    self.nl();
                    self.depth += 1;
                    self.emit_enumerator_list(*child);
                    self.depth -= 1;
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw("}");
                }
                _ => {
                    if !self.at_bol && !self.out.ends_with(' ') {
                        self.space();
                    }
                    self.emit_expr_node(*child);
                }
            }
        }
    }

    /// Render `node` via `emit_decl_children` to a temp buffer and return the
    /// number of bytes on the last (and only expected) output line.
    fn measure_decl_render(&mut self, node: Node) -> usize {
        let saved_out = std::mem::take(&mut self.out);
        let saved_bol = self.at_bol;
        self.at_bol = false;
        self.emit_decl_children(node);
        let rendered = std::mem::replace(&mut self.out, saved_out);
        self.at_bol = saved_bol;
        // Return chars on the last line (handles unlikely multi-line case).
        rendered.len() - rendered.rfind('\n').map(|p| p + 1).unwrap_or(0)
    }

    fn emit_field_list(&mut self, node: Node) {
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
        let mut prev_end = if start_i > 0 {
            children[0].end_byte()
        } else {
            node.start_byte()
        };

        // Pre-scan: find (field_i, comment_i) pairs for same-line trailing comments.
        let mut same_line_pairs: Vec<(usize, usize)> = Vec::new();
        for i in start_i..end_i {
            let child = children[i];
            if child.kind() == "comment" {
                continue;
            }
            if i + 1 < end_i && children[i + 1].kind() == "comment" {
                let field_end_line = self.src[..child.end_byte()].lines().count();
                let cmt_line = self.src[..children[i + 1].start_byte()].lines().count();
                if field_end_line == cmt_line {
                    same_line_pairs.push((i, i + 1));
                }
            }
        }

        // Compute alignment column: indent + max rendered field width + 2.
        let align_col = if !same_line_pairs.is_empty() {
            let indent = self.depth as usize * self.config.indent.width as usize;
            let max_w = same_line_pairs
                .iter()
                .map(|&(fi, _)| self.measure_decl_render(children[fi]))
                .max()
                .unwrap_or(0);
            indent + max_w + 2
        } else {
            0
        };
        let has_trailing_cmt: std::collections::HashSet<usize> =
            same_line_pairs.iter().map(|&(fi, _)| fi).collect();

        let mut i = start_i;
        while i < end_i {
            let child = children[i];
            let kind = child.kind();
            let blanks = self.source_blanks(prev_end, child.start_byte());
            self.emit_blank_lines(blanks);

            match kind {
                "comment" => {
                    // Stand-alone comment (not consumed as a trailing inline comment).
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw(self.node_text(child).trim_end_matches('\n'));
                    self.nl();
                }
                k if is_preproc(k) => {
                    self.ensure_nl();
                    self.emit_preproc(child);
                }
                ";" => {
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw(";");
                    self.nl();
                }
                "ERROR" => {
                    // Bare `;` error nodes (anonymous field) at column 0.
                    let text = self.node_text(child).trim();
                    if text == ";" {
                        self.ensure_nl();
                        self.raw(";");
                        self.nl();
                    } else {
                        self.ensure_nl();
                        self.emit_indent();
                        self.emit_decl_children(child);
                        self.nl();
                    }
                }
                _ => {
                    self.ensure_nl();
                    self.emit_indent();
                    self.emit_decl_children(child);
                    if has_trailing_cmt.contains(&i) {
                        // Align the trailing comment to `align_col`.
                        let cur_col =
                            self.out.len() - self.out.rfind('\n').map(|p| p + 1).unwrap_or(0);
                        let pad = if cur_col < align_col {
                            align_col - cur_col
                        } else {
                            1
                        };
                        for _ in 0..pad {
                            self.out.push(' ');
                        }
                        self.at_bol = false;
                        let cmt = children[i + 1];
                        self.raw(self.node_text(cmt).trim_end_matches('\n'));
                        prev_end = cmt.end_byte();
                        i += 2;
                    } else {
                        prev_end = child.end_byte();
                        i += 1;
                    }
                    self.nl();
                    continue;
                }
            }
            prev_end = child.end_byte();
            i += 1;
        }
    }

    fn emit_enumerator_list(&mut self, node: Node) {
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

        // Group enumerators by source row (preserves source line structure).
        let mut prev_row: Option<usize> = None;
        let mut prev_end = if start_i > 0 {
            children[0].end_byte()
        } else {
            node.start_byte()
        };
        for child in &children[start_i..end_i] {
            let child = *child;
            let kind = child.kind();

            match kind {
                "," => {
                    self.raw(",");
                }
                "comment" => {
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw(self.node_text(child).trim_end_matches('\n'));
                    self.nl();
                    prev_row = None;
                }
                _ => {
                    let blanks = self.source_blanks(prev_end, child.start_byte());
                    let row = child.start_position().row;
                    let same_row = prev_row.map(|r| r == row).unwrap_or(false);

                    if !same_row {
                        self.emit_blank_lines(blanks);
                        self.ensure_nl();
                        self.emit_indent();
                    } else {
                        // Same source row: keep on same output line after `,`.
                        self.space();
                    }

                    let mut leaves = vec![];
                    collect_leaves(child, &mut leaves);
                    self.emit_leaves(&leaves);
                    prev_row = Some(row);
                }
            }
            prev_end = child.end_byte();
        }
        self.ensure_nl();
    }

    // ── Expression formatting (leaf walker) ───────────────────────────────────

    fn emit_expr_node(&mut self, node: Node) {
        match node.kind() {
            "struct_specifier" => self.emit_struct_like(node),
            "union_specifier" => self.emit_struct_like(node),
            "enum_specifier" => self.emit_enum_like(node),
            "class_specifier" => self.emit_class_like(node),
            "initializer_list" => self.emit_initializer_list(node),
            "lambda_expression" => self.emit_lambda_expression(node),
            _ => {
                let mut leaves = vec![];
                collect_leaves(node, &mut leaves);
                self.emit_leaves(&leaves);
            }
        }
    }

    /// C++ lambda: `[capture](params) { body }`.
    /// When fn_brace_newline is on, the body `{`, statements, and `}` are all
    /// placed on separate lines, column-aligned to the `[` of the capture.
    fn emit_lambda_expression(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let body_idx = children
            .iter()
            .position(|n| n.kind() == "compound_statement");

        let sig_nodes: &[Node] = match body_idx {
            Some(bi) => &children[..bi],
            None => &children,
        };

        // Record column of the `[` (first char of capture specifier) before emitting.
        let lambda_col = self.current_col();

        // Emit sig (capture + params + trailing qualifiers).
        for child in sig_nodes {
            let mut sig_leaves = vec![];
            collect_leaves(*child, &mut sig_leaves);
            self.emit_leaves(&sig_leaves);
        }

        if let Some(bi) = body_idx {
            let body = children[bi];
            // Determine if last sig token is `)` — if so, fn_brace_newline applies.
            let sig_ends_rparen = sig_nodes
                .last()
                .map(|&n| {
                    let mut node = n;
                    while node.child_count() > 0 {
                        node = node.child(node.child_count() - 1).unwrap();
                    }
                    node.kind() == ")"
                })
                .unwrap_or(false);

            // The column-aligned quirk is an emergent side-effect of funky's
            // generic paren-continuation tracking: it only shows up when the
            // lambda is itself a call argument (e.g. `f([&]() {...})`). A
            // lambda used as a return value or assignment RHS gets normal
            // block indentation instead.
            let is_call_argument = node
                .parent()
                .map(|p| p.kind() == "argument_list")
                .unwrap_or(false);

            if self.config.braces.fn_brace_newline && sig_ends_rparen && is_call_argument {
                // Column-aligned lambda body: `{`, each statement, `}` all at lambda_col.
                let col_indent = " ".repeat(lambda_col);
                self.nl();
                self.raw(&col_indent);
                self.raw("{");
                // Emit each statement in the body at lambda_col.
                let mut bcursor = body.walk();
                let body_children: Vec<Node> = body.children(&mut bcursor).collect();
                for bchild in &body_children {
                    match bchild.kind() {
                        "{" | "}" => {}
                        _ => {
                            self.nl();
                            self.raw(&col_indent);
                            self.emit_statement(*bchild);
                            // Strip trailing newline from emit_statement.
                            if self.out.ends_with('\n') {
                                self.out.pop();
                                self.at_bol = false;
                            }
                        }
                    }
                }
                self.nl();
                self.raw(&col_indent);
                self.raw("}");
                // Signal emit_leaves to put the closing `)` and `;` on their
                // own lines at the same column (funky's continuation behaviour).
                self.last_lambda_col = Some(lambda_col);
            } else {
                // Inline or regular body.
                let inline = !self.config.braces.fn_brace_newline
                    && body.start_position().row == body.end_position().row;
                if inline {
                    self.emit_compound_body_inline(body);
                } else {
                    self.nl();
                    self.emit_indent();
                    self.raw("{");
                    self.nl();
                    self.depth += 1;
                    self.emit_compound_body(body);
                    self.depth -= 1;
                    self.ensure_nl();
                    self.emit_indent();
                    self.raw("}");
                }
            }
        }
    }

    fn emit_initializer_list(&mut self, node: Node) {
        let src_range = &self.src[node.start_byte()..node.end_byte()];
        let has_newline = src_range.contains('\n');

        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // Small/flat inline initializer.
        if !has_newline {
            let mut leaves = vec![];
            collect_leaves(node, &mut leaves);
            self.emit_leaves(&leaves);
            return;
        }

        // Multi-line: preserve source line groupings.
        // Elements on the same source row are kept on the same output line.
        self.raw("{");
        self.nl();
        self.depth += 1;

        let start_i = 1; // skip `{`
        let end_i = if children.last().map(|n| n.kind()) == Some("}") {
            children.len() - 1
        } else {
            children.len()
        };

        // Collect (element, had_trailing_comma) pairs grouped by source row.
        // Skip `,` separators; track which elements are followed by commas.
        struct Elem<'t> {
            node: Node<'t>,
            row: usize,
            has_comma: bool,
        }
        let mut elems: Vec<Elem> = vec![];
        for child in &children[start_i..end_i] {
            let child = *child;
            if child.kind() == "," {
                if let Some(last) = elems.last_mut() {
                    last.has_comma = true;
                }
                continue;
            }
            let row = child.start_position().row;
            elems.push(Elem {
                node: child,
                row,
                has_comma: false,
            });
        }

        let mut row_start = true;
        let mut prev_row: Option<usize> = None;
        let last_has_comma = elems.last().map(|e| e.has_comma).unwrap_or(false);

        for elem in &elems {
            if prev_row.map(|r| r != elem.row).unwrap_or(false) {
                // New source row: end previous line and start new one.
                self.raw(",");
                self.nl();
                row_start = true;
            }
            if row_start {
                // `initializer_list` elements (starting with `{`) are written
                // with no leading indent — funky's BraceCtx::Other logic calls
                // space() which does nothing at line-start, so the `{` lands at
                // its source column (typically 0). All other elements use the
                // standard indent for the current depth.
                if elem.node.kind() != "initializer_list" {
                    self.emit_indent();
                }
                row_start = false;
            } else {
                self.raw(",");
                self.space();
            }
            self.emit_expr_node(elem.node);
            prev_row = Some(elem.row);
        }
        // Trailing comma after last element only if source had one.
        if !elems.is_empty() {
            if last_has_comma {
                self.raw(",");
            }
            self.nl();
        }

        self.depth -= 1;
        // Closing `}` always at current indent depth.
        self.emit_indent();
        self.raw("}");
    }

    fn emit_leaves<'t>(&mut self, leaves: &[Node<'t>]) {
        // paren_col_stack tracks the column after each `(` so that source
        // newlines inside parenthesized expressions are re-indented to align
        // with the opening paren (matching funky's continuation behavior).
        let mut paren_col_stack: Vec<usize> = Vec::new();
        // bracket_depth for `[...]` — used to determine when `=` is at
        // statement level vs inside a subscript.
        let mut bracket_depth: i32 = 0;
        // assign_col_for_brace tracks the column after `= ` so that a
        // compound-literal `{` on the next line aligns to it.  Initialized
        // from the Fmt field (set by emit_init_declarator) and also updated
        // inline when `=` is seen inside emit_leaves itself.
        let mut assign_col_for_brace: Option<usize> = self.assign_col_for_brace.take();
        // source_brace_depth > 0 when we're inside a `{...}` block that started
        // on a new line outside parens (e.g. a compound literal's initializer).
        // In that mode, all source newlines are preserved using the source column
        // plus an offset (to account for surrounding indentation).
        let mut source_brace_depth: i32 = 0;
        // Delta added to source column when in source_brace_depth mode.
        let mut source_brace_col_delta: i32 = 0;

        for (i, &leaf) in leaves.iter().enumerate() {
            let prev = if i > 0 { Some(leaves[i - 1]) } else { None };
            // Lambda expressions are collected as opaque pseudo-leaves; dispatch them.
            if leaf.kind() == "lambda_expression" {
                let ws = self.ws_before_lambda(leaf, prev);
                match ws {
                    Ws::None => {}
                    Ws::Space => self.space(),
                }
                self.emit_lambda_expression(leaf);
                continue;
            }
            // After a column-aligned lambda, `)` goes on its own line at the
            // lambda's column; `;` follows on that same line (funky's continuation).
            if let Some(col) = self.last_lambda_col {
                if leaf.kind() == ")" {
                    self.nl();
                    self.raw(&" ".repeat(col));
                    self.raw(")");
                    paren_col_stack.pop();
                    continue; // keep last_lambda_col set so `;` appends inline
                } else if leaf.kind() == ";" {
                    self.raw(";");
                    self.last_lambda_col = None;
                    continue;
                } else {
                    self.last_lambda_col = None;
                }
            }

            // Source-newline preservation inside parentheses: if the source
            // has a newline between prev and this token and we're inside
            // open parens, emit a newline + align to the paren column.
            let source_has_nl = if let Some(p) = prev {
                self.src[p.end_byte()..leaf.start_byte()].contains('\n')
            } else {
                false
            };

            if source_has_nl && !paren_col_stack.is_empty() {
                let col = *paren_col_stack.last().unwrap();
                self.nl();
                self.raw(&" ".repeat(col));
                let text = self.node_text(leaf);
                self.raw(text);
                if leaf.kind() == "(" {
                    paren_col_stack.push(self.current_col());
                } else if leaf.kind() == ")" {
                    paren_col_stack.pop();
                }
                continue;
            }

            // Source-column mode: a `{` that appeared on a new line outside
            // parens (compound literal initializer) entered source-column mode.
            // Preserve source column (+ delta) for all tokens until the matching `}`.
            if source_brace_depth > 0 {
                if leaf.kind() == "}" {
                    source_brace_depth -= 1;
                }
                if source_has_nl {
                    let src_col = leaf.start_position().column as i32;
                    let out_col = (src_col + source_brace_col_delta).max(0) as usize;
                    self.nl();
                    self.raw(&" ".repeat(out_col));
                    self.raw(self.node_text(leaf));
                } else {
                    let ws = self.ws_before(leaf, prev);
                    match ws {
                        Ws::None => {}
                        Ws::Space => self.space(),
                    }
                    self.raw(self.node_text(leaf));
                }
                if leaf.kind() == "{" {
                    source_brace_depth += 1;
                }
                continue;
            }

            // Detect compound-literal `{` on its own line: enter source-column mode.
            // Use assign_col_for_brace (column after `= `) if available, otherwise
            // use the source column directly.
            if source_has_nl && leaf.kind() == "{" && paren_col_stack.is_empty() {
                let src_col = leaf.start_position().column;
                let (out_col, delta) = if let Some(ac) = assign_col_for_brace {
                    (ac, ac as i32 - src_col as i32)
                } else {
                    (src_col, 0)
                };
                assign_col_for_brace = None;
                self.nl();
                self.raw(&" ".repeat(out_col));
                self.raw("{");
                source_brace_depth = 1;
                source_brace_col_delta = delta;
                continue;
            }

            let ws = self.ws_before(leaf, prev);
            match ws {
                Ws::None => {}
                Ws::Space => self.space(),
            }
            let text = self.node_text(leaf);
            self.raw(text);

            // Update paren/bracket column tracking.
            if leaf.kind() == "(" {
                paren_col_stack.push(self.current_col());
            } else if leaf.kind() == ")" {
                paren_col_stack.pop();
            } else if leaf.kind() == "[" {
                bracket_depth += 1;
            } else if leaf.kind() == "]" {
                bracket_depth = bracket_depth.saturating_sub(1);
            }

            // Track assignment column: when `=` is emitted outside parens and
            // brackets, note the column where the RHS will start (col after `= `).
            // This is used to align compound-literal `{` on the next line.
            if leaf.kind() == "="
                && paren_col_stack.is_empty()
                && bracket_depth == 0
                && assign_col_for_brace.is_none()
            {
                // After writing `=`, the next token will have a space before it,
                // so the brace column is current_col + 1 (for the space).
                assign_col_for_brace = Some(self.current_col() + 1);
            } else if leaf.kind() == ";" {
                assign_col_for_brace = None;
            }
        }
    }

    fn ws_before_open_paren<'t>(
        &self,
        node: Node<'t>,
        prev: Option<Node<'t>>,
        prev_kind: &str,
        prev_text: &str,
    ) -> Ws {
        // sizeof/alignof/typeof are keywords syntactically but behave like calls.
        let is_call_like_keyword = matches!(
            prev_text,
            "sizeof"
                | "alignof"
                | "typeof"
                | "offsetof"
                | "__typeof__"
                | "__alignof__"
                | "_Alignof"
        );
        // Don't apply space_before_keyword_paren for type keywords
        // (e.g., `void(*)` in function pointer type descriptors).
        let is_type_kw = matches!(
            prev_kind,
            "primitive_type" | "type_identifier" | "type_qualifier"
        );
        if is_keyword_str(prev_text) && !is_call_like_keyword && !is_type_kw {
            return if self.config.spacing.space_before_keyword_paren {
                Ws::Space
            } else {
                Ws::None
            };
        }
        // After binary/ternary operators: always space before `(`.
        // But NOT after unary `*` (pointer dereference), `&` (address-of),
        // or pointer-declarator `*`/`&` (these look like binary ops but aren't).
        let prev_is_unary = matches!(prev_kind, "*" | "&" | "!" | "~" | "-" | "+")
            && prev
                .and_then(|n| n.parent())
                .map(|p| {
                    matches!(
                        p.kind(),
                        "pointer_expression"
                            | "unary_expression"
                            | "pointer_declarator"
                            | "abstract_pointer_declarator"
                            | "reference_declarator"
                            | "abstract_reference_declarator"
                    )
                })
                .unwrap_or(false);
        // `>` closing a template argument list is NOT a binary operator.
        let prev_is_template_close = prev_kind == ">"
            && prev
                .and_then(|n| n.parent())
                .map(|p| matches!(p.kind(), "template_argument_list" | "template_type"))
                .unwrap_or(false);
        if !prev_is_unary
            && !prev_is_template_close
            && (is_binary_op_kind(prev_kind)
                || is_compound_assign(prev_kind)
                || matches!(prev_kind, "=" | "?" | ":" | ","))
        {
            return Ws::Space;
        }
        // Call-like: no space by default.
        if matches!(prev_kind, "identifier" | "type_identifier" | ")" | "]") || is_call_like_keyword
        {
            // `arr[i](args)` — calling through a subscript result: funky's
            // needs_space(LParen) falls through to `return true` for `]`
            // (it's not treated as Ident/Keyword). Add space to match.
            if prev_kind == "]" {
                return Ws::Space;
            }
            return if self.config.spacing.space_before_call_paren {
                Ws::Space
            } else {
                Ws::None
            };
        }
        // `(` is the opening paren of a `parenthesized_declarator` inside a
        // `function_declarator`.  Space rules:
        // - Simple fn-ptr `(*name)` after type/keyword/ptr-star → space
        //   (funky: `needs_space(LParen)` returns true for most prev tokens)
        // - Non-simple `(name)` form or compound fn-ptr-of-fn-ptr → no space
        if node.parent().map(|p| p.kind()) == Some("parenthesized_declarator") {
            if let Some(fn_decl) = node.parent().and_then(|p| p.parent()) {
                if fn_decl.kind() == "function_declarator" {
                    // `prev_is_decl_star` is true only for top-level pointer
                    // declarator stars (not nested inside another paren), e.g.
                    // `typedef S * (*fty)()` but NOT `int(* (*p)...)`.
                    // funky: inside a paren, `*` is processed by fmt_unary_binary
                    // which sets suppress_next_space; at top level it uses
                    // fmt_ptr_decl which does NOT suppress the next space for the
                    // paren → the space is added via needs_space fallthrough.
                    let prev_is_decl_star = matches!(prev_kind, "*" | "&")
                        && prev
                            .and_then(|n| n.parent())
                            .map(|p| {
                                matches!(
                                    p.kind(),
                                    "pointer_declarator"
                                        | "abstract_pointer_declarator"
                                        | "reference_declarator"
                                )
                            })
                            .unwrap_or(false)
                        && prev
                            .and_then(|n| n.parent())
                            .and_then(|p| p.parent())
                            .map(|gp| gp.kind() != "parenthesized_declarator")
                            .unwrap_or(true);
                    let prev_is_type = matches!(
                        prev_kind,
                        "primitive_type"
                            | "type_identifier"
                            | "type_qualifier"
                            | "identifier"
                            | ")"
                            | ">"
                    );
                    if (prev_is_type || prev_is_decl_star) && is_simple_fn_ptr_declarator(fn_decl) {
                        return Ws::Space;
                    }
                    return Ws::None;
                }
            }
        }
        Ws::None
    }

    fn ws_after_comma<'t>(&self, prev: Option<Node<'t>>) -> Ws {
        // Replicate funky's suppress_next_space quirk: when the token before `,`
        // was a pointer `*` that followed a type KEYWORD (not an identifier),
        // suppress the space. `char *,T` → no space; `struct S *,T` → space.
        if self.config.spacing.space_after_comma {
            let star_before_comma = prev
                .and_then(|comma| comma.prev_sibling())
                .map(|before| {
                    let leaf = last_leaf_of(before);
                    if leaf.kind() != "*" {
                        return false;
                    }
                    let in_ptr = leaf.parent().is_some_and(|p| {
                        matches!(
                            p.kind(),
                            "abstract_pointer_declarator" | "pointer_declarator"
                        )
                    });
                    if !in_ptr {
                        return false;
                    }
                    // Only suppress when the leaf before the `*` is a type keyword
                    // (not an ident like a struct tag name).
                    let prev_sibling = leaf
                        .prev_sibling()
                        .or_else(|| leaf.parent()?.prev_sibling());
                    prev_sibling.is_some_and(|ps| {
                        let last = last_leaf_of(ps);
                        matches!(
                            last.kind(),
                            "primitive_type" | "type_qualifier" | "sized_type_specifier"
                        ) || (last.kind() == "*"
                            && last.parent().is_some_and(|p| {
                                matches!(
                                    p.kind(),
                                    "abstract_pointer_declarator" | "pointer_declarator"
                                )
                            }))
                    })
                })
                .unwrap_or(false);
            if star_before_comma {
                return Ws::None;
            }
            return Ws::Space;
        }
        Ws::None
    }

    /// ws_before for a lambda_expression pseudo-leaf: space after `,`, none after `(`.
    fn ws_before_lambda<'t>(&self, _node: Node<'t>, prev: Option<Node<'t>>) -> Ws {
        match prev.map(|n| n.kind()).unwrap_or("") {
            "" | "(" | "[" => Ws::None,
            _ => Ws::Space,
        }
    }

    fn ws_before<'t>(&self, node: Node<'t>, prev: Option<Node<'t>>) -> Ws {
        let kind = node.kind();
        let text = self.node_text(node);
        let prev_kind = prev.map(|n| n.kind()).unwrap_or("");
        let prev_text = prev.map(|n| self.node_text(n)).unwrap_or("");

        // First token — no space.
        if prev_kind.is_empty() {
            return Ws::None;
        }

        // No space before `[` in array declarators (e.g. `int[]`, `int[5]`, `int a[5]`).
        if kind == "[" {
            let in_array_decl = node
                .parent()
                .map(|p| {
                    matches!(
                        p.kind(),
                        "abstract_array_declarator"
                            | "abstract_sized_array_declarator"
                            | "array_declarator"
                    )
                })
                .unwrap_or(false);
            if in_array_decl {
                return Ws::None;
            }
        }

        // Space between consecutive subscript designators `] [` (e.g. `[1] [2]`).
        if prev_kind == "]" && kind == "[" {
            // Preserve source spacing for multi-dimensional subscript designators.
            let gap_start = prev.map(|n| n.end_byte()).unwrap_or(0);
            return if self.src.as_bytes().get(gap_start) == Some(&b' ') {
                Ws::Space
            } else {
                Ws::None
            };
        }

        // Never space after these openers (except initializer_list `{`).
        match prev_kind {
            "(" => return Ws::None,
            "{" => {
                let in_init = prev
                    .and_then(|n| n.parent())
                    .map(|p| p.kind() == "initializer_list")
                    .unwrap_or(false);
                return if in_init { Ws::Space } else { Ws::None };
            }
            "->" | "." | "::" => return Ws::None,
            _ => {}
        }

        // Space before `}` in initializer_list.
        if kind == "}" {
            let in_init = node
                .parent()
                .map(|p| p.kind() == "initializer_list")
                .unwrap_or(false);
            if in_init {
                return Ws::Space;
            }
            return Ws::None;
        }

        // `operator=` / `operator()` — no space inside operator_name, and
        // no space between the last symbol of operator_name and the `(` that
        // follows it as a sibling in function_declarator.
        if prev_kind == "operator"
            && prev.and_then(|n| n.parent()).map(|p| p.kind()) == Some("operator_name")
        {
            return Ws::None;
        }
        if kind == "(" && prev.and_then(|n| n.parent()).map(|p| p.kind()) == Some("operator_name") {
            return Ws::None;
        }

        // No space before these closers/separators.
        match kind {
            ")" | ";" => return Ws::None,
            "::" => return Ws::None,
            "," => return Ws::None,
            "." | "->" => {
                // Allow space before `.` when it's a field_designator (`.x = val` in initializers).
                let is_designator = node
                    .parent()
                    .map(|p| p.kind() == "field_designator")
                    .unwrap_or(false);
                if !is_designator {
                    return Ws::None;
                }
                // Fall through to normal spacing rules (comma → space).
            }
            _ => {}
        }

        // space_inside_brackets handling for `[` and `]`.
        if prev_kind == "[" {
            return match self.config.spacing.space_inside_brackets {
                SpaceOption::Add => Ws::Space,
                SpaceOption::Remove => Ws::None,
                SpaceOption::Preserve => {
                    // Check if source had a space after `[`.
                    let gap_start = prev.map(|n| n.end_byte()).unwrap_or(0);
                    if self.src.as_bytes().get(gap_start) == Some(&b' ') {
                        Ws::Space
                    } else {
                        Ws::None
                    }
                }
            };
        }
        if kind == "]" {
            return match self.config.spacing.space_inside_brackets {
                SpaceOption::Add => Ws::Space,
                SpaceOption::Remove => Ws::None,
                SpaceOption::Preserve => {
                    let gap_start = prev.map(|n| n.end_byte()).unwrap_or(0);
                    if self.src.as_bytes().get(gap_start) == Some(&b' ') {
                        Ws::Space
                    } else {
                        Ws::None
                    }
                }
            };
        }

        // Suffix `++`/`--`: no space before.
        if matches!(kind, "++" | "--") && is_suffix(node) {
            return Ws::None;
        }

        // `(` spacing: call vs. keyword.
        if kind == "(" {
            return self.ws_before_open_paren(node, prev, prev_kind, prev_text);
        }

        // After `,`: space — but replicate funky's suppress_next_space quirk.
        if prev_kind == "," {
            return self.ws_after_comma(prev);
        }

        // Unary operators: no space after them.
        if is_unary_prefix(prev_kind, prev) {
            return Ws::None;
        }

        // Pointer declarator `*`: handle alignment.
        if kind == "*" || text == "*" {
            if let Some(parent) = node.parent() {
                match parent.kind() {
                    "pointer_declarator" | "abstract_pointer_declarator" => {
                        // No space between consecutive * in **.
                        if prev_kind == "*" {
                            return Ws::None;
                        }
                        return match self.config.spacing.pointer_align {
                            PointerAlign::Name | PointerAlign::Middle => Ws::Space,
                            PointerAlign::Type => Ws::None,
                        };
                    }
                    "pointer_expression" => {
                        // Unary dereference `*p`. Space-before follows the
                        // preceding token's own rules (e.g. `x = *p`), except
                        // it defaults to none — most contexts (call/group
                        // parens, casts, other unary operators) are handled
                        // by their own rules above/below.
                        if matches!(prev_kind, "=" | "==" | "!=" | "<=" | ">=" | "&&" | "||") {
                            return Ws::Space;
                        }
                        return Ws::None;
                    }
                    _ => {}
                }
            }
        }

        // After `*` in pointer_declarator: no space before name.
        if prev_kind == "*" {
            if let Some(p) = prev {
                if let Some(par) = p.parent() {
                    if matches!(
                        par.kind(),
                        "pointer_declarator" | "abstract_pointer_declarator"
                    ) {
                        return Ws::None;
                    }
                    if par.kind() == "pointer_expression" {
                        return Ws::None;
                    }
                }
            }
        }

        // Reference declarator `&`.
        if kind == "&" || text == "&" {
            if let Some(parent) = node.parent() {
                if matches!(
                    parent.kind(),
                    "reference_declarator" | "abstract_reference_declarator"
                ) {
                    return match self.config.spacing.pointer_align {
                        PointerAlign::Name | PointerAlign::Middle => Ws::Space,
                        PointerAlign::Type => Ws::None,
                    };
                }
                // For pointer_expression (address-of `&x`): space before `&` comes from
                // general rules (e.g., after `=`). Do NOT early-return here.
            }
        }

        // After `&` in reference_declarator or pointer_expression: no space before operand.
        if prev_kind == "&" {
            if let Some(p) = prev {
                if let Some(par) = p.parent() {
                    if matches!(
                        par.kind(),
                        "reference_declarator"
                            | "abstract_reference_declarator"
                            | "pointer_expression"
                    ) {
                        return Ws::None;
                    }
                }
            }
        }

        // Template angle brackets: `<` and `>` in template_parameter_list / template_argument_list.
        if kind == "<" || kind == ">" {
            if let Some(parent) = node.parent() {
                if matches!(
                    parent.kind(),
                    "template_parameter_list" | "template_argument_list"
                ) {
                    let sp = self.config.spacing.space_inside_angle_brackets;
                    return if sp { Ws::Space } else { Ws::None };
                }
            }
        }
        if prev_kind == "<" || prev_kind == ">" {
            if let Some(p) = prev {
                if let Some(par) = p.parent() {
                    if matches!(
                        par.kind(),
                        "template_parameter_list" | "template_argument_list"
                    ) {
                        let sp = self.config.spacing.space_inside_angle_brackets;
                        return if sp { Ws::Space } else { Ws::None };
                    }
                }
            }
        }

        // Binary operators: space around them (when space_around_binary_ops).
        if is_binary_op_kind(kind) {
            // After ternary `:`, no space before a unary operator (funky: `:-1`).
            if prev_kind == ":" && is_unary_prefix(kind, Some(node)) {
                return Ws::None;
            }
            return if self.config.spacing.space_around_binary_ops {
                Ws::Space
            } else {
                Ws::None
            };
        }
        if is_binary_op_kind(prev_kind) {
            return if self.config.spacing.space_around_binary_ops {
                Ws::Space
            } else {
                Ws::None
            };
        }

        // Compound assignment.
        if is_compound_assign(kind) || is_compound_assign(prev_kind) {
            return Ws::Space;
        }

        // `=` in declaration init.
        if kind == "=" || prev_kind == "=" {
            return Ws::Space;
        }

        // Keywords before/after identifiers.
        if is_keyword_str(prev_text) {
            return Ws::Space;
        }
        if is_keyword_str(text) {
            return Ws::Space;
        }

        // After `}`.
        if prev_kind == "}" {
            return Ws::Space;
        }

        // `:` (ternary, bitfield, case) — but NOT the `:` introducing a
        // constructor member-initializer list (field_initializer_list).
        if kind == ":" || prev_kind == ":" {
            // No space before `:` that opens a member-initializer list.
            if kind == ":" {
                let no_space_before = node
                    .parent()
                    .map(|p| {
                        matches!(
                            p.kind(),
                            "field_initializer_list" | "base_class_clause" | "for_range_loop"
                        )
                    })
                    .unwrap_or(false);
                if no_space_before {
                    return Ws::None;
                }
            }
            // After `:`, no space before a unary operator or negative literal (funky: `:-1`).
            if prev_kind == ":"
                && (is_unary_prefix(kind, Some(node))
                    || (kind == "number_literal" && text.starts_with('-')))
            {
                return Ws::None;
            }
            return Ws::Space;
        }

        // `?` in ternary.
        if kind == "?" || prev_kind == "?" {
            return Ws::Space;
        }

        // Adjacent string literals in a concatenated_string need a space: "foo" "bar".
        // The leaves inside string_literal nodes are `"`, string_content, escape_sequence, etc.
        // A space is needed between the closing `"` of one string_literal and the opening
        // `"` of the next, when they are children of different string_literal nodes that share
        // a common concatenated_string grandparent.
        {
            let node_parent = node.parent();
            let prev_parent = prev.and_then(|n| n.parent());
            if node_parent.map(|p| p.kind()) == Some("string_literal")
                && prev_parent.map(|p| p.kind()) == Some("string_literal")
            {
                if let (Some(np), Some(pp)) = (node_parent, prev_parent) {
                    if np.start_byte() != pp.start_byte() {
                        return Ws::Space;
                    }
                }
            }
        }

        // space_after_cast: `)` closing a cast_expression followed by the value.
        if prev_kind == ")" {
            let in_cast = prev
                .and_then(|n| n.parent())
                .map(|p| p.kind() == "cast_expression")
                .unwrap_or(false);
            if in_cast {
                return match self.config.spacing.space_after_cast {
                    SpaceOption::Add => Ws::Space,
                    SpaceOption::Remove => Ws::None,
                    SpaceOption::Preserve => {
                        let gap_start = prev.map(|n| n.end_byte()).unwrap_or(0);
                        if self.src.as_bytes().get(gap_start) == Some(&b' ') {
                            Ws::Space
                        } else {
                            Ws::None
                        }
                    }
                };
            }
        }

        // Identifier / type pairs.
        match (prev_kind, kind) {
            ("identifier", "identifier")
            | ("identifier", "type_identifier")
            | ("type_identifier", "identifier")
            | ("type_identifier", "type_identifier")
            | ("type_identifier", "field_identifier")
            | ("primitive_type", _)
            | (_, "primitive_type")
            | ("storage_class_specifier", _)
            | ("type_qualifier", _)
            | ("number_literal", "identifier")
            | ("identifier", "number_literal") => Ws::Space,
            _ => Ws::None,
        }
    }

    // ── var-decl-block state machine ──────────────────────────────────────────

    fn decl_block_enter(&mut self) {
        if self.config.newlines.blank_line_after_var_decl_block {
            self.decl_block_active = true;
            self.decl_block_saw_decl = false;
            self.decl_block_at_stmt_start = true;
        }
    }

    fn decl_block_exit(&mut self) {
        self.decl_block_active = false;
        self.decl_block_saw_decl = false;
        self.decl_block_at_stmt_start = false;
    }
}

// ── Free functions ────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum Ws {
    None,
    Space,
}

fn collect_leaves<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    if node.child_count() == 0 || node.kind() == "lambda_expression" {
        // Treat lambda_expression as an opaque leaf so emit_leaves can dispatch
        // it to emit_lambda_expression rather than flattening its body tokens.
        out.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_leaves(child, out);
    }
}

fn is_preproc(kind: &str) -> bool {
    matches!(
        kind,
        "preproc_include"
            | "preproc_def"
            | "preproc_function_def"
            | "preproc_if"
            | "preproc_ifdef"
            | "preproc_ifndef"
            | "preproc_else"
            | "preproc_elif"
            | "preproc_endif"
            | "preproc_call"
            | "preproc_defined"
            | "preproc_undef"
            | "preproc_pragma"
            | "preproc_error"
            | "preproc_warning"
            | "preproc_line"
    )
}

/// Returns true when an ERROR node represents a braceless `switch (cond)` —
/// i.e. tree-sitter couldn't attach a compound_statement body because the
/// source uses `switch (x) case N:` without braces.
fn is_braceless_switch_error(node: Node) -> bool {
    if node.kind() != "ERROR" {
        return false;
    }
    let mut cursor = node.walk();
    let mut has_switch = false;
    let mut has_paren_expr = false;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "switch" => has_switch = true,
            "parenthesized_expression" => has_paren_expr = true,
            _ => {}
        }
    }
    has_switch && has_paren_expr
}

fn is_null_stmt(node: Node) -> bool {
    if node.kind() == "expression_statement" {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        return children.len() == 1 && children[0].kind() == ";";
    }
    false
}

fn should_inject(node: Node) -> bool {
    match node.kind() {
        "compound_statement" => false,
        k if is_preproc(k) => false,
        _ => !is_null_stmt(node),
    }
}

/// Funky quirk: when a `case_statement` is the last body statement of a
/// BraceCtx::Other block (column-0 `{...}` directly after another `{`),
/// the case body's indent_level increment leaks into the closing `}`.
/// Returns true when the compound_statement's last content is a case.
fn compound_ends_with_case(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    let last = children
        .iter()
        .rev()
        .find(|n| !matches!(n.kind(), "{" | "}"));
    match last {
        None => false,
        Some(n) => match n.kind() {
            "case_statement" => true,
            "labeled_statement" => {
                let mut c2 = n.walk();
                n.children(&mut c2)
                    .last()
                    .is_some_and(|lc| lc.kind() == "case_statement")
            }
            _ => false,
        },
    }
}

fn is_binary_op_kind(kind: &str) -> bool {
    matches!(
        kind,
        "+" | "-"
            | "*"
            | "/"
            | "%"
            | "&"
            | "|"
            | "^"
            | "<<"
            | ">>"
            | "=="
            | "!="
            | "<"
            | ">"
            | "<="
            | ">="
            | "&&"
            | "||"
    )
}

fn is_compound_assign(kind: &str) -> bool {
    matches!(
        kind,
        "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="
    )
}

fn is_unary_prefix(kind: &str, node: Option<Node<'_>>) -> bool {
    if !matches!(kind, "!" | "~" | "+" | "-" | "*" | "&" | "++" | "--") {
        return false;
    }
    if let Some(n) = node {
        if let Some(parent) = n.parent() {
            if parent.kind() == "update_expression" {
                // Prefix if this node is first child.
                let mut cursor = parent.walk();
                let first_start = parent.children(&mut cursor).next().map(|c| c.start_byte());
                drop(cursor);
                if let Some(fs) = first_start {
                    return fs == n.start_byte();
                }
            }
            return matches!(parent.kind(), "unary_expression" | "pointer_expression");
        }
    }
    false
}

fn last_leaf_of(mut node: Node<'_>) -> Node<'_> {
    loop {
        let count = node.child_count();
        if count == 0 {
            return node;
        }
        node = node.child(count - 1).unwrap();
    }
}

fn is_suffix(node: Node<'_>) -> bool {
    if let Some(parent) = node.parent() {
        if parent.kind() == "update_expression" {
            let mut cursor = parent.walk();
            let children: Vec<Node> = parent.children(&mut cursor).collect();
            if let Some(&last) = children.last() {
                return last.start_byte() == node.start_byte();
            }
        }
    }
    false
}

fn is_keyword_str(text: &str) -> bool {
    matches!(
        text,
        "if" | "else"
            | "for"
            | "while"
            | "do"
            | "switch"
            | "case"
            | "default"
            | "return"
            | "break"
            | "continue"
            | "goto"
            | "typedef"
            | "struct"
            | "union"
            | "enum"
            | "sizeof"
            | "typeof"
            | "alignof"
            | "offsetof"
            | "static"
            | "extern"
            | "auto"
            | "register"
            | "inline"
            | "volatile"
            | "const"
            | "restrict"
            | "unsigned"
            | "signed"
            | "void"
            | "char"
            | "short"
            | "int"
            | "long"
            | "float"
            | "double"
            | "_Bool"
            | "namespace"
            | "class"
            | "template"
            | "typename"
            | "new"
            | "delete"
            | "public"
            | "private"
            | "protected"
            | "virtual"
            | "override"
            | "explicit"
            | "friend"
            | "operator"
            | "this"
            | "using"
            | "nullptr"
            | "true"
            | "false"
            | "constexpr"
            | "noexcept"
            | "static_assert"
            | "decltype"
            | "throw"
            | "catch"
            | "try"
    )
}

fn is_declaration_node(node: Node) -> bool {
    matches!(node.kind(), "declaration" | "type_definition")
}

/// Returns true when a `function_declarator` node's first meaningful child
/// is a `parenthesized_declarator` (i.e. starts with `(`).
fn fn_declarator_has_paren_first_child(fn_decl: Node) -> bool {
    let mut cur = fn_decl.walk();
    let children: Vec<Node> = fn_decl.children(&mut cur).collect();
    children
        .iter()
        .any(|n| n.kind() == "parenthesized_declarator")
}

/// Returns true when a `function_declarator` node's first child is a
/// `parenthesized_declarator` that holds `(*name)` or `(&name)` — i.e.,
/// a simple function-pointer/reference declarator where the name is directly
/// inside the parens with no further parameter list.  This mirrors funky's
/// `next_is_fn_ptr_declarator()` which requires `*`/`&` → Ident → `)`.
///
/// When false, no space should be emitted before the `(` of the declarator
/// (e.g. `int(fp)()`, `int(*f(params))(...)`).
fn is_simple_fn_ptr_declarator(fn_decl: Node) -> bool {
    let mut cur = fn_decl.walk();
    let paren_decl = match fn_decl
        .children(&mut cur)
        .find(|n| n.kind() == "parenthesized_declarator")
    {
        Some(n) => n,
        None => return false,
    };
    let mut inner_cur = paren_decl.walk();
    let inner: Vec<Node> = paren_decl.children(&mut inner_cur).collect();
    // inner should be: `(`, pointer_declarator, `)`
    let ptr_decl = match inner.iter().find(|n| n.kind() == "pointer_declarator") {
        Some(n) => *n,
        None => return false,
    };
    // pointer_declarator children: `*`/`&`, then exactly an identifier (not function_declarator)
    let mut ptr_cur = ptr_decl.walk();
    let ptr_children: Vec<Node> = ptr_decl.children(&mut ptr_cur).collect();
    // Must have exactly 2 children: `*` (or `&`) and `identifier`/`type_identifier`
    let non_star: Vec<&Node> = ptr_children
        .iter()
        .filter(|n| !matches!(n.kind(), "*" | "&"))
        .collect();
    non_star.len() == 1
        && matches!(
            non_star[0].kind(),
            "identifier" | "type_identifier" | "field_identifier"
        )
}

// ── Post-processing: trailing comment alignment ───────────────────────────────

/// Return the byte offset of the `/*` or `//` that starts a TRAILING comment
/// on `line` (i.e., code appears before the comment), or `None` otherwise.
fn trailing_comment_col(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let n = bytes.len();
    let mut in_str = false;
    let mut str_ch = b'"';
    let mut code_seen = false;
    let mut i = 0;
    while i < n {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == str_ch {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_str = true;
            str_ch = b;
            code_seen = true;
            i += 1;
            continue;
        }
        if i + 1 < n && b == b'/' && (bytes[i + 1] == b'*' || bytes[i + 1] == b'/') {
            // Only a trailing comment if code appeared before this.
            if code_seen {
                return Some(i);
            }
            return None;
        }
        if b != b' ' && b != b'\t' {
            code_seen = true;
        }
        i += 1;
    }
    None
}

fn round_up_to_multiple(n: usize, m: usize) -> usize {
    if m == 0 {
        return n;
    }
    n.div_ceil(m) * m
}

/// Post-processing pass: align trailing comments within groups of consecutive
/// commented lines. Replicates funky's `align_trailing_comments` semantics.
fn align_trailing_comments(
    output: &str,
    min_gap: usize,
    normalize_single: bool,
    on_tabstop: bool,
    tab_width: usize,
    span: usize,
) -> String {
    let lines: Vec<&str> = output.split('\n').collect();
    let n = lines.len();
    let cols: Vec<Option<usize>> = lines.iter().map(|l| trailing_comment_col(l)).collect();
    let mut result: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    let mut i = 0;
    while i < n {
        if cols[i].is_some() {
            // Extend the group as long as the next commented line is within
            // `span` non-commented lines of the last commented line found.
            let mut last_cmt = i;
            let mut scan = i + 1;
            loop {
                let next = (scan..n).find(|&k| cols[k].is_some());
                match next {
                    Some(k) if k - last_cmt < span => {
                        last_cmt = k;
                        scan = k + 1;
                    }
                    _ => break,
                }
            }
            let j = last_cmt + 1;

            let commented_in_group = (i..j).filter(|&k| cols[k].is_some()).count();
            let is_single = commented_in_group == 1;
            if is_single && !normalize_single {
                i = j;
                continue;
            }

            // Use byte-length of the code portion (before comment), matching funky.
            let max_code_len = (i..j)
                .filter(|&k| cols[k].is_some())
                .map(|k| {
                    let col = cols[k].unwrap();
                    lines[k][..col].trim_end().len() // byte len
                })
                .max()
                .unwrap_or(0);

            let raw_target = max_code_len + min_gap;
            let target = if on_tabstop && tab_width > 0 {
                round_up_to_multiple(raw_target, tab_width)
            } else {
                raw_target
            };

            for k in i..j {
                if let Some(col) = cols[k] {
                    let code = lines[k][..col].trim_end();
                    let comment = lines[k][col..].trim_start_matches(' ');
                    let pad = target.saturating_sub(code.len()).max(1);
                    result[k] = format!("{}{}{}", code, " ".repeat(pad), comment);
                }
            }

            i = j;
        } else {
            i += 1;
        }
    }

    result.join("\n")
}

/// Byte offset of the value-assigning `=` in an enum-member line, or `None`
/// if the line doesn't look like `NAME = value,`/`NAME = value` (last member).
/// Ported from funky's `enum_eq_col`.
fn enum_eq_col(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        return None;
    }
    // Enum members end with `,` (non-last) or with an alphanumeric/`_`/`)` (last
    // member). Reject anything that ends with `;`, `{`, `}`, etc. to avoid
    // false-positives on declarations or initializer lines.
    let last = trimmed.trim_end().chars().last().unwrap_or(' ');
    if !matches!(last, ',' | ')') && !last.is_alphanumeric() && last != '_' {
        return None;
    }
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut in_char = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_string || in_char => {
                i += 2;
                continue;
            }
            b'"' if !in_char => {
                in_string = !in_string;
            }
            b'\'' if !in_string => {
                in_char = !in_char;
            }
            b'=' if !in_string && !in_char => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    i += 2;
                    continue;
                }
                if i > 0
                    && matches!(
                        bytes[i - 1],
                        b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^'
                    )
                {
                    i += 1;
                    continue;
                }
                return Some(i);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// True when `line` is a comment-only line that should not break an enum
/// alignment group.
fn is_enum_comment_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//") || (trimmed.starts_with("/*") && trimmed.trim_end().ends_with("*/"))
}

/// True when `line` looks like a bare enum member with no explicit value
/// (e.g. `    RED,`). Bare members act as transparent connectors within an
/// alignment group so `RED, GREEN = 5, BLUE, YELLOW = 10` all align together.
fn is_bare_enum_member(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        return false;
    }
    if !trimmed.contains(',') {
        return false;
    }
    !trimmed.contains('=')
}

/// Post-processing pass: align `=` signs within groups of consecutive enum
/// value lines. Replicates funky's `align_enum_equals` semantics.
fn align_enum_equals(output: &str, on_tabstop: bool, tab_width: usize) -> String {
    let lines: Vec<&str> = output.split('\n').collect();
    let n = lines.len();
    let cols: Vec<Option<usize>> = lines.iter().map(|l| enum_eq_col(l)).collect();
    let mut result: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    let mut i = 0;
    while i < n {
        if cols[i].is_some() {
            // Extend the group through bare members, blank lines, preprocessor
            // directives, and comment lines — all transparent connectors.
            let mut j = i + 1;
            while j < n
                && (cols[j].is_some()
                    || is_bare_enum_member(lines[j])
                    || lines[j].trim().is_empty()
                    || lines[j].trim_start().starts_with('#')
                    || is_enum_comment_line(lines[j]))
            {
                j += 1;
            }
            // Trim trailing blank/bare lines so they don't become orphaned group members.
            while j > i + 1 && cols[j - 1].is_none() {
                j -= 1;
            }
            let eq_indices: Vec<usize> = (i..j).filter(|&k| cols[k].is_some()).collect();
            if eq_indices.len() > 1 {
                let max_name_len = eq_indices
                    .iter()
                    .map(|&k| lines[k][..cols[k].unwrap()].trim_end().len())
                    .max()
                    .unwrap();
                let raw_target = max_name_len + 1;
                let target = if on_tabstop && tab_width > 0 {
                    round_up_to_multiple(raw_target, tab_width)
                } else {
                    raw_target
                };
                for k in eq_indices {
                    let col = cols[k].unwrap();
                    let name = lines[k][..col].trim_end();
                    let rest = &lines[k][col..];
                    let pad = target - name.len();
                    result[k] = format!("{}{}{}", name, " ".repeat(pad), rest);
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }

    result.join("\n")
}
