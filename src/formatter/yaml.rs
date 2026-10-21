// YAML formatter (`.yaml`, `.yml`): the structural target is Prettier's
// YAML output at its defaults (2-space indent, width 80, proseWrap
// preserve). Block mappings and sequences are re-indented, with a sequence
// under a mapping key indented one level; `key: value` and `- item` take
// one space; comments take one space before `#`; one blank line between
// entries is kept.
//
// Flow collections are rendered `[a, b]` / `{ a: 1 }`. One that does not fit
// after its key moves to the next line, and one that still does not fit is
// broken one item per line with a trailing comma, as Prettier does.
//
// Quoted scalars prefer double quotes. A single-quoted scalar becomes
// double-quoted when its value has no backslash and no `"`;
// a double-quoted one becomes single-quoted when its value has more `"`
// than `'` and no escape other than `\"`. Other scalars are emitted
// verbatim. Block scalar content is re-indented one level under its key
// (or, with an explicit indentation indicator, shifted with its parent);
// continuation lines of a multi-line flow scalar are re-indented the same
// way.
//
// Known gaps (documented rather than half-implemented):
//   - A file with any syntax error is returned unchanged.
//   - Explicit `? key` entries and flow collections that contain comments
//     or multi-line scalars are emitted verbatim (re-indented with their
//     parent) rather than normalized.
//   - Long plain and quoted scalars are never re-wrapped.

use super::output::OutputOps;
use crate::config::Config;
use crate::error::MoldyError;
use tree_sitter::Node;

pub fn format(source: &str, config: &Config) -> Result<String, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key("yaml")
        .ok_or_else(|| MoldyError::UnsupportedLanguage("yaml".to_string()))?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;

    super::check_block_depth("YAML", source, open_blocks)?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))?;

    let root = tree.root_node();
    if root.has_error() {
        return Err(super::syntax_error("YAML", root));
    }
    super::check_depth("YAML", root)?;

    let mut fmt = Fmt {
        src: source,
        config,
        out: String::new(),
        last_row: None,
        line_comment: false,
        blocks: Vec::new(),
        scalar_end: 0,
        markers: Vec::new(),
    };
    fmt.emit_stream(root);
    let printed = std::mem::take(&mut fmt.markers);
    // `finish` trims trailing whitespace, which a block scalar at the end
    // of the file can own, and adds a final newline, which would become
    // part of its value.
    let content_end = fmt.out.trim_end_matches(['\n', '\r', ' ', '\t']).len();
    let out = if fmt.scalar_end > 0 && fmt.scalar_end >= content_end {
        let mut out = fmt.out[..fmt.scalar_end].to_string();
        if source.ends_with('\n') && !out.ends_with('\n') {
            out.push('\n');
        }
        out
    } else {
        fmt.finish()
    };
    // Text starting with `---` or `...` moved to column 0 (an indented
    // top-level scalar) would start or end a document, so every marker at
    // column 0 of the output must have been printed from a marker node.
    let mut offset = 0;
    let stray = out.split_inclusive('\n').any(|l| {
        let start = offset;
        offset += l.len();
        is_marker(l.trim_end_matches('\n')) && printed.binary_search(&start).is_err()
    });
    if stray {
        let line = source
            .lines()
            .position(|l| l.starts_with([' ', '\t']) && is_marker(l.trim_ascii_start()))
            .map_or(1, |i| i + 1);
        return Err(MoldyError::Unverified {
            language: "YAML".to_string(),
            line,
        });
    }
    Ok(out)
}

/// An upper bound on the block collections open on `line`: a mapping and
/// a sequence can share a column, so two for each column before its
/// content, after any `- `, `? ` or `: ` indicators. A blank line opens
/// none.
fn open_blocks(line: &str) -> usize {
    if line.trim_ascii().is_empty() {
        return 0;
    }
    let bytes = line.as_bytes();
    let blank = |i: usize| bytes.get(i).is_none_or(|b| matches!(b, b' ' | b'\t'));
    let column = (0..=bytes.len())
        .find(|&i| match bytes.get(i) {
            Some(b' ') => false,
            Some(b'-' | b'?' | b':') => !blank(i + 1),
            _ => true,
        })
        .unwrap_or(0);
    2 * (column + 1)
}

/// Whether `line` is a document marker (`---` or `...`, alone or followed
/// by a blank).
fn is_marker(line: &str) -> bool {
    ["---", "..."].iter().any(|m| {
        line.strip_prefix(m)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t', '\r']))
    })
}

/// Where a value starts, relative to the node that owns it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    /// At the start of its own line (a document's root value).
    Line,
    /// After `key:`.
    AfterKey,
    /// After a sequence item's `-`.
    AfterDash,
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// Last source row holding `node`'s text (a node can end at column 0 of
/// the row after its last line).
fn last_row(node: Node) -> usize {
    let end = node.end_position();
    if end.column == 0 && end.row > node.start_position().row {
        end.row - 1
    } else {
        end.row
    }
}

fn is_property(kind: &str) -> bool {
    matches!(kind, "anchor" | "tag")
}

fn is_directive(kind: &str) -> bool {
    matches!(
        kind,
        "yaml_directive" | "tag_directive" | "reserved_directive"
    )
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn shifted(col: usize, shift: isize) -> usize {
    (col as isize + shift).max(0) as usize
}

/// Prettier's quote preference: double quotes unless the value has more
/// `"` than `'`. A conversion that would need new escapes is skipped.
fn normalize_quotes(kind: &str, text: &str) -> String {
    let quote = match kind {
        "single_quote_scalar" => '\'',
        "double_quote_scalar" => '"',
        _ => return text.to_string(),
    };
    let Some(inner) = text.strip_prefix(quote).and_then(|t| t.strip_suffix(quote)) else {
        return text.to_string();
    };
    match kind {
        "single_quote_scalar" => {
            let value = inner.replace("''", "'");
            if value.contains(['\\', '"']) {
                text.to_string()
            } else {
                format!("\"{value}\"")
            }
        }
        "double_quote_scalar" => {
            if inner.replace("\\\"", "").contains('\\') {
                return text.to_string();
            }
            let value = inner.replace("\\\"", "\"");
            if value.matches('"').count() > value.matches('\'').count() {
                format!("'{}'", value.replace('\'', "''"))
            } else {
                text.to_string()
            }
        }
        _ => text.to_string(),
    }
}

struct Fmt<'a> {
    src: &'a str,
    config: &'a Config,
    out: String,
    /// Last source row emitted, for trailing-comment and blank-line checks.
    last_row: Option<usize>,
    /// The current output line ends in a comment.
    line_comment: bool,
    /// (source column, output column) of each block mapping or sequence
    /// being emitted, outermost first.
    blocks: Vec<(usize, usize)>,
    /// Output length after the last block scalar, whose trailing spaces
    /// and kept blank lines are part of its value.
    scalar_end: usize,
    /// Output offsets of the document markers printed from marker nodes.
    markers: Vec<usize>,
}

impl<'a> OutputOps<'a> for Fmt<'a> {
    fn src(&self) -> &'a str {
        self.src
    }

    fn config(&self) -> &Config {
        self.config
    }

    fn depth(&self) -> u32 {
        0
    }

    fn out(&self) -> &str {
        &self.out
    }

    fn out_mut(&mut self) -> &mut String {
        &mut self.out
    }
}

impl<'a> Fmt<'a> {
    fn width(&self) -> usize {
        self.config.yaml.indent_width as usize
    }

    fn max_width(&self) -> usize {
        self.config.yaml.max_width as usize
    }

    fn token(&mut self, node: Node, text: &str) {
        self.raw(text);
        self.last_row = Some(last_row(node));
    }

    /// One space before the next token, unless the line is empty so far.
    fn sep(&mut self) {
        if !self.at_bol() && !self.out.ends_with(' ') {
            self.raw(" ");
        }
    }

    fn newline_at(&mut self, col: usize) {
        self.ensure_nl();
        self.line_comment = false;
        self.raw(&" ".repeat(col));
    }

    /// Starts `node`'s line at `col`, keeping one blank line when the source
    /// had any between the previous token and `node` (and `blank_ok`).
    fn start_line(&mut self, node: Node, col: usize, blank_ok: bool) {
        if !self.out.is_empty() {
            self.ensure_nl();
            let gap = self
                .last_row
                .is_some_and(|r| node.start_position().row > r + 1);
            if blank_ok && gap && !self.out.ends_with("\n\n") {
                self.nl();
            }
        }
        self.newline_at(col);
    }

    fn comment(&mut self, node: Node, col: usize) {
        let text = self.node_text(node).trim_ascii_end();
        if !self.at_bol() && self.last_row == Some(node.start_position().row) {
            self.raw(" ");
        } else {
            // A comment written left of the current block belongs to the
            // outermost enclosing block that starts at or right of it.
            let src_col = node.start_position().column;
            let col = match self.blocks.last() {
                Some(&(block_src, _)) if src_col < block_src => self
                    .blocks
                    .iter()
                    .find(|(s, _)| *s >= src_col)
                    .map_or(col, |&(_, out)| out),
                _ => col,
            };
            self.start_line(node, col, true);
        }
        self.token(node, text);
        self.line_comment = true;
    }

    /// Emits `node`'s source with continuation lines shifted so that the
    /// node's first line moves to `col`.
    fn verbatim(&mut self, node: Node, col: usize) {
        let shift = col as isize - node.start_position().column as isize;
        let text = self.node_text(node);
        for (i, line) in text.split('\n').enumerate() {
            let line = line.trim_ascii_end();
            if i == 0 {
                self.raw(line);
            } else if line.is_empty() {
                self.ensure_nl();
                self.nl();
            } else {
                let indent = leading_spaces(line);
                self.newline_at(shifted(indent, shift));
                self.raw(&line[indent..]);
            }
        }
        self.last_row = Some(last_row(node));
    }

    fn emit_stream(&mut self, root: Node) {
        let kids = children(root);
        for (i, kid) in kids.iter().enumerate() {
            match kid.kind() {
                "comment" => self.comment(*kid, 0),
                "document" => {
                    // A `...` stays before directives, and becomes `---`
                    // before a document without its own, which would
                    // otherwise join this one (as Prettier prints it).
                    // Before a document of only `...`, it is dropped: that
                    // document's `...` decides.
                    let end = kids[i + 1..]
                        .iter()
                        .find(|n| n.kind() == "document")
                        .and_then(|d| {
                            let kids = children(*d);
                            if kids.iter().any(|c| is_directive(c.kind())) {
                                Some("...")
                            } else if kids.iter().all(|c| matches!(c.kind(), "..." | "comment")) {
                                None
                            } else if !kids.iter().any(|c| c.kind() == "---") {
                                Some("---")
                            } else {
                                None
                            }
                        });
                    self.emit_document(*kid, end);
                }
                _ => {
                    self.start_line(*kid, 0, true);
                    self.verbatim(*kid, 0);
                }
            }
        }
    }

    /// Emits a document. `end` is what its `...` is printed as, if it is
    /// kept.
    fn emit_document(&mut self, doc: Node, end: Option<&str>) {
        for kid in children(doc) {
            match kid.kind() {
                "comment" => self.comment(kid, 0),
                "---" | "..." => {
                    let text = if kid.kind() == "---" {
                        Some("---")
                    } else {
                        end
                    };
                    if let Some(text) = text {
                        self.start_line(kid, 0, true);
                        self.markers.push(self.out.len());
                        self.token(kid, text);
                    } else {
                        // A dropped `...` leaves no blank line behind. A
                        // comment after it goes on its own line, since the
                        // line before may end a block scalar.
                        if !self.out.is_empty() {
                            self.ensure_nl();
                        }
                        self.last_row = Some(last_row(kid));
                    }
                }
                "block_node" | "flow_node" => {
                    let scalar = children(kid).iter().any(|k| k.kind() == "block_scalar");
                    if self.last_row == Some(kid.start_position().row) && !self.at_bol() {
                        self.sep();
                        if scalar {
                            // `--- |`: the scalar keeps its content as is.
                            self.emit_value(kid, 0, 0, Ctx::Line);
                        } else {
                            self.verbatim(kid, 0);
                        }
                    } else {
                        self.start_line(kid, 0, true);
                        self.emit_value(kid, 0, 0, Ctx::Line);
                    }
                }
                _ => {
                    self.start_line(kid, 0, true);
                    let text = self.node_text(kid).trim_ascii_end();
                    self.token(kid, text);
                }
            }
        }
    }

    /// Emits a `block_node` or `flow_node`. `col` is the output column of
    /// the owner (the key, the `-`, or the value itself on its own line);
    /// `old_col` is the owner's source column.
    fn emit_value(&mut self, node: Node, col: usize, old_col: usize, ctx: Ctx) {
        let child_col = match ctx {
            Ctx::Line => col,
            Ctx::AfterKey => col + self.width(),
            Ctx::AfterDash => col + 2,
        };
        let mut has_props = false;
        for kid in children(node) {
            match kid.kind() {
                k if is_property(k) => {
                    self.sep();
                    let text = self.node_text(kid);
                    self.token(kid, text);
                    has_props = true;
                }
                "comment" => self.comment(kid, child_col),
                "block_mapping" | "block_sequence" => {
                    let inline = !has_props && !self.line_comment && ctx != Ctx::AfterKey;
                    self.emit_block(kid, child_col, inline);
                }
                "block_scalar" => self.emit_block_scalar(kid, col, old_col, child_col),
                _ => self.emit_flow_content(kid, ctx, child_col),
            }
        }
    }

    /// A block mapping or sequence whose entries start at `col`. With
    /// `first_inline`, the first entry continues the current line.
    fn emit_block(&mut self, node: Node, col: usize, first_inline: bool) {
        self.blocks.push((node.start_position().column, col));
        let mut first = true;
        for kid in children(node) {
            match kid.kind() {
                "comment" => self.comment(kid, col),
                "block_mapping_pair" | "block_sequence_item" => {
                    if first && first_inline && !self.line_comment {
                        self.sep();
                    } else {
                        // A first entry keeps a blank line only after a
                        // comment line.
                        let after_comment = self
                            .out
                            .rsplit('\n')
                            .find(|l| !l.trim_ascii().is_empty())
                            .is_some_and(|l| l.trim_ascii_start().starts_with('#'));
                        self.start_line(kid, col, !first || after_comment);
                    }
                    first = false;
                    if kid.kind() == "block_mapping_pair" {
                        self.emit_pair(kid, col);
                    } else {
                        self.emit_item(kid, col);
                    }
                }
                _ => {
                    self.start_line(kid, col, true);
                    self.verbatim(kid, col);
                }
            }
        }
        self.blocks.pop();
    }

    fn emit_pair(&mut self, pair: Node, col: usize) {
        let kids = children(pair);
        if kids.iter().any(|k| k.kind() == "?") {
            self.verbatim(pair, col);
            return;
        }
        let key = pair.child_by_field_name("key");
        let old_col = pair.start_position().column;
        for kid in kids {
            match kid.kind() {
                ":" => self.token(kid, ":"),
                "comment" => self.comment(kid, col + self.width()),
                _ if Some(kid) == key => match self.flow_single(kid) {
                    Some(text) => self.token(kid, &text),
                    None => self.verbatim(kid, col),
                },
                _ => self.emit_value(kid, col, old_col, Ctx::AfterKey),
            }
        }
    }

    fn emit_item(&mut self, item: Node, col: usize) {
        let old_col = item.start_position().column;
        for kid in children(item) {
            match kid.kind() {
                "-" => self.token(kid, "-"),
                "comment" => self.comment(kid, col + 2),
                _ => self.emit_value(kid, col, old_col, Ctx::AfterDash),
            }
        }
    }

    /// A block scalar's header stays on the owner's line; its content moves
    /// to `child_col`, or with an indentation indicator shifts by as much as
    /// its owner moved (`col` versus `old_col`).
    fn emit_block_scalar(&mut self, node: Node, col: usize, old_col: usize, child_col: usize) {
        let src = self.src;
        let start = node.start_byte();
        let mut end = node.end_byte();
        if src[..end].ends_with('\n') {
            end -= 1;
        }
        let line_end = |from: usize| src[from..].find('\n').map_or(src.len(), |i| from + i);
        let header = &src[start..line_end(start)];
        let keep = header.split('#').next().unwrap_or("").contains('+');
        let indicator = header.chars().any(|c| c.is_ascii_digit());
        // A scalar with no text line has only empty lines, whatever their
        // spaces. A tab after the indentation is text, including on the
        // whitespace lines after the node.
        let trailing = src[end..]
            .split('\n')
            .skip(1)
            .take_while(|l| l.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r')));
        let base = src[start..end]
            .split('\n')
            .skip(1)
            .chain(trailing)
            .filter(|l| !l.trim_end_matches('\r').trim_start_matches(' ').is_empty())
            .map(leading_spaces)
            .min()
            .unwrap_or(usize::MAX);
        // Spaces past the content indentation, and trailing spaces, are
        // part of the value. With an indentation indicator the content
        // indentation is not known here, so any spaces are shifted.
        let blank = |line: &str| {
            line.bytes().all(|b| b == b' ') && (line.is_empty() || !indicator && line.len() <= base)
        };
        // The node ends before trailing whitespace on its last line, and
        // before following lines of spaces, which are content when they
        // are longer than the indentation, or kept blank lines (`+`).
        if src[end..line_end(end)]
            .bytes()
            .all(|b| matches!(b, b' ' | b'\t' | b'\r'))
        {
            end = line_end(end);
        }
        let mut next = end;
        while next + 1 < src.len() {
            let line = &src[next + 1..line_end(next + 1)];
            let line = line.strip_suffix('\r').unwrap_or(line);
            if !line.bytes().all(|b| matches!(b, b' ' | b'\t')) {
                break;
            }
            next = line_end(next + 1);
            if keep || !blank(line) {
                end = next;
            }
        }
        let text = &src[start..end];
        let mut lines = text.split('\n');
        let header = lines.next().unwrap_or("").trim_ascii_end();
        // `>-   # note` keeps one space before its comment.
        let header = match header.find('#') {
            Some(i) => format!("{} {}", header[..i].trim_ascii_end(), &header[i..]),
            None => header.to_string(),
        };
        let content: Vec<&str> = lines.map(|l| l.trim_end_matches('\r')).collect();
        self.sep();
        self.raw(&header);
        // A scalar that is the whole document keeps its indentation:
        // moved to column 0 it would no longer parse.
        let shift = if child_col == 0 {
            0
        } else if indicator {
            col as isize - old_col as isize
        } else if base == usize::MAX {
            0
        } else {
            child_col as isize - base as isize
        };
        for line in &content {
            if blank(line) {
                self.ensure_nl();
                self.nl();
            } else {
                let indent = leading_spaces(line);
                self.newline_at(shifted(indent, shift));
                self.raw(&line[indent..]);
            }
        }
        self.scalar_end = self.out.len();
        self.last_row = Some(node.start_position().row + src[start..end].matches('\n').count());
    }

    fn emit_flow_content(&mut self, node: Node, ctx: Ctx, child_col: usize) {
        if self.line_comment {
            self.newline_at(child_col);
        }
        match node.kind() {
            "flow_sequence" | "flow_mapping" => {
                let Some(single) = self.flow_single(node) else {
                    self.sep();
                    self.verbatim(node, self.current_column());
                    return;
                };
                let len = single.chars().count();
                let here = self.current_column() + usize::from(!self.at_bol());
                if here + len <= self.max_width() {
                    self.sep();
                    self.token(node, &single);
                } else if ctx == Ctx::AfterKey {
                    self.newline_at(child_col);
                    if child_col + len <= self.max_width() {
                        self.token(node, &single);
                    } else {
                        self.flow_broken(node, child_col);
                    }
                } else {
                    self.sep();
                    self.flow_broken(node, child_col);
                }
            }
            "plain_scalar" | "single_quote_scalar" | "double_quote_scalar" => {
                self.sep();
                self.emit_scalar(node, child_col);
            }
            _ => {
                self.sep();
                self.verbatim(node, child_col);
            }
        }
    }

    /// A flow scalar; continuation lines of a multi-line one move to `col`.
    fn emit_scalar(&mut self, node: Node, col: usize) {
        let text = self.node_text(node);
        if !text.contains('\n') {
            let text = normalize_quotes(node.kind(), text);
            self.token(node, &text);
            return;
        }
        for (i, line) in text.split('\n').enumerate() {
            let line = line.trim_ascii();
            if i == 0 {
                self.raw(line);
            } else if line.is_empty() {
                self.ensure_nl();
                self.nl();
            } else {
                self.newline_at(col);
                self.raw(line);
            }
        }
        self.last_row = Some(last_row(node));
    }

    /// Single-line rendering of a flow node, or `None` when it holds a
    /// comment, a multi-line scalar or an explicit `?` key.
    fn flow_single(&self, node: Node) -> Option<String> {
        match node.kind() {
            "flow_sequence" | "flow_mapping" => {
                let seq = node.kind() == "flow_sequence";
                let mut items = Vec::new();
                for kid in children(node) {
                    match kid.kind() {
                        "[" | "]" | "{" | "}" | "," => {}
                        _ => items.push(self.flow_single(kid)?),
                    }
                }
                Some(match (seq, items.is_empty()) {
                    (true, true) => "[]".to_string(),
                    (false, true) => "{}".to_string(),
                    (true, false) => format!("[{}]", items.join(", ")),
                    (false, false) => format!("{{ {} }}", items.join(", ")),
                })
            }
            "flow_pair" => {
                let key = node.child_by_field_name("key");
                let value = node.child_by_field_name("value");
                let mut text = String::new();
                for kid in children(node) {
                    match kid.kind() {
                        ":" => text.push(':'),
                        _ if Some(kid) == key => text.push_str(&self.flow_single(kid)?),
                        _ if Some(kid) == value => {
                            text.push(' ');
                            text.push_str(&self.flow_single(kid)?);
                        }
                        _ => return None,
                    }
                }
                Some(text)
            }
            "flow_node" => {
                let parts: Option<Vec<String>> = children(node)
                    .iter()
                    .map(|k| self.flow_single(*k))
                    .collect();
                Some(parts?.join(" "))
            }
            "plain_scalar" | "single_quote_scalar" | "double_quote_scalar" => {
                let text = self.node_text(node);
                (!text.contains('\n')).then(|| normalize_quotes(node.kind(), text))
            }
            "alias" | "anchor" | "tag" => Some(self.node_text(node).to_string()),
            _ => None,
        }
    }

    /// One item per line at `col` + indent, with a trailing comma; the
    /// brackets sit at `col`. An item that is itself too long and a plain
    /// collection is broken the same way.
    fn flow_broken(&mut self, node: Node, col: usize) {
        let seq = node.kind() == "flow_sequence";
        self.raw(if seq { "[" } else { "{" });
        let item_col = col + self.width();
        for kid in children(node) {
            if matches!(kid.kind(), "[" | "]" | "{" | "}" | ",") {
                continue;
            }
            self.newline_at(item_col);
            let single = self.flow_single(kid).unwrap_or_default();
            let inner = (kid.kind() == "flow_node" && kid.named_child_count() == 1)
                .then(|| kid.named_child(0))
                .flatten()
                .filter(|c| matches!(c.kind(), "flow_sequence" | "flow_mapping"));
            match inner {
                Some(inner) if item_col + single.chars().count() + 1 > self.max_width() => {
                    self.flow_broken(inner, item_col);
                }
                _ => self.raw(&single),
            }
            self.raw(",");
        }
        self.newline_at(col);
        self.token(node, if seq { "]" } else { "}" });
    }
}
