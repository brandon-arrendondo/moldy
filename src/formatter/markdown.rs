// Markdown formatter (`.md`, `.markdown`): the structural target is
// Prettier's Markdown output with `proseWrap: preserve`, so paragraph line
// breaks stay where the author put them.
//
// The block grammar gives the document structure; each block is rendered
// to lines, and containers (list items, block quotes) prefix their
// children's lines. Blocks are separated by one blank line. Headings become
// ATX (`#`) headings, thematic breaks become `---`, code fences use
// backticks, tables are aligned by display width, and front matter is
// formatted as YAML.
//
// Lists follow Prettier: adjacent lists alternate `-`/`*` (or `.`/`)`),
// ordered lists are renumbered from their first number (or keep `1.` on
// every item when the second item is numbered 1), content is indented by
// the marker's width, and an item is followed by a blank line only when it
// is loose.
//
// Paragraph, heading and table-cell text is re-parsed with the
// `markdown_inline` grammar: runs of spaces collapse to one, `*emphasis*`
// becomes `_emphasis_` (unless it touches a word character or sits inside
// other emphasis), and `__strong__` becomes `**strong**`. Code spans,
// inline HTML, autolinks, link destinations and escapes are verbatim.
//
// The grammar reads some input differently from CommonMark and GFM.
// Emphasis is paired with CommonMark's own algorithm
// (`markdown_emphasis.rs`); blocks the grammar starts where CommonMark
// continues a paragraph are kept as written; and input the formatter
// cannot render faithfully (`Md::misread`) is refused. The output is then
// parsed again and its shape (`Md::shape`) compared with the input's, and
// both are read with an independent CommonMark parser
// (`markdown_check.rs`); on any difference the file is refused with
// `MoldyError::Unverified`.
//
// Known gaps (documented rather than half-implemented):
//   - A file with any syntax error is returned unchanged.
//   - Escapes are never added or removed, link titles keep their quotes,
//     and a multi-line setext heading stays setext.
//   - A paragraph line that would start a block once unindented keeps its
//     original indentation.

use super::markdown_emphasis as emphasis;
use crate::config::Config;
use crate::error::MoldyError;
use lang_parsing_substrate::markdown::ContentMap;
use std::collections::HashSet;
use tree_sitter::{Node, Parser};
use unicode_width::UnicodeWidthStr;

pub fn format(source: &str, config: &Config) -> Result<String, MoldyError> {
    super::check_block_depth("Markdown", source, open_blocks)?;
    let mut cost = 0;
    if let Some(row) = source.lines().position(|l| {
        cost += line_cost(l);
        cost > MAX_LINE_COST
    }) {
        return Err(super::too_deep("Markdown", row));
    }
    let tree = parse(source, "markdown")?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(super::syntax_error("Markdown", root));
    }
    super::check_depth("Markdown", root)?;
    if let Some(row) = costly_inline(source, root) {
        return Err(super::too_deep("Markdown", row));
    }

    let md = Md::new(source, root, config);
    if let Some(row) = md.misread(root) {
        return Err(unverified(row));
    }
    let lines = md.blocks(root, false);
    let mut out = lines.join("\n");
    let trimmed = out.trim_end_matches(['\n', ' ']).len();
    out.truncate(trimmed);
    if config.newlines.final_newline && !out.is_empty() {
        out.push('\n');
    }
    // The grammar misreads some unusual input (see `markdown_emphasis.rs`
    // and the lazy-list and code-block handling); check the result.
    // Front matter is formatted as YAML, so the rendering check skips it.
    let front = Md::block_children(root)
        .first()
        .is_some_and(|b| b.kind() == "minus_metadata");
    let body = |text: &str| {
        if front {
            without_front_matter(text)
        } else {
            text.to_string()
        }
    };
    let (before, after) = (body(source), body(&out));
    if let Some(offset) = super::markdown_check::first_difference(&before, &after) {
        let skipped = source.len() - before.len();
        return Err(unverified(md.row_of(skipped + offset)));
    }
    if block_shape(&out) != Some(md.shape(root)) {
        return Err(unverified(md.first_shape_difference(root, &out)));
    }
    Ok(out)
}

/// An upper bound on the blocks open on `line`: one for each block quote
/// and list marker in its prefix, one for each list level its indentation
/// could hold, and one leaf block.
fn open_blocks(line: &str) -> usize {
    let bytes = line.as_bytes();
    let blank = |i: usize| bytes.get(i).is_none_or(|b| matches!(b, b' ' | b'\t'));
    let (mut i, mut markers, mut columns) = (0, 0, 0);
    while i < bytes.len() {
        match bytes[i] {
            b' ' => columns += 1,
            b'\t' => columns += 4,
            b'>' => markers += 1,
            b'-' | b'+' | b'*' if blank(i + 1) => markers += 1,
            b'0'..=b'9' => {
                let end = i + bytes[i..].iter().take_while(|b| b.is_ascii_digit()).count();
                if !matches!(bytes.get(end), Some(b'.' | b')')) || !blank(end + 1) {
                    break;
                }
                markers += 1;
                i = end;
            }
            _ => break,
        }
        i += 1;
    }
    markers + columns / 2 + 1
}

fn unverified(row: usize) -> MoldyError {
    MoldyError::Unverified {
        language: "Markdown".into(),
        line: row + 1,
    }
}

/// `text` after its leading `---` ... `---` block.
fn without_front_matter(text: &str) -> String {
    let rest: Vec<&str> = text
        .split_inclusive('\n')
        .skip(1)
        .skip_while(|row| row.trim_ascii_end() != "---")
        .skip(1)
        .collect();
    rest.concat()
}

/// `source`'s document structure as compared by `Md::shape`, or `None` if
/// it does not parse cleanly.
fn block_shape(source: &str) -> Option<String> {
    let tree = parse(source, "markdown").ok()?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    Some(Md::new(source, root, &Config::default()).shape(root))
}

/// Inline text's structure (see `inline_shape`), or the text without
/// whitespace if the inline grammar cannot parse it.
fn text_shape(text: &str) -> String {
    let parsed = (!too_deep_inline(text))
        .then(|| parse(text, "markdown_inline"))
        .and_then(Result::ok);
    match parsed {
        Some(tree)
            if !tree.root_node().has_error()
                && super::check_depth("Markdown", tree.root_node()).is_ok() =>
        {
            let root = tree.root_node();
            let (marks, chars) = inline_shape(text, root, &opaque_ranges(root));
            format!("{marks}|{chars}")
        }
        _ => text.chars().filter(|c| !c.is_ascii_whitespace()).collect(),
    }
}

/// The largest sum of squared `*` / `_` run lengths in inline text, in
/// one text or in all a file's parsed texts, and the most backtick runs in
/// inline text, that are parsed. The inline grammar rescans a delimiter
/// run for each delimiter in it and looks ahead from each backtick run for
/// its match, so both grow with the square. A file whose parsed texts pass
/// the first in all, or the square of the second, is refused before its
/// inline text is parsed.
const MAX_RUN_COST: usize = 1_000_000;
const MAX_BACKTICK_RUNS: usize = 4_000;

/// The largest `line_cost` of a file. A file over it is refused before
/// parsing.
const MAX_LINE_COST: usize = 10_000_000;

/// The sum of squared numbers of `*` or `_` in each stretch of `line`
/// made of one of them and blanks, and of squared lengths of `` ` `` and
/// `~` runs. The block grammar's time on a line, in code blocks too, grows
/// with these squares: from each delimiter it looks ahead for a thematic
/// break, and from each fence character for a closing fence.
fn line_cost(line: &str) -> usize {
    let (mut cost, mut mark, mut count) = (0, 0, 0);
    for b in line.bytes() {
        if count > 0 && b == mark {
            count += 1;
        } else if !(matches!(b, b' ' | b'\t') && matches!(mark, b'*' | b'_')) {
            cost += count * count;
            (mark, count) = if matches!(b, b'*' | b'_' | b'`' | b'~') {
                (b, 1)
            } else {
                (0, 0)
            };
        }
    }
    cost + count * count
}

/// The source row where the inline text that will be parsed passes
/// `MAX_RUN_COST`, or the square of `MAX_BACKTICK_RUNS` in squared
/// backtick runs, in all, if it does. Text that is kept as written is not
/// parsed and does not count.
fn costly_inline(source: &str, root: Node) -> Option<usize> {
    let (mut cost, mut ticks) = (0, 0);
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "inline" | "pipe_table_cell") {
            let text = &source[node.byte_range()];
            if !too_deep_inline(text) {
                let (runs, backticks) = run_cost(text);
                cost += runs;
                ticks += backticks * backticks;
                if cost > MAX_RUN_COST || ticks > MAX_BACKTICK_RUNS * MAX_BACKTICK_RUNS {
                    return Some(node.start_position().row);
                }
            }
        } else {
            stack.extend(children(node).into_iter().rev());
        }
    }
    None
}

/// The sum of squared `*` / `_` run lengths in `text`, and its number of
/// backtick runs.
fn run_cost(text: &str) -> (usize, usize) {
    let bytes = text.as_bytes();
    let (mut cost, mut ticks, mut i) = (0, 0, 0);
    while i < bytes.len() {
        let b = bytes[i];
        let run = bytes[i..].iter().take_while(|&&c| c == b).count();
        match b {
            b'*' | b'_' => cost += run * run,
            b'`' => ticks += 1,
            _ => {}
        }
        i += run;
    }
    (cost, ticks)
}

/// Whether inline text is too costly to parse: emphasis that may nest
/// too deeply, very long delimiter runs, or very many backticks. The text
/// is then kept as written. Checked before parsing, whose time grows
/// faster than the text in these cases. The tree is checked again after
/// parsing, since the inline writer recurses.
fn too_deep_inline(text: &str) -> bool {
    let (cost, ticks) = run_cost(text);
    cost > MAX_RUN_COST
        || ticks > MAX_BACKTICK_RUNS
        || emphasis::nesting_bound(text) > super::MAX_BLOCK_DEPTH
}

/// Lines with their common indentation and trailing whitespace removed,
/// and leading and trailing blank lines dropped.
fn dedent(lines: &[String]) -> String {
    let indent = lines
        .iter()
        .filter(|l| !l.trim_ascii().is_empty())
        .map(|l| l.len() - l.trim_ascii_start().len())
        .min()
        .unwrap_or(0);
    let body: Vec<&str> = lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or("").trim_ascii_end())
        .collect();
    body.join("\n").trim_matches('\n').to_string()
}

fn parse(source: &str, key: &str) -> Result<tree_sitter::Tree, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key(key)
        .ok_or_else(|| MoldyError::UnsupportedLanguage(key.to_string()))?;
    let mut parser = Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;
    parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// Container syntax rather than content: prefixes and markers.
fn is_container_token(kind: &str) -> bool {
    kind == "block_continuation"
        || kind == "block_quote_marker"
        || kind.starts_with("list_marker_")
        || kind.starts_with("task_list_marker_")
}

fn is_ordered(list: Node) -> bool {
    list.named_child(0)
        .and_then(|item| item.named_child(0))
        .is_some_and(|m| matches!(m.kind(), "list_marker_dot" | "list_marker_parenthesis"))
}

fn trim_blank_tail(lines: &mut Vec<String>) {
    while lines.last().is_some_and(|l| l.trim_ascii().is_empty()) {
        lines.pop();
    }
}

struct Md<'a> {
    src: &'a str,
    config: &'a Config,
    line_starts: Vec<usize>,
    /// Where container content (after `> ` and list prefixes) starts on
    /// each row.
    map: ContentMap,
}

impl<'a> Md<'a> {
    fn new(src: &'a str, root: Node, config: &'a Config) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        Md {
            src,
            config,
            line_starts,
            map: ContentMap::new(root, src),
        }
    }

    fn text(&self, node: Node) -> &'a str {
        &self.src[node.start_byte()..node.end_byte()]
    }

    /// The node's text, one entry per source row, with container prefixes
    /// removed.
    fn content_lines(&self, node: Node) -> Vec<String> {
        self.map
            .content_lines(node, self.src)
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// `node`'s source lines, with each row after the first cut at the
    /// node's own starting column (the width of its container prefix)
    /// rather than at the content start. The grammar counts an indented
    /// code block's indent, and the markers of a list it misreads, as part
    /// of a row's continuation; this keeps them.
    fn column_lines(&self, node: Node) -> Vec<String> {
        self.lines_at_column(node, node.start_position().column)
    }

    /// `node`'s lines, each starting at `column` of its row (or at the
    /// node's or the row's content start, if that is earlier).
    fn lines_at_column(&self, node: Node, column: usize) -> Vec<String> {
        let first = node.start_position().row;
        let mut lines = self.content_lines(node);
        for (i, line) in lines.iter_mut().enumerate() {
            let row = first + i;
            let content = if i == 0 {
                node.start_byte()
            } else {
                self.map.content_start(row)
            };
            let start = (self.line_starts[row] + column).min(content);
            if self.src.is_char_boundary(start) {
                *line = format!("{}{line}", &self.src[start..content]);
            }
        }
        lines
    }

    /// The column where `node`'s container content starts: the least
    /// content start over the node's non-blank rows. A preceding nested
    /// list can claim more of a row as continuation than the container
    /// owns, so no single row is reliable.
    fn container_col(&self, node: Node) -> usize {
        (node.start_position().row..=node.end_position().row)
            .filter(|&row| row < self.line_starts.len())
            .filter(|&row| {
                let end = self.line_starts.get(row + 1).map_or(self.src.len(), |e| *e);
                !self.src[self.line_starts[row]..end].trim_ascii().is_empty()
            })
            .map(|row| self.map.content_start(row) - self.line_starts[row])
            .min()
            .unwrap_or(0)
    }

    /// The row holding byte `offset`.
    fn row_of(&self, offset: usize) -> usize {
        self.line_starts
            .partition_point(|&s| s <= offset)
            .saturating_sub(1)
    }

    /// The row of the first top-level block whose shape differs from the
    /// matching block of `out`.
    fn first_shape_difference(&self, root: Node, out: &str) -> usize {
        let Ok(tree) = parse(out, "markdown") else {
            return 0;
        };
        let formatted = Md::new(out, tree.root_node(), self.config);
        let theirs = Md::block_children(tree.root_node());
        Md::block_children(root)
            .iter()
            .enumerate()
            .find(|(i, block)| {
                theirs
                    .get(*i)
                    .is_none_or(|other| formatted.shape(*other) != self.shape(**block))
            })
            .map_or(0, |(_, block)| block.start_position().row)
    }

    /// What a reader sees of `node`: block kinds and nesting, heading
    /// levels, inline structure and text, and code content. Two documents
    /// with the same shape render alike; marker style, numbering,
    /// whitespace, fence style and YAML layout are not part of it.
    fn shape(&self, node: Node) -> String {
        let mut out = String::new();
        self.shape_into(node, &mut out);
        out
    }

    fn shape_into(&self, node: Node, out: &mut String) {
        let kind = node.kind();
        let inline = |n: Node| {
            n.named_children(&mut n.walk())
                .find(|c| c.kind() == "inline")
                .map(|i| text_shape(&self.content_lines(i).join("\n")))
                .unwrap_or_default()
        };
        match kind {
            "atx_heading" => {
                let level = children(node)
                    .iter()
                    .find_map(|c| c.kind().strip_prefix("atx_h")?.chars().next())
                    .unwrap_or('1');
                let text = children(node)
                    .into_iter()
                    .find(|c| c.kind() == "inline")
                    .map(|i| text_shape(&self.atx_text(i)))
                    .unwrap_or_default();
                out.push_str(&format!("(h{level} {text})"));
            }
            "setext_heading" => {
                let level = if children(node)
                    .iter()
                    .any(|c| c.kind() == "setext_h2_underline")
                {
                    '2'
                } else {
                    '1'
                };
                let text = children(node)
                    .into_iter()
                    .find(|c| c.kind() == "paragraph")
                    .map(inline)
                    .unwrap_or_default();
                out.push_str(&format!("(h{level} {text})"));
            }
            "paragraph" => out.push_str(&format!("(p {})", inline(node))),
            "pipe_table_header" | "pipe_table_row" => {
                // Rows with fewer cells than the header render padded with
                // empty cells, which is how the formatter writes them.
                let mut cells: Vec<String> = children(node)
                    .into_iter()
                    .filter(|c| c.kind() == "pipe_table_cell")
                    .map(|c| self.shape(c))
                    .collect();
                while cells.last().is_some_and(|c| c == "(td )") {
                    cells.pop();
                }
                out.push_str(&format!("({kind}{})", cells.concat()));
            }
            "pipe_table_cell" => {
                out.push_str(&format!(
                    "(td {})",
                    text_shape(self.text(node).trim_ascii())
                ));
            }
            "fenced_code_block" => {
                let kids = children(node);
                let info = kids
                    .iter()
                    .find(|c| c.kind() == "info_string")
                    .map(|c| self.text(*c).split_ascii_whitespace().next().unwrap_or(""))
                    .unwrap_or("");
                let body = if matches!(info, "yaml" | "yml") {
                    String::new()
                } else {
                    kids.iter()
                        .find(|c| c.kind() == "code_fence_content")
                        .map(|c| dedent(&self.content_lines(*c)))
                        .unwrap_or_default()
                        .trim_end_matches(['`', '~', '\n'])
                        .to_string()
                };
                out.push_str(&format!("(code {info} {body})"));
            }
            "indented_code_block" => {
                out.push_str(&format!("(code  {})", dedent(&self.column_lines(node))));
            }
            "html_block" | "link_reference_definition" => {
                let text: String = self
                    .text(node)
                    .chars()
                    .filter(|c| !c.is_ascii_whitespace())
                    .collect();
                out.push_str(&format!("({kind} {text})"));
            }
            "minus_metadata" | "plus_metadata" | "thematic_break" => {
                out.push_str(&format!("({kind})"));
            }
            "task_list_marker_checked" | "task_list_marker_unchecked" => {
                out.push_str(&format!("({kind})"));
            }
            "document" | "section" => {
                for kid in children(node) {
                    self.shape_into(kid, out);
                }
            }
            "list" => {
                // A loose list wraps its items' text in paragraphs.
                let items: Vec<Node> = children(node)
                    .into_iter()
                    .filter(|c| c.kind() == "list_item")
                    .collect();
                let loose = (0..items.len()).any(|i| self.item_loose(&items, i));
                out.push_str(if loose { "(loose_list" } else { "(list" });
                for kid in items {
                    self.shape_into(kid, out);
                }
                out.push(')');
            }
            _ if kind.starts_with("list_marker")
                || is_container_token(kind)
                || !node.is_named() => {}
            "block_continuation" | "pipe_table_delimiter_row" => {}
            _ => {
                out.push('(');
                out.push_str(kind);
                for kid in children(node) {
                    self.shape_into(kid, out);
                }
                out.push(')');
            }
        }
    }

    /// Whether the grammar has read part of `node` differently from
    /// CommonMark and GFM, or the formatter cannot keep its meaning: a
    /// fenced code block closed on a line indented 4 or more columns or
    /// with text before the fence, a table cell holding an unescaped `|`
    /// (GFM splits the cell there, even inside math or a code span), or a
    /// table row with more cells than its header.
    fn misread(&self, node: Node) -> Option<usize> {
        if node.kind() == "pipe_table" {
            // GFM drops cells past the header's count; the formatter would
            // widen the header instead.
            let cells = |row: Node| {
                children(row)
                    .iter()
                    .filter(|c| c.kind() == "pipe_table_cell")
                    .count()
            };
            let rows = children(node);
            let header = rows
                .iter()
                .find(|r| r.kind() == "pipe_table_header")
                .map_or(0, |h| cells(*h));
            let wide = rows
                .iter()
                .find(|r| r.kind() == "pipe_table_row" && cells(**r) > header);
            if let Some(row) = wide {
                return Some(row.start_position().row);
            }
            // A line indented 4 or more columns ends a table in GFM
            // renderers; the grammar keeps it as a row.
            let column = self.container_col(node);
            let indented = rows.iter().map(|r| r.start_position().row).find(|&row| {
                let line_end = self.line_starts.get(row + 1).map_or(self.src.len(), |e| *e);
                let start = (self.line_starts[row] + column).min(line_end);
                let line = &self.src[start..line_end];
                line.len() - line.trim_ascii_start().len() >= 4
            });
            if indented.is_some() {
                return indented;
            }
        }
        if node.kind() == "pipe_table_cell" {
            let text = self.text(node);
            let unescaped = text
                .match_indices('|')
                .any(|(i, _)| !text[..i].ends_with('\\'));
            if unescaped {
                return Some(node.start_position().row);
            }
        }
        if node.kind() == "fenced_code_block" {
            let delimiters: Vec<Node> = children(node)
                .into_iter()
                .filter(|c| c.kind() == "fenced_code_block_delimiter")
                .collect();
            if let [_, close] = delimiters.as_slice() {
                let row = close.start_position().row;
                let text = self.text(*close);
                let fence_at = close.start_byte() + text.len() - text.trim_ascii_start().len();
                let before = &self.src[self.map.content_start(row).min(fence_at)..fence_at];
                if !before.trim_ascii().is_empty() || before.len() >= 4 {
                    return Some(row);
                }
            }
        }
        // The repair for a lazy continuation keeps its lines as written;
        // CommonMark may read lines inside it, or right after it, as part
        // of a list the grammar separates, so only plain cases are safe.
        let blocks = Self::block_children(node);
        for (i, pair) in blocks.windows(2).enumerate() {
            let (prev, lazy) = (pair[0], pair[1]);
            if !self.lazy_continuation(prev, lazy) {
                continue;
            }
            let nested = lazy.kind() == "list"
                && children(lazy).iter().any(|item| {
                    Self::block_children(*item)
                        .iter()
                        .any(|b| b.kind() != "paragraph")
                });
            let adjacent = blocks.get(i + 2).is_some_and(|next| {
                next.kind() != "paragraph"
                    && next.start_position().row == self.last_content_row(lazy) + 1
            });
            if nested || adjacent {
                return Some(lazy.start_position().row);
            }
        }
        children(node).into_iter().find_map(|c| self.misread(c))
    }

    /// Last row holding non-whitespace text of `node`, not counting
    /// container prefixes such as the `>` of an otherwise blank quoted line.
    fn last_content_row(&self, node: Node) -> usize {
        let first = node.start_position().row;
        let lines = self.content_lines(node);
        lines
            .iter()
            .rposition(|l| !l.trim_ascii().is_empty())
            .map_or(first, |i| first + i)
    }

    fn block_children(node: Node) -> Vec<Node> {
        let mut out = Vec::new();
        for kid in children(node) {
            if kid.kind() == "section" {
                out.extend(Self::block_children(kid));
            } else if kid.is_named() && !is_container_token(kid.kind()) {
                out.push(kid);
            }
        }
        out
    }

    /// Whether list item `i` of `items` is loose: a blank line between its
    /// blocks, or after it and before the next item. An empty item has no
    /// blocks, so its own row is used.
    fn item_loose(&self, items: &[Node], i: usize) -> bool {
        let next_row = items.get(i + 1).map(|n| n.start_position().row);
        let blocks = Self::block_children(items[i]);
        if blocks.is_empty() {
            return next_row.is_some_and(|n| n > items[i].start_position().row + 1);
        }
        self.has_blank_between(&blocks, next_row)
    }

    /// Whether a blank line separates `blocks` (or follows the last one,
    /// before `next_row`).
    fn has_blank_between(&self, blocks: &[Node], next_row: Option<usize>) -> bool {
        let mut prev: Option<usize> = None;
        for block in blocks {
            if prev.is_some_and(|p| block.start_position().row > p + 1) {
                return true;
            }
            prev = Some(self.last_content_row(*block));
        }
        matches!((prev, next_row), (Some(p), Some(n)) if n > p + 1)
    }

    /// Renders a container's child blocks, separated by a blank line, or
    /// by a plain newline when `tight`.
    fn blocks(&self, container: Node, tight: bool) -> Vec<String> {
        let mut out = Vec::new();
        let mut list_run: Option<(bool, usize)> = None;
        let mut prev: Option<Node> = None;
        let mut lazy_prev = false;
        for block in Self::block_children(container) {
            // Text right after a lazy continuation is still the same
            // paragraph.
            let same_paragraph = lazy_prev
                && block.kind() == "paragraph"
                && prev.is_some_and(|p| block.start_position().row == self.last_content_row(p) + 1);
            lazy_prev = false;
            if same_paragraph {
                out.extend(self.block(block, 0));
                list_run = None;
                prev = Some(block);
                continue;
            }
            if prev.is_some_and(|p| self.lazy_continuation(p, block)) {
                // Still paragraph text under CommonMark; keep it as written.
                let mut lines = self.column_lines(block);
                for line in &mut lines {
                    *line = line.trim_ascii_end().to_string();
                }
                trim_blank_tail(&mut lines);
                out.extend(lines);
                list_run = None;
                prev = Some(block);
                lazy_prev = true;
                continue;
            }
            let nth = if block.kind() == "list" {
                let ordered = is_ordered(block);
                let nth = match list_run {
                    Some((o, n)) if o == ordered => n + 1,
                    _ => 0,
                };
                list_run = Some((ordered, nth));
                nth
            } else {
                list_run = None;
                0
            };
            if prev.is_some_and(|p| !tight && !self.joined(p, block)) {
                out.push(String::new());
            }
            out.extend(self.block(block, nth));
            prev = Some(block);
        }
        out
    }

    /// A block the grammar starts directly after a paragraph that, under
    /// CommonMark, cannot interrupt one: an indented code block, or an
    /// ordered list that does not start at 1. Its lines continue the
    /// paragraph.
    fn lazy_continuation(&self, prev: Node, next: Node) -> bool {
        if prev.kind() != "paragraph"
            || next.start_position().row != self.last_content_row(prev) + 1
        {
            return false;
        }
        if next.kind() == "indented_code_block" {
            return true;
        }
        if next.kind() != "list" || !is_ordered(next) {
            return false;
        }
        let marker = next
            .named_child(0)
            .and_then(|item| item.named_child(0))
            .map_or("", |m| self.text(m));
        let digits = marker.trim_ascii().trim_end_matches(['.', ')']);
        digits.parse::<u64>().is_ok_and(|n| n != 1)
    }

    /// Blocks Prettier keeps on adjacent lines: consecutive link reference
    /// definitions, and HTML blocks with no blank line between them.
    fn joined(&self, prev: Node, next: Node) -> bool {
        match (prev.kind(), next.kind()) {
            ("link_reference_definition", "link_reference_definition") => true,
            ("html_block", "html_block") => {
                next.start_position().row == self.last_content_row(prev) + 1
            }
            _ => false,
        }
    }

    fn block(&self, node: Node, nth: usize) -> Vec<String> {
        match node.kind() {
            "paragraph" => node
                .named_children(&mut node.walk())
                .find(|c| c.kind() == "inline")
                .map(|inline| self.inline_lines(inline))
                .unwrap_or_default(),
            "atx_heading" => vec![self.atx_heading(node)],
            "setext_heading" => self.setext_heading(node),
            "thematic_break" => vec!["---".to_string()],
            "fenced_code_block" => self.fenced_code(node),
            "list" => self.list(node, nth),
            "block_quote" => {
                let lines: Vec<String> = self
                    .blocks(node, false)
                    .into_iter()
                    .map(|l| {
                        if l.is_empty() {
                            ">".into()
                        } else {
                            format!("> {l}")
                        }
                    })
                    .collect();
                if lines.is_empty() {
                    vec![">".into()]
                } else {
                    lines
                }
            }
            "pipe_table" => self.table(node),
            "link_reference_definition" => self.link_definition(node),
            "minus_metadata" => self.front_matter(node),
            "indented_code_block" => {
                let mut lines = self.column_lines(node);
                trim_blank_tail(&mut lines);
                lines
            }
            _ => {
                let mut lines = self.content_lines(node);
                for line in &mut lines {
                    *line = line.trim_ascii_end().to_string();
                }
                trim_blank_tail(&mut lines);
                lines
            }
        }
    }

    fn inline_lines(&self, inline: Node) -> Vec<String> {
        let text = self.content_lines(inline).join("\n");
        format_inline(&text)
            .split('\n')
            .map(str::to_string)
            .collect()
    }

    /// An ATX heading's text without its optional closing `#` sequence.
    fn atx_text(&self, inline: Node) -> String {
        let text = self.content_lines(inline).join(" ");
        let text = text.trim_ascii_end();
        let without_closing = text.trim_end_matches('#');
        if without_closing.is_empty() || without_closing.ends_with([' ', '\t']) {
            without_closing.trim_ascii_end().to_string()
        } else {
            text.to_string()
        }
    }

    fn atx_heading(&self, node: Node) -> String {
        let level = children(node)
            .iter()
            .find_map(|c| {
                c.kind()
                    .strip_prefix("atx_h")?
                    .strip_suffix("_marker")?
                    .parse()
                    .ok()
            })
            .unwrap_or(1);
        let marker = "#".repeat(level);
        let Some(inline) = children(node).into_iter().find(|c| c.kind() == "inline") else {
            return marker;
        };
        let text = self.atx_text(inline);
        let content = format_inline(&text);
        if content.is_empty() {
            marker
        } else {
            format!("{marker} {content}")
        }
    }

    fn setext_heading(&self, node: Node) -> Vec<String> {
        let kids = children(node);
        let lines = kids
            .iter()
            .find(|c| c.kind() == "paragraph")
            .map(|p| self.block(*p, 0))
            .unwrap_or_default();
        let h2 = kids.iter().any(|c| c.kind() == "setext_h2_underline");
        match lines.as_slice() {
            [line] => vec![format!("{} {line}", if h2 { "##" } else { "#" })],
            _ => {
                let mut lines = lines;
                lines.push(if h2 { "---" } else { "===" }.to_string());
                lines
            }
        }
    }

    fn fenced_code(&self, node: Node) -> Vec<String> {
        let kids = children(node);
        let info = kids
            .iter()
            .find(|c| c.kind() == "info_string")
            .map(|c| self.text(*c).trim_ascii().to_string())
            .unwrap_or_default();
        let fence_indent = kids
            .iter()
            .find(|c| c.kind() == "fenced_code_block_delimiter")
            .map(|d| {
                // The fence's indent, relative to its container's content
                // column. The delimiter node may include that indent, and
                // a preceding nested list can claim it as continuation on
                // the fence's row, so measure the container's column on
                // every row of the block and take the least.
                let text = self.text(*d);
                let own = text.len() - text.trim_start_matches(' ').len();
                let fence_col = d.start_byte() + own - self.line_starts[d.start_position().row];
                fence_col.saturating_sub(self.container_col(node))
            })
            .unwrap_or(0);
        // Content rows are cut at the container's column too: the grammar
        // can count part of the code's own indent as continuation.
        let mut content: Vec<String> = kids
            .iter()
            .find(|c| c.kind() == "code_fence_content")
            .map(|c| self.lines_at_column(*c, self.container_col(node)))
            .unwrap_or_default();
        for line in &mut content {
            let strip = line.len() - line.trim_start_matches(' ').len();
            *line = line[strip.min(fence_indent)..].to_string();
        }
        // The grammar does not close a fence on a last line with no line
        // ending after it; CommonMark does.
        let delimiters: Vec<&str> = kids
            .iter()
            .filter(|c| c.kind() == "fenced_code_block_delimiter")
            .map(|d| self.text(*d).trim_ascii())
            .collect();
        if let ([open], Some(last)) = (delimiters.as_slice(), content.last()) {
            let unit = open.chars().next().unwrap_or('`');
            let indent = last.len() - last.trim_start_matches(' ').len();
            let fence = last.trim_ascii();
            if indent < 4
                && fence.len() >= open.len()
                && fence.chars().all(|c| c == unit)
                && self.src.ends_with(last.as_str())
            {
                content.pop();
            }
        }
        // Prettier formats embedded YAML; other languages stay verbatim.
        // A `{{ }}` template (Helm, Jinja) is not YAML until rendered.
        let template = content.iter().any(|l| l.contains("{{") || l.contains("{%"));
        if matches!(info.split_ascii_whitespace().next(), Some("yaml" | "yml"))
            && !content.is_empty()
            && !template
        {
            let body = format!("{}\n", content.join("\n"));
            if let Ok(yaml) = super::yaml::format(&body, self.config) {
                content = yaml.lines().map(str::to_string).collect();
            }
        }
        let unit = if info.contains('`') { '~' } else { '`' };
        let longest = content
            .iter()
            .flat_map(|l| l.split(|c| c != unit))
            .map(str::len)
            .max()
            .unwrap_or(0);
        let fence = unit.to_string().repeat((longest + 1).max(3));
        let mut lines = vec![format!("{fence}{info}")];
        lines.extend(content);
        lines.push(fence);
        lines
    }

    fn list(&self, list: Node, nth: usize) -> Vec<String> {
        let items: Vec<Node> = children(list)
            .into_iter()
            .filter(|c| c.kind() == "list_item")
            .collect();
        let ordered = is_ordered(list);
        let numbers: Vec<u64> = items
            .iter()
            .map(|item| {
                let marker = item.named_child(0).map_or("", |m| self.text(m));
                marker
                    .trim_ascii()
                    .trim_end_matches(['.', ')'])
                    .parse()
                    .unwrap_or(1)
            })
            .collect();
        let start = numbers.first().copied().unwrap_or(1);
        let all_ones = match numbers.as_slice() {
            [0, 1, 1, ..] => true,
            [0, ..] if numbers.len() > 2 => false,
            [_, second, ..] => *second == 1,
            _ => false,
        };

        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let loose = self.item_loose(&items, i);
            let marker = match (ordered, nth.is_multiple_of(2)) {
                (true, even) => {
                    let n = match i {
                        0 => start,
                        _ if all_ones => 1,
                        _ => start + i as u64,
                    };
                    format!("{n}{}", if even { "." } else { ")" })
                }
                (false, true) => "-".to_string(),
                (false, false) => "*".to_string(),
            };
            let mut body = self.blocks(*item, !loose);
            if let Some(task) = children(*item)
                .iter()
                .find(|c| c.kind().starts_with("task_list_marker_"))
            {
                let task = if task.kind().ends_with("_checked") {
                    "[x]"
                } else {
                    "[ ]"
                };
                match body.first_mut() {
                    Some(first) => *first = format!("{task} {first}"),
                    None => body.push(task.to_string()),
                }
            }
            let indent = " ".repeat(marker.len() + 1);
            match body.split_first() {
                None => out.push(marker),
                Some((first, rest)) => {
                    // Trailing spaces can be a hard line break; keep them.
                    out.push(if first.is_empty() {
                        marker
                    } else {
                        format!("{marker} {first}")
                    });
                    out.extend(rest.iter().map(|l| {
                        if l.is_empty() {
                            String::new()
                        } else {
                            format!("{indent}{l}")
                        }
                    }));
                }
            }
            if loose && i + 1 < items.len() {
                out.push(String::new());
            }
        }
        out
    }

    fn table(&self, table: Node) -> Vec<String> {
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut aligns: Vec<(bool, bool)> = Vec::new();
        for row in children(table) {
            match row.kind() {
                "pipe_table_delimiter_row" => {
                    aligns = children(row)
                        .iter()
                        .filter(|c| c.kind() == "pipe_table_delimiter_cell")
                        .map(|c| {
                            let kinds: Vec<&str> = children(*c).iter().map(|k| k.kind()).collect();
                            (
                                kinds.contains(&"pipe_table_align_left"),
                                kinds.contains(&"pipe_table_align_right"),
                            )
                        })
                        .collect();
                }
                "pipe_table_header" | "pipe_table_row" => rows.push(
                    children(row)
                        .iter()
                        .filter(|c| c.kind() == "pipe_table_cell")
                        .map(|c| format_inline(self.text(*c).trim_ascii()))
                        .collect(),
                ),
                _ => {}
            }
        }
        let columns = aligns
            .len()
            .max(rows.iter().map(Vec::len).max().unwrap_or(0));
        aligns.resize(columns, (false, false));
        let mut widths = vec![3; columns];
        for row in &mut rows {
            row.resize(columns, String::new());
            for (w, cell) in widths.iter_mut().zip(row.iter()) {
                *w = (*w).max(cell.width());
            }
        }
        let render = |cells: Vec<String>| format!("| {} |", cells.join(" | "));
        let mut lines = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let cells = row
                .iter()
                .zip(&widths)
                .zip(&aligns)
                .map(|((cell, &w), &(left, right))| {
                    let pad = w - cell.width();
                    match (left, right) {
                        (true, true) => {
                            let before = pad / 2;
                            format!("{}{cell}{}", " ".repeat(before), " ".repeat(pad - before))
                        }
                        (false, true) => format!("{}{cell}", " ".repeat(pad)),
                        _ => format!("{cell}{}", " ".repeat(pad)),
                    }
                })
                .collect();
            lines.push(render(cells));
            if r == 0 {
                let delims = widths
                    .iter()
                    .zip(&aligns)
                    .map(|(&w, &(left, right))| match (left, right) {
                        (true, true) => format!(":{}:", "-".repeat(w - 2)),
                        (true, false) => format!(":{}", "-".repeat(w - 1)),
                        (false, true) => format!("{}:", "-".repeat(w - 1)),
                        (false, false) => "-".repeat(w),
                    })
                    .collect();
                lines.push(render(delims));
            }
        }
        lines
    }

    fn link_definition(&self, node: Node) -> Vec<String> {
        let kids = children(node);
        let part = |kind: &str| {
            kids.iter()
                .find(|c| c.kind() == kind)
                .map(|c| self.text(*c))
        };
        let single_row = node.start_position().row == self.last_content_row(node);
        match (part("link_label"), part("link_destination")) {
            (Some(label), Some(dest)) if single_row => {
                let title = part("link_title")
                    .map(|t| format!(" {t}"))
                    .unwrap_or_default();
                vec![format!("{label}: {dest}{title}")]
            }
            _ => {
                let mut lines = self.content_lines(node);
                trim_blank_tail(&mut lines);
                lines
            }
        }
    }

    /// `---`-delimited front matter, formatted as YAML.
    fn front_matter(&self, node: Node) -> Vec<String> {
        let mut lines = self.content_lines(node);
        trim_blank_tail(&mut lines);
        if lines.len() < 2 {
            return lines;
        }
        let body = lines[1..lines.len() - 1].join("\n");
        if body.contains("{{") || body.contains("{%") {
            return lines;
        }
        let Ok(yaml) = super::yaml::format(&format!("{body}\n"), self.config) else {
            return lines;
        };
        let mut out = vec!["---".to_string()];
        out.extend(yaml.lines().map(str::to_string));
        out.push("---".to_string());
        out
    }
}

/// Whether `rest` (a paragraph line without its indentation) would start a
/// block if the indentation were removed.
fn starts_block(rest: &str) -> bool {
    let first = rest.chars().next().unwrap_or(' ');
    let after_marker = |n: usize| rest[n..].is_empty() || rest[n..].starts_with([' ', '\t']);
    if matches!(first, '#' | '>' | '<' | '|' | '=' | '`' | '~') {
        return true;
    }
    if matches!(first, '-' | '*' | '+' | '_') {
        return after_marker(1) || rest.chars().all(|c| c == first || c == ' ');
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && rest[digits..].starts_with(['.', ')']) && after_marker(digits + 1)
}

/// Normalizes inline Markdown (one paragraph, heading or cell).
///
/// Rewriting emphasis delimiters can re-pair them with other `*` or `_`
/// in the text, so each result is parsed again and must have the same
/// structure as the input. If it does not, the delimiters are kept as
/// written; if that still differs, the text is returned unchanged.
fn format_inline(text: &str) -> String {
    if too_deep_inline(text) {
        return text.to_string();
    }
    let Ok(tree) = parse(text, "markdown_inline") else {
        return text.to_string();
    };
    let root = tree.root_node();
    if root.has_error() || super::check_depth("Markdown", root).is_err() {
        return text.to_string();
    }
    let excluded = opaque_ranges(root);
    let shape = inline_shape(text, root, &excluded);
    let pairs = emphasis::pairs(text, &excluded);
    for rewrite in [true, false] {
        let mut writer = Inline {
            src: text,
            out: String::new(),
            rewrite,
            pairs: &pairs,
        };
        writer.contents(root, false, 0);
        let trimmed = writer.out.trim_ascii_end().len();
        writer.out.truncate(trimmed);
        let same = parse(&writer.out, "markdown_inline").is_ok_and(|t| {
            let root = t.root_node();
            !root.has_error() && inline_shape(&writer.out, root, &opaque_ranges(root)) == shape
        });
        if same {
            return writer.out;
        }
    }
    text.to_string()
}

/// Whether a `hard_line_break` node's text is a break under CommonMark:
/// a backslash, or two or more spaces, before the line ending. cmark
/// reads `a  \t` (spaces, then a tab) as a soft break.
fn is_hard_break(text: &str) -> bool {
    let trailing = text.trim_end_matches(['\n', '\r']);
    trailing.ends_with('\\') || trailing.ends_with("  ")
}

/// Ranges whose `*` and `_` are not emphasis delimiters: code spans, raw
/// HTML, autolinks and link destinations and titles. The grammar's LaTeX
/// spans (`$...$`) are not CommonMark, so their delimiters still count.
fn opaque_ranges(root: Node) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if matches!(
            node.kind(),
            "code_span"
                | "html_tag"
                | "uri_autolink"
                | "email_autolink"
                | "link_destination"
                | "link_title"
        ) {
            out.push(node.start_byte()..node.end_byte());
        } else {
            stack.extend(children(node));
        }
    }
    out
}

/// The inline structure of `text`: its links, images and code spans as
/// nested markers from the grammar, and its emphasis as CommonMark pairs
/// it, with whitespace removed.
fn inline_shape(text: &str, root: Node, excluded: &[std::ops::Range<usize>]) -> (String, String) {
    fn marks(node: Node, text: &str, out: &mut String) {
        let kind = node.kind();
        if kind == "hard_line_break" && is_hard_break(&text[node.start_byte()..node.end_byte()]) {
            out.push_str("(br)");
        }
        let mark = node.is_named()
            && !matches!(
                kind,
                "emphasis" | "strong_emphasis" | "emphasis_delimiter" | "inline"
            )
            && node.child_count() > 0;
        if mark {
            out.push('(');
            out.push_str(kind);
        }
        for kid in children(node) {
            marks(kid, text, out);
        }
        if mark {
            out.push(')');
        }
    }
    let mut structure = String::new();
    marks(root, text, &mut structure);
    (structure, emphasis::shape(text, excluded))
}

struct Inline<'a> {
    src: &'a str,
    out: String,
    /// Normalize emphasis delimiters (`*a*` to `_a_`, `__a__` to `**a**`).
    rewrite: bool,
    /// CommonMark's emphasis pairs in `src` (see `markdown_emphasis.rs`).
    pairs: &'a HashSet<(usize, usize, usize)>,
}

impl Inline<'_> {
    /// The characters around the delimiter run that holds byte `at`.
    fn run_neighbors(&self, at: usize) -> (Option<char>, Option<char>) {
        let c = self.src.as_bytes()[at];
        let start = self.src[..at].trim_end_matches(c as char).len();
        let end = at + self.src[at..].len() - self.src[at..].trim_start_matches(c as char).len();
        (
            self.src[..start].chars().next_back(),
            self.src[end..].chars().next(),
        )
    }

    /// Whether `delimiter` could open at `open` and close at `close`, the
    /// first and last bytes of an emphasis node, under CommonMark's
    /// flanking rules.
    fn can_pair(&self, delimiter: char, open: usize, close: usize) -> bool {
        let (before, after) = self.run_neighbors(open);
        let opens = emphasis::can_delimit(delimiter, before, after, true);
        let (before, after) = self.run_neighbors(close);
        opens && emphasis::can_delimit(delimiter, before, after, false)
    }

    /// Whether CommonMark pairs this emphasis node's delimiters as the
    /// grammar does. The grammar accepts some pairs CommonMark does not,
    /// such as the `_` in `ota_0 or ota_1`.
    fn is_real(&self, node: Node) -> bool {
        let width = if node.kind() == "strong_emphasis" {
            2
        } else {
            1
        };
        node.end_byte() >= node.start_byte() + 2 * width
            && self
                .pairs
                .contains(&(node.start_byte(), node.end_byte() - width, width))
    }

    fn verbatim(&mut self, node: Node) {
        self.out
            .push_str(&self.src[node.start_byte()..node.end_byte()]);
    }

    /// Plain text between nodes: whitespace runs collapse to one space, and
    /// soft line breaks lose the whitespace around them.
    fn gap(&mut self, start: usize, end: usize) {
        let mut pos = start;
        for (i, part) in self.src[start..end].split('\n').enumerate() {
            if i > 0 {
                let kept = self.out.trim_end_matches([' ', '\t']).len();
                let trimmed = kept < self.out.len();
                self.out.truncate(kept);
                // An unescaped `\` at the end of a line is a hard break;
                // keep the space that stopped it from being one.
                let slashes = self.out.len() - self.out.trim_end_matches('\\').len();
                if trimmed && slashes % 2 == 1 {
                    self.out.push(' ');
                }
                self.out.push('\n');
            }
            let at_line_start = self.out.is_empty() || self.out.ends_with('\n');
            let next_pos = pos + part.len() + 1;
            let mut part = part;
            if at_line_start {
                let line_end = self.src[pos..]
                    .find('\n')
                    .map_or(self.src.len(), |n| pos + n);
                let indent = part.len() - part.trim_ascii_start().len();
                if i > 0 && starts_block(self.src[pos + indent..line_end].trim_ascii_start()) {
                    self.out.push_str(&part[..indent]);
                }
                part = part.trim_ascii_start();
            }
            let mut last_space = false;
            for c in part.chars() {
                let space = c == ' ' || c == '\t';
                if !(space && last_space) {
                    self.out.push(if space { ' ' } else { c });
                }
                last_space = space;
            }
            pos = next_pos;
        }
    }

    /// Emits `node`'s children and the text between them, skipping its own
    /// first and last `skip` emphasis delimiters.
    fn contents(&mut self, node: Node, in_emphasis: bool, skip: usize) {
        let kids = children(node);
        let delimiters: Vec<usize> = kids
            .iter()
            .enumerate()
            .filter(|(_, k)| k.kind() == "emphasis_delimiter")
            .map(|(i, _)| i)
            .collect();
        let skipped = |i: usize| {
            skip > 0
                && (delimiters.iter().take(skip).any(|&d| d == i)
                    || delimiters.iter().rev().take(skip).any(|&d| d == i))
        };
        let mut pos = node.start_byte();
        for (i, kid) in kids.iter().enumerate() {
            self.gap(pos, kid.start_byte());
            if !skipped(i) {
                self.node(*kid, in_emphasis);
            }
            pos = kid.end_byte();
        }
        self.gap(pos, node.end_byte());
    }

    fn node(&mut self, node: Node, in_emphasis: bool) {
        match node.kind() {
            "code_span" => {
                // A code span that wraps keeps its line breaks but, as in
                // Prettier, not the indentation of its continuation lines.
                let text = &self.src[node.start_byte()..node.end_byte()];
                for (i, line) in text.split('\n').enumerate() {
                    if i > 0 {
                        self.out.push('\n');
                    }
                    self.out
                        .push_str(if i > 0 { line.trim_ascii_start() } else { line });
                }
            }
            "html_tag"
            | "uri_autolink"
            | "email_autolink"
            | "link_destination"
            | "link_title"
            | "backslash_escape"
            | "latex_block"
            | "entity_reference"
            | "numeric_character_reference" => self.verbatim(node),
            "hard_line_break" => {
                let text = &self.src[node.start_byte()..node.end_byte()];
                if !is_hard_break(text) {
                    // Tabs before a line ending are not a break to
                    // CommonMark, though some renderers treat them as one;
                    // keep them as written.
                    self.verbatim(node);
                    return;
                }
                let kept = self.out.trim_end_matches([' ', '\t']).len();
                self.out.truncate(kept);
                self.out.push_str(if text.starts_with('\\') {
                    "\\\n"
                } else {
                    "  \n"
                });
            }
            "emphasis" | "strong_emphasis" if !self.is_real(node) => {
                self.verbatim(node);
            }
            "emphasis" | "strong_emphasis" if !self.rewrite => {
                self.contents(node, in_emphasis, 0);
            }
            "emphasis" => {
                let before = self.src[..node.start_byte()].chars().next_back();
                let after = self.src[node.end_byte()..].chars().next();
                let word = before.is_some_and(char::is_alphanumeric)
                    || after.is_some_and(char::is_alphanumeric);
                // Renderers disagree on whether symbols such as `×` count as
                // punctuation, so `_` is only used next to ASCII or space.
                let plain = [node.start_byte(), node.end_byte() - 1].iter().all(|&at| {
                    let (before, after) = self.run_neighbors(at);
                    [before, after]
                        .iter()
                        .all(|c| c.is_none_or(|c| c.is_ascii() || c.is_whitespace()))
                });
                let underscore =
                    plain && self.can_pair('_', node.start_byte(), node.end_byte() - 1);
                let style = if word || in_emphasis || !underscore {
                    "*"
                } else {
                    "_"
                };
                let named: Vec<Node> = children(node)
                    .into_iter()
                    .filter(|k| k.kind() != "emphasis_delimiter")
                    .collect();
                // `***x***` parses as emphasis around strong; print it as
                // strong around emphasis, `**_x_**`.
                if let [strong] = named.as_slice() {
                    if strong.kind() == "strong_emphasis"
                        && strong.start_byte() == node.start_byte() + 1
                        && strong.end_byte() + 1 == node.end_byte()
                    {
                        self.out.push_str("**");
                        self.out.push_str(style);
                        self.contents(*strong, true, 2);
                        self.out.push_str(style);
                        self.out.push_str("**");
                        return;
                    }
                }
                self.out.push_str(style);
                self.contents(node, true, 1);
                self.out.push_str(style);
            }
            "strong_emphasis" => {
                self.out.push_str("**");
                self.contents(node, in_emphasis, 2);
                self.out.push_str("**");
            }
            _ if node.child_count() == 0 => self.verbatim(node),
            _ => self.contents(node, in_emphasis, 0),
        }
    }
}
