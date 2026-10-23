// reStructuredText formatter (`.rst`): the layout target is docstrfmt's
// default output, without its rewrites that change meaning.
//
// Blocks are separated by one blank line. Paragraphs are rewrapped
// greedily at `max_width` (88). Section titles are written in a fixed
// hierarchy by level (`#` and `*` overlined, then `=`, `-`, `^`, ...); a
// level is assigned to each adornment style in the order the styles first
// appear, as docutils does. Bullets become `-` and enumerated lists are
// renumbered from their first value with a `.` suffix; a list item is
// followed by a blank line only when it holds more than one block. List
// bodies are indented by their marker's width; definitions, block quotes,
// field and footnote bodies, comments and directive bodies by
// `indent_width` (4). `Para::` is written as `Para:` and a standalone `::`.
// Admonitions are formatted as their own documents; other directive bodies,
// literal blocks, doctests, option lists, line blocks and tables are kept
// as written, re-indented.
//
// Unlike docstrfmt, field names and order, target names, classifiers,
// footnote bodies, escapes and runs of spaces inside inline literals are
// kept, and Python code is not reformatted.
//
// The grammar parses tables as paragraphs and reads some input differently
// from docutils; a paragraph that looks like a table or a misread, and a
// multi-line definition term, are kept as written. The output is parsed
// again and its shape (`Rst::shape`) compared with the input's; on any
// difference the file is refused with `MoldyError::Unverified`.
//
// Known gaps (documented rather than half-implemented):
//   - A file with any syntax error is returned unchanged.
//   - Section levels that skip, or nest past the 13 adornment styles, are
//     refused.
//   - Field bodies and footnotes start on the name line only when their
//     first block is a paragraph.

use crate::config::{Config, RstConfig};
use crate::error::MoldyError;
use std::collections::HashMap;
use tree_sitter::{Node, Parser, Tree};
use unicode_width::UnicodeWidthStr;

const LANGUAGE: &str = "reStructuredText";

/// The adornment character and overline flag for each section level.
const SECTION_STYLES: [(char, bool); 13] = [
    ('#', true),
    ('*', true),
    ('=', false),
    ('-', false),
    ('^', false),
    ('"', false),
    ('\'', false),
    ('~', false),
    ('+', false),
    ('.', false),
    ('`', false),
    ('_', false),
    (':', false),
];

/// Directives whose body is RST, formatted as its own document. Text on
/// the first line starts the body.
const ADMONITIONS: &[&str] = &[
    "attention",
    "caution",
    "danger",
    "error",
    "hint",
    "important",
    "note",
    "tip",
    "warning",
    "seealso",
];

/// Directives whose arguments stay on the first line and whose body is
/// RST: a title, an image, a version.
const TITLED: &[&str] = &[
    "admonition",
    "topic",
    "sidebar",
    "figure",
    "list-table",
    "versionadded",
    "versionchanged",
    "versionremoved",
    "deprecated",
];

/// How deeply directive bodies are formatted as nested documents.
const MAX_NESTED_DOCUMENTS: usize = 32;

pub fn format(source: &str, config: &Config) -> Result<String, MoldyError> {
    super::check_block_depth(LANGUAGE, source, open_blocks)?;
    let cfg = &config.rst;
    // A byte order mark is kept in front of the formatted text.
    let (bom, source) = match source.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", source),
    };
    // docutils does not count a non-ASCII space as indentation; the
    // grammar does.
    if let Some(row) = source.lines().position(|l| {
        l.trim_start_matches([' ', '\t'])
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() && !c.is_ascii())
    }) {
        return Err(unverified(row));
    }
    let lines = render(source, cfg, cfg.max_width as usize, 0, 0)?;
    let mut out = lines.join("\n");
    let trimmed = out.trim_end_matches(['\n', ' ']).len();
    out.truncate(trimmed);
    if config.newlines.final_newline && !out.is_empty() {
        out.push('\n');
    }
    let (before, rows) = document_shape(source, cfg, 0)?;
    match document_shape(&out, cfg, 0) {
        Ok((after, _)) if after == before => Ok(format!("{bom}{out}")),
        Ok((after, _)) => {
            // The input line of the top-level block where the shapes part.
            let at = before
                .bytes()
                .zip(after.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            let row = rows.iter().take_while(|(offset, _)| *offset <= at).last();
            Err(unverified(row.map_or(0, |r| r.1)))
        }
        Err(_) => Err(unverified(0)),
    }
}

/// An upper bound on the blocks open on `line`: its indentation is at
/// least one column per level.
fn open_blocks(line: &str) -> usize {
    1 + line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 8 } else { 1 })
        .sum::<usize>()
        / 2
}

/// The error for a tree with a syntax error. The grammar's error recovery
/// can insert a hidden token (a blank line, a dedent) that no node shows;
/// the line is then where the deepest node holding the error ends.
fn syntax_error(root: Node, row: usize) -> MoldyError {
    let mut node = root;
    let line = loop {
        let mut cursor = node.walk();
        let next = node.children(&mut cursor).find(|c| c.has_error());
        match next {
            Some(c) if c.is_error() || c.is_missing() => break c.start_position().row,
            Some(c) => node = c,
            None => break node.end_position().row,
        }
    };
    MoldyError::Syntax {
        language: LANGUAGE.into(),
        line: row + line + 1,
    }
}

fn unverified(row: usize) -> MoldyError {
    MoldyError::Unverified {
        language: LANGUAGE.into(),
        line: row + 1,
    }
}

fn parse(source: &str) -> Result<Tree, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key("rst")
        .ok_or_else(|| MoldyError::UnsupportedLanguage("rst".to_string()))?;
    let mut parser = Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;
    parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))
}

/// Parses `source` and checks it can be formatted: no syntax error, not
/// too deep. `row` is the line `source` starts on in the file.
fn checked_tree(source: &str, depth: usize, row: usize) -> Result<Tree, MoldyError> {
    if depth > MAX_NESTED_DOCUMENTS {
        return Err(super::too_deep(LANGUAGE, row));
    }
    let tree = parse(source)?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(syntax_error(root, row));
    }
    super::check_depth(LANGUAGE, root)?;
    Ok(tree)
}

/// `source` formatted as a document whose lines are `width` columns wide.
fn render(
    source: &str,
    cfg: &RstConfig,
    width: usize,
    depth: usize,
    row: usize,
) -> Result<Vec<String>, MoldyError> {
    let tree = checked_tree(source, depth, row)?;
    let root = tree.root_node();
    let rst = Rst::new(source, cfg, root, depth, row)?;
    rst.container(root, width)
}

/// The shape of `source` as a document (see `Rst::shape`).
/// The shape of `source` as a document (see `Rst::shape`), and the offset
/// in it and source row of each top-level block.
fn document_shape(
    source: &str,
    cfg: &RstConfig,
    depth: usize,
) -> Result<(String, Vec<(usize, usize)>), MoldyError> {
    let tree = checked_tree(source, depth, 0)?;
    let root = tree.root_node();
    let rst = Rst::new(source, cfg, root, depth, 0)?;
    let mut rows = Vec::new();
    let mut out = String::new();
    rst.shape_container_rows(root, &mut out, Some(&mut rows));
    Ok((out, rows))
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    children(node).into_iter().find(|c| c.kind() == kind)
}

fn col(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn indent_lines(lines: Vec<String>, by: usize) -> Vec<String> {
    let pad = " ".repeat(by);
    lines
        .into_iter()
        .map(|l| if l.is_empty() { l } else { format!("{pad}{l}") })
        .collect()
}

/// `first` prefixed to the first line, and `by` spaces to the others.
fn hang(prefix: &str, lines: Vec<String>, by: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len().max(1));
    let mut rest = lines.into_iter();
    match rest.next() {
        Some(first) if !first.is_empty() => out.push(format!("{prefix} {first}")),
        _ => out.push(prefix.to_string()),
    }
    out.extend(indent_lines(rest.collect(), by));
    out
}

/// Tabs expanded to 8-column stops, as docutils reads them.
fn expand_tabs(line: &str) -> String {
    if !line.contains('\t') {
        return line.to_string();
    }
    let mut out = String::new();
    let mut column = 0;
    for ch in line.chars() {
        if ch == '\t' {
            let n = 8 - column % 8;
            out.extend(std::iter::repeat_n(' ', n));
            column += n;
        } else {
            out.push(ch);
            column += col(ch.encode_utf8(&mut [0; 4]));
        }
    }
    out
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// `lines` without their common indentation, trailing whitespace and
/// leading or trailing blank lines.
fn dedent(lines: Vec<String>) -> Vec<String> {
    let lines: Vec<String> = lines
        .iter()
        .map(|l| expand_tabs(l).trim_end().to_string())
        .collect();
    let common = lines
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| leading_spaces(l))
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = lines
        .into_iter()
        .map(|l| {
            if l.is_empty() {
                l
            } else {
                l[common..].to_string()
            }
        })
        .collect();
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    let lead = out.iter().take_while(|l| l.is_empty()).count();
    out.drain(..lead);
    out
}

/// A line made only of one repeated punctuation character, which could be
/// read as a section adornment or a transition.
fn is_adornment_line(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some(c) if c.is_ascii_punctuation() => chars.all(|d| d == c),
        _ => false,
    }
}

/// A line that would start a list, field, explicit markup, line block or
/// doctest: a paragraph starting this way is likely a grammar misread.
fn starts_like_block(line: &str) -> bool {
    let spaced = |rest: &str| rest.is_empty() || rest.starts_with([' ', '\t']);
    let mut chars = line.chars();
    if chars
        .next()
        .is_some_and(|c| "-*+\u{2022}\u{2023}\u{2043}".contains(c))
        && spaced(chars.as_str())
    {
        return true;
    }
    for prefix in ["..", "|", ">>>"] {
        if let Some(rest) = line.strip_prefix(prefix) {
            if spaced(rest) {
                return true;
            }
        }
    }
    if let Some(rest) = line.strip_prefix(':') {
        if let Some(end) = rest.find(':') {
            if end > 0 && spaced(&rest[end + 1..]) {
                return true;
            }
        }
    }
    let word = line.split([' ', '\t']).next().unwrap_or("");
    let (inner, _) = split_enumerator(word);
    let punctuated = word != inner;
    punctuated
        && !inner.is_empty()
        && (inner == "#"
            || inner.chars().all(|c| c.is_ascii_digit())
            || (inner.len() == 1 && inner.chars().all(|c| c.is_ascii_alphabetic()))
            || roman_value(&inner.to_ascii_lowercase()).is_some())
        && spaced(&line[word.len()..])
}

/// A simple or grid table border.
fn is_table_border(line: &str) -> bool {
    let t = line.trim();
    let simple = t.starts_with('=')
        && t.contains(' ')
        && t.split(' ')
            .filter(|s| !s.is_empty())
            .all(|s| s.chars().all(|c| c == '='));
    let grid = t.len() > 2
        && t.starts_with('+')
        && t.ends_with('+')
        && t.chars().all(|c| matches!(c, '+' | '-' | '=' | ':'));
    simple || grid
}

/// Greedy wrapping: the first line may be `first` columns wide and the
/// others `rest`. A word wider than its line sits alone on it. A line that
/// would be read as an adornment takes a word from its neighbour.
fn wrap(words: &[String], first: usize, rest: usize) -> Vec<String> {
    let mut lines: Vec<Vec<&str>> = Vec::new();
    let mut line: Vec<&str> = Vec::new();
    let mut len = 0;
    for word in words {
        let budget = if lines.is_empty() { first } else { rest };
        let w = col(word);
        if !line.is_empty() && len + 1 + w > budget {
            lines.push(std::mem::take(&mut line));
            len = 0;
        }
        len += if line.is_empty() { w } else { w + 1 };
        line.push(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.len() > 1 {
        let mut i = 0;
        while i < lines.len() {
            if is_adornment_line(&lines[i].join(" ")) {
                if i > 0 && lines[i - 1].len() > 1 {
                    let moved = lines[i - 1].pop().unwrap();
                    lines[i].insert(0, moved);
                } else if i + 1 < lines.len() {
                    let next = lines.remove(i + 1);
                    lines[i].extend(next);
                    continue;
                } else if i > 0 {
                    let line = lines.remove(i);
                    lines[i - 1].extend(line);
                    continue;
                }
            }
            i += 1;
        }
    }
    lines.into_iter().map(|l| l.join(" ")).collect()
}

/// What a list continues from when it directly follows another list in
/// the same container (the grammar splits a list at blank lines between
/// items).
#[derive(Default)]
struct Sequence {
    /// The previous enumerated list's numbering.
    numbering: Option<Numbering>,
    /// The previous bullet list's source and output bullet characters.
    bullet: Option<(char, char)>,
    /// Whether the previous block was a definition list.
    definitions: bool,
}

impl Sequence {
    /// Forgets the lists before `block` unless it is a list of that kind.
    fn reset_unless(&mut self, block: Node) {
        if block.kind() != "enumerated_list" {
            self.numbering = None;
        }
        if block.kind() != "bullet_list" {
            self.bullet = None;
        }
        if block.kind() != "definition_list" {
            self.definitions = false;
        }
    }
}

struct Numbering {
    style: Enum,
    /// The enumerator punctuation (`.`, `)` or `()`).
    punct: String,
    next: usize,
    /// Whether an item was auto-numbered (`#`), after which docutils ends
    /// the list at an explicit enumerator.
    auto: bool,
}

/// An enumerated list's numbering style.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Enum {
    Arabic,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

fn roman_value(text: &str) -> Option<usize> {
    let digit = |c: char| -> Option<isize> {
        Some(match c.to_ascii_lowercase() {
            'i' => 1,
            'v' => 5,
            'x' => 10,
            'l' => 50,
            'c' => 100,
            'd' => 500,
            'm' => 1000,
            _ => return None,
        })
    };
    let values: Vec<isize> = text.chars().map(digit).collect::<Option<_>>()?;
    let mut value = 0;
    for (i, v) in values.iter().enumerate() {
        if values.get(i + 1).is_some_and(|n| n > v) {
            value -= v;
        } else {
            value += v;
        }
    }
    // Only the canonical spelling counts (`iiii` is not a numeral).
    let value = usize::try_from(value).ok().filter(|v| *v > 0)?;
    (to_roman(value) == text.to_ascii_lowercase()).then_some(value)
}

fn to_roman(mut value: usize) -> String {
    const TABLE: [(usize, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (n, s) in TABLE {
        while value >= n {
            out.push_str(s);
            value -= n;
        }
    }
    out
}

/// An enumerator's text without its punctuation, and the punctuation form
/// (`.`, `)` or `()`).
fn split_enumerator(marker: &str) -> (&str, &str) {
    if let Some(inner) = marker.strip_prefix('(').and_then(|m| m.strip_suffix(')')) {
        (inner, "()")
    } else if let Some(inner) = marker.strip_suffix(')') {
        (inner, ")")
    } else {
        (marker.strip_suffix('.').unwrap_or(marker), ".")
    }
}

/// The value `text` has in `style`.
fn enum_value(text: &str, style: Enum) -> Option<usize> {
    match style {
        Enum::Arabic => text.parse().ok(),
        Enum::LowerAlpha | Enum::UpperAlpha => {
            let mut chars = text.chars();
            let c = chars.next()?;
            let ok = chars.next().is_none()
                && if style == Enum::LowerAlpha {
                    c.is_ascii_lowercase()
                } else {
                    c.is_ascii_uppercase()
                };
            ok.then(|| (c.to_ascii_lowercase() as u8 - b'a') as usize + 1)
        }
        Enum::LowerRoman | Enum::UpperRoman => {
            let case_ok = if style == Enum::LowerRoman {
                text.chars().all(|c| c.is_ascii_lowercase())
            } else {
                text.chars().all(|c| c.is_ascii_uppercase())
            };
            if case_ok {
                roman_value(text)
            } else {
                None
            }
        }
    }
}

fn enum_text(value: usize, style: Enum) -> String {
    match style {
        Enum::Arabic => value.to_string(),
        Enum::LowerAlpha => ((b'a' + (value - 1) as u8) as char).to_string(),
        Enum::UpperAlpha => ((b'A' + (value - 1) as u8) as char).to_string(),
        Enum::LowerRoman => to_roman(value),
        Enum::UpperRoman => to_roman(value).to_ascii_uppercase(),
    }
}

/// The style and first value of a list whose first two enumerators are
/// `first` and `second`, read as docutils reads an ambiguous letter.
fn enum_style(first: &str, second: Option<&str>) -> Option<(Enum, usize)> {
    if first == "#" {
        return Some((Enum::Arabic, 1));
    }
    if first.chars().all(|c| c.is_ascii_digit()) {
        return Some((Enum::Arabic, first.parse().ok()?));
    }
    let lower = first.chars().all(|c| c.is_ascii_lowercase());
    let upper = first.chars().all(|c| c.is_ascii_uppercase());
    if !lower && !upper {
        return None;
    }
    let (alpha, roman) = if lower {
        (Enum::LowerAlpha, Enum::LowerRoman)
    } else {
        (Enum::UpperAlpha, Enum::UpperRoman)
    };
    let as_roman = enum_value(first, roman);
    let as_alpha = enum_value(first, alpha);
    match (as_alpha, as_roman) {
        (Some(a), Some(r)) => {
            // A single letter that is also a numeral: the next item decides,
            // and `i` alone is roman.
            let next_is = |style: Enum, value: usize| {
                second.is_some_and(|s| s == "#" || enum_value(s, style) == Some(value + 1))
            };
            if next_is(roman, r) && !next_is(alpha, a) {
                Some((roman, r))
            } else if next_is(alpha, a) {
                Some((alpha, a))
            } else if first.eq_ignore_ascii_case("i") {
                Some((roman, r))
            } else {
                Some((alpha, a))
            }
        }
        (Some(a), None) => Some((alpha, a)),
        (None, Some(r)) => Some((roman, r)),
        (None, None) => None,
    }
}

/// A container's blocks, with blocks inside a table grouped as the table.
enum Run<'t> {
    Block(Node<'t>),
    /// Explicit markup and the indented block the grammar ended it before,
    /// which docutils reads as part of it: kept as written.
    Joined(Node<'t>, Node<'t>),
    /// A table's first and last rows, and the container's column.
    Table((usize, usize), usize),
}

/// The first and last rows of each table in `lines`: a simple table from a
/// `=== ===` border after a blank line to a border before a blank line, or
/// a grid table of `+`/`|` lines starting with a border. The grammar does
/// not parse tables.
fn table_regions(lines: &[&str]) -> Vec<(usize, usize)> {
    let blank = |i: usize| lines.get(i).is_none_or(|l| l.trim().is_empty());
    let mut regions = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if !is_table_border(t) || (i > 0 && !blank(i - 1)) {
            i += 1;
            continue;
        }
        let end = if t.starts_with('+') {
            let mut j = i;
            while lines
                .get(j + 1)
                .is_some_and(|l| l.trim_start().starts_with(['+', '|']))
            {
                j += 1;
            }
            j
        } else {
            let mut j = i + 1;
            while j < lines.len() && !(is_table_border(lines[j].trim()) && blank(j + 1)) {
                j += 1;
            }
            j.min(lines.len() - 1)
        };
        if end > i {
            regions.push((i, end));
        }
        i = end + 1;
    }
    regions
}

struct Rst<'a> {
    src: &'a str,
    /// The rows of each table.
    tables: Vec<(usize, usize)>,
    cfg: &'a RstConfig,
    /// Byte offset of each line's start.
    line_starts: Vec<usize>,
    /// The level of each section adornment style seen.
    levels: HashMap<(char, bool), usize>,
    depth: usize,
    /// The line `src` starts on in the file, for error lines.
    row: usize,
}

impl<'a> Rst<'a> {
    fn new(
        src: &'a str,
        cfg: &'a RstConfig,
        root: Node,
        depth: usize,
        row: usize,
    ) -> Result<Self, MoldyError> {
        let mut line_starts = vec![0];
        line_starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        let lines: Vec<&str> = src.lines().collect();
        let mut rst = Rst {
            src,
            tables: table_regions(&lines),
            cfg,
            line_starts,
            levels: HashMap::new(),
            depth,
            row,
        };
        rst.assign_levels(root)?;
        rst.check(root)?;
        Ok(rst)
    }

    fn ind(&self) -> usize {
        self.cfg.indent_width as usize
    }

    fn text(&self, node: Node) -> &'a str {
        &self.src[node.byte_range()]
    }

    fn err_row(&self, node: Node) -> usize {
        self.row + node.start_position().row
    }

    /// Line `row` of the source, without its line ending.
    fn line(&self, row: usize) -> &'a str {
        let start = self.line_starts[row];
        let end = self
            .line_starts
            .get(row + 1)
            .map_or(self.src.len(), |e| e - 1);
        self.src[start..end].trim_end_matches('\r')
    }

    /// The section style of `section`, or `None` if its overline and
    /// underline differ.
    fn section_style(&self, section: Node) -> Option<(char, bool)> {
        let adornments: Vec<Node> = children(section)
            .into_iter()
            .filter(|c| c.kind() == "adornment")
            .collect();
        let under = self.text(*adornments.last()?).chars().next()?;
        let over = children(section)
            .first()
            .is_some_and(|c| c.kind() == "adornment");
        if over && adornments.len() == 2 && !self.text(adornments[0]).starts_with(under) {
            return None;
        }
        Some((under, over))
    }

    /// Assigns levels to section styles in the order they appear, refusing
    /// a skipped level or one past the style table.
    fn assign_levels(&mut self, root: Node) -> Result<(), MoldyError> {
        let mut current = 0;
        for section in children(root).into_iter().filter(|c| c.kind() == "section") {
            // The grammar reads some table rows as titles; docutils does not.
            if self.table_of(section).is_some() {
                continue;
            }
            let style = self
                .section_style(section)
                .ok_or_else(|| unverified(self.err_row(section)))?;
            let next = self.levels.len() + 1;
            let level = *self.levels.entry(style).or_insert(next);
            if level > current + 1 || level > SECTION_STYLES.len() {
                return Err(unverified(self.err_row(section)));
            }
            current = level;
        }
        Ok(())
    }

    /// The table that holds all of `block`, if any.
    fn table_of(&self, block: Node) -> Option<(usize, usize)> {
        if block.kind() == "literal_block" {
            return None;
        }
        let (start, mut end) = (block.start_position().row, block.end_position().row);
        if block.end_position().column == 0 && end > start {
            end -= 1;
        }
        self.tables
            .iter()
            .copied()
            .find(|(a, b)| *a <= start && end <= *b)
    }

    /// A container's blocks, each run of blocks inside one table merged.
    fn runs<'t>(&self, node: Node<'t>) -> Vec<Run<'t>> {
        let base = self.base_column(node);
        let mut out: Vec<Run> = Vec::new();
        for block in self.blocks(node) {
            match self.table_of(block) {
                Some(t) if matches!(out.last(), Some(Run::Table(u, _)) if *u == t) => {}
                Some(t) => out.push(Run::Table(t, base)),
                None if block.kind() == "block_quote"
                    && matches!(out.last(), Some(Run::Block(b)) if self.takes_indented_block(*b)) =>
                {
                    let Some(Run::Block(markup)) = out.pop() else {
                        unreachable!()
                    };
                    out.push(Run::Joined(markup, block));
                }
                None => out.push(Run::Block(block)),
            }
        }
        out
    }

    /// Explicit markup that docutils extends over a following indented
    /// block: anything but an empty comment.
    fn takes_indented_block(&self, node: Node) -> bool {
        match node.kind() {
            "directive" | "footnote" | "citation" | "target" | "substitution_definition" => true,
            "comment" => !self.comment_lines(node).is_empty(),
            _ => false,
        }
    }

    /// The lines from `first`'s start to `last`'s end as written, relative
    /// to `first`'s line.
    fn joined_lines(&self, first: Node, last: Node) -> Vec<String> {
        let start = first.start_position();
        let mut end = last.end_position().row;
        if last.end_position().column == 0 && end > start.row {
            end -= 1;
        }
        let start_col = col(&expand_tabs(&self.line(start.row)[..start.column]));
        let mut lines = vec![expand_tabs(self.line(start.row))
            .get(start_col..)
            .unwrap_or("")
            .trim_end()
            .to_string()];
        let rest: Vec<String> = (start.row + 1..=end)
            .map(|r| expand_tabs(self.line(r)).trim_end().to_string())
            .collect();
        let strip = rest
            .iter()
            .filter(|l| !l.is_empty())
            .map(|l| leading_spaces(l))
            .min()
            .unwrap_or(start_col)
            .min(start_col);
        lines.extend(rest.into_iter().map(|l| {
            if l.is_empty() {
                l
            } else {
                l[strip..].to_string()
            }
        }));
        lines
    }

    /// A table's lines as written, relative to the container's column
    /// `base`: an indented table is a block quote.
    fn table_lines(&self, (start, end): (usize, usize), base: usize) -> Vec<String> {
        let lines: Vec<String> = (start..=end)
            .map(|r| expand_tabs(self.line(r)).trim_end().to_string())
            .collect();
        let strip = lines
            .iter()
            .filter(|l| !l.is_empty())
            .map(|l| leading_spaces(l))
            .min()
            .unwrap_or(0)
            .min(base);
        lines
            .into_iter()
            .map(|l| {
                if l.is_empty() {
                    l
                } else {
                    l[strip..].to_string()
                }
            })
            .collect()
    }

    /// Refuses input the grammar reads differently from docutils in a way
    /// that formatting would make worse: a "section" whose underline is too
    /// short to be one, a paragraph docutils reads as a title, and a
    /// paragraph ending in `::` whose literal block the grammar missed.
    fn check(&self, node: Node) -> Result<(), MoldyError> {
        let refuse = |n: Node| Err(unverified(self.err_row(n)));
        if node.is_named() && node.kind() != "document" {
            let (start, mut end) = (node.start_position().row, node.end_position().row);
            if node.end_position().column == 0 && end > start {
                end -= 1;
            }
            // A node the grammar ran out of a table, or past a dedent.
            let leaves_table = self
                .tables
                .iter()
                .any(|(a, b)| *a <= start && start <= *b && end > *b);
            if leaves_table || self.dedents_below_start(node) {
                return refuse(node);
            }
        }
        match node.kind() {
            "section" => {
                let title = child_of_kind(node, "title").map_or(0, |t| col(self.text(t).trim()));
                let short = children(node)
                    .into_iter()
                    .filter(|c| c.kind() == "adornment")
                    .any(|a| {
                        let n = col(self.text(a));
                        n < 4 && n < title
                    });
                if short {
                    return refuse(node);
                }
            }
            "paragraph" => {
                let lines = self.verbatim_lines(node);
                let rule = |l: &String| is_adornment_line(l.trim());
                let title = match lines.as_slice() {
                    [text, under] => {
                        rule(under)
                            && (col(under.trim()) >= 4 || col(under.trim()) >= col(text.trim()))
                    }
                    [over, _, under] => rule(over) && rule(under) && over.trim() == under.trim(),
                    _ => false,
                };
                if title {
                    return refuse(node);
                }
            }
            _ => {}
        }
        let blocks = self.blocks(node);
        for pair in blocks.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.kind() == "paragraph"
                && !self.has_literal(a)
                && b.kind() == "block_quote"
                && self.text(a).trim_end().ends_with("::")
            {
                return refuse(b);
            }
        }
        for child in children(node) {
            self.check(child)?;
        }
        Ok(())
    }

    /// Whether a node that starts its line has a later line indented less
    /// than it: the grammar ran it past where docutils ends it.
    fn dedents_below_start(&self, node: Node) -> bool {
        let start = node.start_position();
        let mut end = node.end_position().row;
        if node.end_position().column == 0 && end > start.row {
            end -= 1;
        }
        let before = &self.line(start.row)[..start.column];
        if end == start.row || !before.trim().is_empty() {
            return false;
        }
        let start_col = col(&expand_tabs(before));
        (start.row + 1..=end).any(|r| {
            let l = expand_tabs(self.line(r));
            !l.trim().is_empty() && leading_spaces(&l) < start_col
        })
    }

    /// The body blocks of a container, pairing a standalone `::` with its
    /// literal block.
    fn blocks<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        children(node)
            .into_iter()
            .filter(|c| c.is_named() && c.kind() != "attribution")
            .collect()
    }

    fn container(&self, node: Node, width: usize) -> Result<Vec<String>, MoldyError> {
        self.container_first(node, width, None)
    }

    /// A container's blocks separated by blank lines. The first block, if
    /// a paragraph, has `first` columns on its first line.
    fn container_first(
        &self,
        node: Node,
        width: usize,
        first: Option<usize>,
    ) -> Result<Vec<String>, MoldyError> {
        let mut out = Vec::new();
        let mut seq = Sequence::default();
        let mut prev: Option<Node> = None;
        let base = self.base_column(node);
        let mut tight_after: Option<Node> = None;
        for (i, run) in self.runs(node).into_iter().enumerate() {
            // A list the grammar split off at a blank line continues the
            // previous list, with a blank line only after a loose item.
            let joins = match (&run, tight_after) {
                (Run::Block(b), Some(p)) => self.continues(*b, p, &seq),
                _ => false,
            };
            if i > 0 && !joins {
                out.push(String::new());
            }
            tight_after = match &run {
                Run::Block(b) if matches!(b.kind(), "bullet_list" | "enumerated_list") => {
                    self.last_item_tight(*b).then_some(*b)
                }
                _ => None,
            };
            match run {
                Run::Table(rows, base) => {
                    seq = Sequence::default();
                    prev = None;
                    out.extend(self.table_lines(rows, base));
                }
                Run::Joined(markup, quote) => {
                    seq = Sequence::default();
                    out.extend(self.joined_lines(markup, quote));
                    prev = Some(quote);
                }
                Run::Block(block) if block.kind() == "block_quote" => {
                    seq.reset_unless(block);
                    let indent = self.quote_indent(block, prev, base);
                    out.extend(self.block_quote(block, width, indent)?);
                    prev = Some(block);
                }
                Run::Block(block) => {
                    let budget = if i == 0 { first } else { None };
                    seq.reset_unless(block);
                    out.extend(self.block(block, width.max(1), budget, &mut seq)?);
                    prev = Some(block);
                }
            }
        }
        Ok(out)
    }

    fn block(
        &self,
        node: Node,
        width: usize,
        first: Option<usize>,
        seq: &mut Sequence,
    ) -> Result<Vec<String>, MoldyError> {
        if self.explicit_misread(node) {
            return Ok(self.comment(node));
        }
        Ok(match node.kind() {
            "section" => self.section(node),
            "paragraph" => self.paragraph(node, width, first),
            "literal_block" => self.literal(node),
            "bullet_list" | "enumerated_list" => self.list(node, width, seq)?,
            "definition_list" => self.definition_list(node, width)?,
            "field_list" => self.field_list(node, width)?,
            "block_quote" => self.block_quote(node, width, self.ind())?,
            "transition" => vec!["----".to_string()],
            "comment" => self.comment(node),
            "directive" => self.directive(node, width, "")?,
            "substitution_definition" => self.substitution(node, width)?,
            "target" => self.target(node),
            "footnote" | "citation" => self.footnote(node, width)?,
            _ => self.verbatim(node),
        })
    }

    /// `node`'s source lines, its first line from its start column and the
    /// others relative to it, re-indentable as a unit.
    fn verbatim_lines(&self, node: Node) -> Vec<String> {
        let start = node.start_position();
        let mut end_row = node.end_position().row;
        if node.end_position().column == 0 && end_row > start.row {
            end_row -= 1;
        }
        let first = expand_tabs(self.line(start.row));
        let start_col = col(&expand_tabs(&self.line(start.row)[..start.column]));
        let rest: Vec<String> = (start.row + 1..=end_row)
            .map(|r| expand_tabs(self.line(r)).trim_end().to_string())
            .collect();
        let common = rest
            .iter()
            .filter(|l| !l.is_empty())
            .map(|l| leading_spaces(l))
            .min()
            .unwrap_or(start_col)
            .min(start_col);
        // A node that starts its line keeps its first line's indentation
        // relative to the others; one after a marker on its first line
        // (a field body) is aligned with them.
        let hung = !self.line(start.row)[..start.column].trim().is_empty();
        let lead = if hung { 0 } else { start_col - common };
        let mut out = vec![format!(
            "{}{}",
            " ".repeat(lead),
            first
                .get(start_col..)
                .unwrap_or(first.trim_start())
                .trim_end()
        )];
        out.extend(rest.into_iter().map(|l| {
            if l.is_empty() {
                l
            } else {
                l[common..].to_string()
            }
        }));
        while out.last().is_some_and(|l| l.is_empty()) {
            out.pop();
        }
        out
    }

    fn verbatim(&self, node: Node) -> Vec<String> {
        self.verbatim_lines(node)
    }

    /// The words of `range`: split on ASCII whitespace, except after an
    /// escaping backslash and inside runs of spaces in inline literals.
    fn words(&self, start: usize, end: usize, node: Node) -> Vec<String> {
        let mut protected = Vec::new();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if n.kind() == "literal" {
                protected.push(n.byte_range());
            } else {
                stack.extend(children(n));
            }
        }
        let text = &self.src[start..end];
        let bytes = text.as_bytes();
        let is_space = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
        let mut words = Vec::new();
        let mut word = String::new();
        let mut i = 0;
        while i < bytes.len() {
            if !is_space(bytes[i]) {
                let ch = text[i..].chars().next().unwrap();
                word.push(ch);
                i += ch.len_utf8();
                continue;
            }
            let run_end = i + bytes[i..].iter().take_while(|b| is_space(**b)).count();
            let run = &text[i..run_end];
            let at = start + i;
            let in_literal = protected.iter().any(|r| r.start < at && at < r.end);
            let escaped = word.len() - word.trim_end_matches('\\').len();
            if in_literal && run.len() > 1 && run.bytes().all(|b| b == b' ') {
                word.push_str(run);
            } else {
                let mut sep = run;
                if escaped % 2 == 1 && bytes[i] == b' ' {
                    word.push(' ');
                    sep = &run[1..];
                }
                if !sep.is_empty() && !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            i = run_end;
        }
        if !word.is_empty() {
            words.push(word);
        }
        words
    }

    fn section(&self, node: Node) -> Vec<String> {
        let title = child_of_kind(node, "title").expect("section has a title");
        let text = self
            .words(title.start_byte(), title.end_byte(), title)
            .join(" ");
        let style = self.section_style(node).expect("checked in assign_levels");
        let level = self.levels[&style];
        let (ch, over) = SECTION_STYLES[level - 1];
        let width = col(&text);
        if over {
            let rule = ch.to_string().repeat(width + 2);
            vec![rule.clone(), format!(" {text}"), rule]
        } else {
            vec![text, ch.to_string().repeat(width.max(1))]
        }
    }

    /// A paragraph kept as written: it holds a table or a blank line, or a
    /// line that reads as an adornment.
    fn keep_paragraph(&self, node: Node) -> bool {
        let lines = self.verbatim_lines(node);
        let blank = lines.iter().any(|l| l.trim().is_empty());
        let marked = lines.iter().any(|l| is_table_border(l))
            || lines.iter().skip(1).any(|l| is_adornment_line(l.trim()))
            || lines.first().is_some_and(|l| starts_like_block(l));
        (blank && !self.has_literal(node)) || marked
    }

    fn has_literal(&self, node: Node) -> bool {
        child_of_kind(node, "literal_block").is_some()
    }

    /// A paragraph's words, and its literal block. `Para::` gives `Para:`,
    /// `Para ::` and `Para: ::` give `Para` and `Para:`.
    fn paragraph_parts<'t>(&self, node: Node<'t>) -> (Vec<String>, Option<Node<'t>>) {
        let literal = child_of_kind(node, "literal_block");
        let marker = children(node)
            .into_iter()
            .find(|c| !c.is_named() && c.kind() == "::" && literal.is_some());
        let end = marker.map_or(node.end_byte(), |m| m.start_byte());
        let mut words = self.words(node.start_byte(), end, node);
        if let (Some(m), Some(last)) = (marker, words.last_mut()) {
            let attached = m.start_byte() > node.start_byte()
                && !self.src.as_bytes()[m.start_byte() - 1].is_ascii_whitespace();
            if attached {
                last.push(':');
            }
        }
        (words, literal)
    }

    fn paragraph(&self, node: Node, width: usize, first: Option<usize>) -> Vec<String> {
        if self.keep_paragraph(node) {
            return self.verbatim(node);
        }
        let (words, literal) = self.paragraph_parts(node);
        let mut out = wrap(&words, first.unwrap_or(width).max(1), width);
        if let Some(lit) = literal {
            if !out.is_empty() {
                out.push(String::new());
            }
            out.extend(self.literal(lit));
        }
        out
    }

    fn literal_content(&self, node: Node) -> Vec<String> {
        let start = node.start_position().row;
        let mut end = node.end_position().row;
        if node.end_position().column == 0 && end > start {
            end -= 1;
        }
        dedent((start..=end).map(|r| self.line(r).to_string()).collect())
    }

    /// A literal block as `::`, a blank line and the content.
    fn literal(&self, node: Node) -> Vec<String> {
        let mut out = vec!["::".to_string(), String::new()];
        out.extend(indent_lines(self.literal_content(node), self.ind()));
        out
    }

    fn list(
        &self,
        node: Node,
        width: usize,
        seq: &mut Sequence,
    ) -> Result<Vec<String>, MoldyError> {
        let items: Vec<Node> = children(node)
            .into_iter()
            .filter(|c| c.kind() == "list_item")
            .collect();
        let markers = self.markers(node, &items, seq)?;
        let mut out = Vec::new();
        for (i, (item, marker)) in items.iter().zip(&markers).enumerate() {
            let by = col(marker) + 1;
            let body = match child_of_kind(*item, "body") {
                Some(b) => self.container(b, width.saturating_sub(by))?,
                None => Vec::new(),
            };
            out.extend(hang(marker, body, by));
            let blocks = child_of_kind(*item, "body").map_or(0, |b| self.output_blocks(b));
            if blocks > 1 && i + 1 < items.len() {
                out.push(String::new());
            }
        }
        Ok(out)
    }

    /// Whether `list` continues `prev`, the list before it in the same
    /// container, as docutils reads them (one list split by the grammar).
    fn continues(&self, list: Node, prev: Node, seq: &Sequence) -> bool {
        if list.kind() != prev.kind() {
            return false;
        }
        let items: Vec<Node> = children(list)
            .into_iter()
            .filter(|c| c.kind() == "list_item")
            .collect();
        let Some(first) = self.raw_markers(&items).first().copied() else {
            return false;
        };
        if list.kind() == "bullet_list" {
            return seq.bullet.is_some_and(|(src, _)| first.starts_with(src));
        }
        let (text, punct) = split_enumerator(first);
        seq.numbering.as_ref().is_some_and(|p| {
            p.punct == punct
                && (text == "#" || (!p.auto && enum_value(text, p.style) == Some(p.next)))
        })
    }

    /// Whether a list's last item holds at most one block, so another item
    /// may follow it without a blank line.
    fn last_item_tight(&self, list: Node) -> bool {
        children(list)
            .into_iter()
            .rfind(|c| c.kind() == "list_item")
            .is_some_and(|item| {
                child_of_kind(item, "body").is_none_or(|b| self.output_blocks(b) <= 1)
            })
    }

    /// Each item's marker text in the source.
    fn raw_markers<'n>(&self, items: &[Node<'n>]) -> Vec<&'a str> {
        items
            .iter()
            .map(|item| {
                children(*item)
                    .into_iter()
                    .find(|c| c.kind() == "bullet")
                    .map_or("", |b| self.text(b))
            })
            .collect()
    }

    /// The output markers of a list's items. Bullets become `-`, or `*`
    /// for a list right after a separate one (which used a different
    /// bullet); enumerated lists count up from their first value, with `.`.
    fn markers(
        &self,
        list: Node,
        items: &[Node],
        seq: &mut Sequence,
    ) -> Result<Vec<String>, MoldyError> {
        if list.kind() == "bullet_list" {
            let raw = self.raw_markers(items);
            let src = raw.first().and_then(|m| m.chars().next()).unwrap_or('-');
            if raw.iter().any(|m| !m.starts_with(src)) {
                return Err(unverified(self.err_row(list)));
            }
            let out = match seq.bullet {
                Some((prev, out)) if prev == src => out,
                Some((_, '-')) => '*',
                _ => '-',
            };
            seq.bullet = Some((src, out));
            return Ok(vec![out.to_string(); items.len()]);
        }
        Ok(self
            .enumeration(list, items, seq)?
            .into_iter()
            .map(|t| format!("{t}."))
            .collect())
    }

    /// The enumerator text of each item, refusing a list docutils would not
    /// read as one: mixed punctuation, values out of sequence, or an
    /// explicit value after `#`. A list that continues the previous one (as
    /// docutils reads it) keeps counting from it. A separate list that the
    /// renumbered output would join to the previous one is refused.
    fn enumeration(
        &self,
        list: Node,
        items: &[Node],
        seq: &mut Sequence,
    ) -> Result<Vec<String>, MoldyError> {
        let refuse = || unverified(self.err_row(list));
        let raw = self.raw_markers(items);
        let parts: Vec<(&str, &str)> = raw.iter().map(|m| split_enumerator(m)).collect();
        if parts.iter().any(|p| p.1 != parts[0].1) {
            return Err(refuse());
        }
        let first = parts[0].0;
        let prev = seq.numbering.take();
        let continued = prev.as_ref().filter(|p| {
            p.punct == parts[0].1
                && (first == "#" || (!p.auto && enum_value(first, p.style) == Some(p.next)))
        });
        let (style, start, mut auto) = match continued {
            Some(p) => (p.style, p.next, p.auto),
            None => {
                let (style, start) =
                    enum_style(first, parts.get(1).map(|p| p.0)).ok_or_else(refuse)?;
                if prev
                    .as_ref()
                    .is_some_and(|p| p.style == style && p.next == start)
                {
                    return Err(refuse());
                }
                (style, start, false)
            }
        };
        let mut out = Vec::new();
        for (i, (text, _)) in parts.iter().enumerate() {
            let value = start + i;
            if *text == "#" {
                auto = true;
            } else if i > 0 && (auto || enum_value(text, style) != Some(value)) {
                return Err(refuse());
            }
            if matches!(style, Enum::LowerAlpha | Enum::UpperAlpha) && value > 26 {
                return Err(refuse());
            }
            out.push(enum_text(value, style));
        }
        seq.numbering = Some(Numbering {
            style,
            punct: parts[0].1.to_string(),
            next: start + items.len(),
            auto,
        });
        Ok(out)
    }

    /// A definition list kept as written: a term spans lines, or starts like
    /// another block (the grammar reads an indented continuation of a list
    /// item as a definition).
    fn keep_definition_list(&self, node: Node) -> bool {
        children(node)
            .into_iter()
            .filter(|c| c.kind() == "list_item")
            .flat_map(children)
            .filter(|c| matches!(c.kind(), "term" | "classifier"))
            .any(|c| {
                c.start_position().row != c.end_position().row
                    || (c.kind() == "term" && starts_like_block(self.text(c)))
            })
    }

    /// Whether a paragraph in `node` starts like another block: the grammar
    /// reads lists inside block quotes as paragraphs.
    fn has_misread_paragraph(&self, node: Node) -> bool {
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if n.kind() == "paragraph" {
                if self
                    .verbatim_lines(n)
                    .first()
                    .is_some_and(|l| starts_like_block(l))
                {
                    return true;
                }
            } else {
                stack.extend(children(n));
            }
        }
        false
    }

    /// Explicit markup whose text does not start on the `..` line: docutils
    /// reads it as a comment (`..` alone, then `   name:: x`).
    fn explicit_misread(&self, node: Node) -> bool {
        if !matches!(
            node.kind(),
            "directive" | "footnote" | "citation" | "target" | "substitution_definition"
        ) {
            return false;
        }
        let kids = children(node);
        match (kids.first(), kids.get(1)) {
            (Some(dots), Some(next)) if dots.kind() == ".." => {
                next.start_position().row != dots.start_position().row
            }
            _ => false,
        }
    }

    /// How many blocks a container's output has: `Para::` with its
    /// literal block is written as two.
    /// List fragments the grammar split from one list count once.
    fn output_blocks(&self, node: Node) -> usize {
        let mut seq = Sequence::default();
        let mut prev: Option<Node> = None;
        let mut count = 0;
        for b in self.blocks(node) {
            let continued = prev.is_some_and(|p| self.continues(b, p, &seq));
            seq.reset_unless(b);
            if matches!(b.kind(), "bullet_list" | "enumerated_list") {
                let items: Vec<Node> = children(b)
                    .into_iter()
                    .filter(|c| c.kind() == "list_item")
                    .collect();
                let _ = self.markers(b, &items, &mut seq);
            }
            prev = Some(b);
            if continued {
                continue;
            }
            let split = b.kind() == "paragraph"
                && !self.keep_paragraph(b)
                && self.has_literal(b)
                && !self.paragraph_parts(b).0.is_empty();
            count += if split { 2 } else { 1 };
        }
        count
    }

    fn term_line(&self, item: Node) -> String {
        let mut parts = Vec::new();
        for c in children(item) {
            match c.kind() {
                "term" | "classifier" => {
                    parts.push(self.words(c.start_byte(), c.end_byte(), c).join(" "))
                }
                _ => {}
            }
        }
        parts.join(" : ")
    }

    fn definition_list(&self, node: Node, width: usize) -> Result<Vec<String>, MoldyError> {
        let items: Vec<Node> = children(node)
            .into_iter()
            .filter(|c| c.kind() == "list_item")
            .collect();
        if self.keep_definition_list(node) {
            return Ok(self.verbatim(node));
        }
        let ind = self.ind();
        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push(String::new());
            }
            out.push(self.term_line(*item));
            if let Some(def) = child_of_kind(*item, "definition") {
                out.extend(indent_lines(
                    self.container(def, width.saturating_sub(ind))?,
                    ind,
                ));
            }
        }
        Ok(out)
    }

    /// `:name:` with its body, the body's first paragraph starting on the
    /// name line.
    fn named_body(
        &self,
        name: &str,
        body: Option<Node>,
        width: usize,
    ) -> Result<Vec<String>, MoldyError> {
        let ind = self.ind();
        let Some(body) = body else {
            return Ok(vec![name.to_string()]);
        };
        let blocks = self.blocks(body);
        if blocks.is_empty() {
            return Ok(vec![name.to_string()]);
        }
        let inner = width.saturating_sub(ind);
        if blocks[0].kind() == "paragraph" && !self.keep_paragraph(blocks[0]) {
            let first = width.saturating_sub(col(name) + 1);
            let lines = self.container_first(body, inner, Some(first))?;
            Ok(hang(name, lines, ind))
        } else {
            let mut out = vec![name.to_string()];
            out.extend(indent_lines(self.container(body, inner)?, ind));
            Ok(out)
        }
    }

    fn field_list(&self, node: Node, width: usize) -> Result<Vec<String>, MoldyError> {
        let mut out = Vec::new();
        for field in children(node).into_iter().filter(|c| c.kind() == "field") {
            out.extend(self.field(field, width)?);
        }
        Ok(out)
    }

    fn field(&self, field: Node, width: usize) -> Result<Vec<String>, MoldyError> {
        let name = child_of_kind(field, "field_name")
            .map(|n| self.words(n.start_byte(), n.end_byte(), n).join(" "))
            .unwrap_or_default();
        self.named_body(
            &format!(":{name}:"),
            child_of_kind(field, "field_body"),
            width,
        )
    }

    /// A block quote's indentation: `indent_width`, or after anything but a
    /// plain paragraph, section or transition its source indentation in the
    /// container (`base`). After a list, directive or literal block, more
    /// indentation would join the quote to it, and the grammar ends some of
    /// those early (at a literal block in a list item, or a directive whose
    /// arguments continue on the next line).
    fn quote_indent(&self, quote: Node, prev: Option<Node>, base: usize) -> usize {
        let plain = prev.is_none_or(|p| {
            matches!(p.kind(), "section" | "transition")
                || (p.kind() == "paragraph" && !self.has_literal(p))
        });
        if plain {
            return self.ind();
        }
        let start = quote.start_position();
        let column = col(&expand_tabs(&self.line(start.row)[..start.column]));
        column.saturating_sub(base).max(1)
    }

    /// The column a container's blocks start at.
    fn base_column(&self, node: Node) -> usize {
        if node.kind() == "document" {
            return 0;
        }
        let start = node.start_position();
        col(&expand_tabs(&self.line(start.row)[..start.column]))
    }

    fn block_quote(&self, node: Node, width: usize, ind: usize) -> Result<Vec<String>, MoldyError> {
        if self.has_misread_paragraph(node) {
            return Ok(indent_lines(self.verbatim(node), ind));
        }
        let mut out = self.container(node, width.saturating_sub(ind))?;
        if let Some(attr) = child_of_kind(node, "attribution") {
            let words = self.words(attr.start_byte(), attr.end_byte(), attr);
            out.push(String::new());
            out.push(words.join(" "));
        }
        Ok(indent_lines(out, ind))
    }

    /// A comment's text lines: the first line's text, then the following
    /// lines relative to each other.
    fn comment_lines(&self, node: Node) -> Vec<String> {
        let lines = self.verbatim_lines(node);
        let first = lines[0]
            .strip_prefix("..")
            .unwrap_or(&lines[0])
            .trim()
            .to_string();
        let mut out = Vec::new();
        let rest = lines[1..].to_vec();
        if !first.is_empty() {
            out.push(first);
            let blanks = rest.iter().take_while(|l| l.trim().is_empty()).count();
            let body = dedent(rest);
            if !body.is_empty() {
                out.extend(std::iter::repeat_n(String::new(), blanks));
            }
            out.extend(body);
        } else {
            out.extend(dedent(rest));
        }
        out
    }

    fn comment(&self, node: Node) -> Vec<String> {
        let lines = self.comment_lines(node);
        match lines.len() {
            0 => vec!["..".to_string()],
            1 if self.verbatim_lines(node).len() == 1 => vec![format!(".. {}", lines[0])],
            _ => {
                let mut out = vec!["..".to_string()];
                out.extend(indent_lines(lines, self.ind()));
                out
            }
        }
    }

    fn target(&self, node: Node) -> Vec<String> {
        let lines = self.verbatim_lines(node);
        if lines.len() > 1 {
            return lines;
        }
        vec![self
            .words(node.start_byte(), node.end_byte(), node)
            .join(" ")]
    }

    fn footnote(&self, node: Node, width: usize) -> Result<Vec<String>, MoldyError> {
        let label = child_of_kind(node, "label").map_or("", |l| self.text(l));
        self.named_body(&format!(".. {label}"), child_of_kind(node, "body"), width)
    }

    fn substitution(&self, node: Node, width: usize) -> Result<Vec<String>, MoldyError> {
        let name = child_of_kind(node, "substitution").map_or("", |s| self.text(s));
        match child_of_kind(node, "directive") {
            Some(d) => self.directive(d, width, &format!("{name} ")),
            None => Ok(self.verbatim(node)),
        }
    }

    /// The `.. name::` line, with `prefix` (a substitution) before the name
    /// and `args` after it.
    fn directive_header(&self, node: Node, prefix: &str, args: Option<&str>) -> String {
        let name = child_of_kind(node, "type").map_or("", |t| self.text(t));
        let mut header = format!(".. {prefix}{name}::");
        if let Some(a) = args.filter(|a| !a.is_empty()) {
            header.push(' ');
            header.push_str(a);
        }
        header
    }

    fn directive_args(&self, node: Node) -> Option<String> {
        let body = child_of_kind(node, "body")?;
        let args = child_of_kind(body, "arguments")?;
        Some(
            self.words(args.start_byte(), args.end_byte(), args)
                .join(" "),
        )
    }

    /// The directive's option lines, normalized, if each option is on one
    /// line directly after the header.
    fn directive_options(&self, node: Node) -> Option<(Vec<String>, usize)> {
        let body = child_of_kind(node, "body");
        let Some(options) = body.and_then(|b| child_of_kind(b, "options")) else {
            return Some((Vec::new(), node.start_position().row));
        };
        if options.start_position().row != node.start_position().row + 1 {
            return None;
        }
        let mut out = Vec::new();
        let mut last = node.start_position().row;
        for field in children(options)
            .into_iter()
            .filter(|c| c.kind() == "field")
        {
            if field.start_position().row != field.end_position().row {
                return None;
            }
            let words = self.words(field.start_byte(), field.end_byte(), field);
            out.push(words.join(" "));
            last = field.end_position().row;
        }
        Some((out, last))
    }

    /// The directive's lines after `after_row`, dedented.
    fn rows_after(&self, node: Node, after_row: usize) -> Vec<String> {
        let mut end = node.end_position().row;
        if node.end_position().column == 0 && end > node.start_position().row {
            end -= 1;
        }
        (after_row + 1..=end)
            .map(|r| self.line(r).to_string())
            .collect()
    }

    /// Whether `node`'s content is formatted as a document: an admonition
    /// (its first-line text starts the body) or a titled directive.
    fn rst_body(&self, node: Node, prefix: &str) -> Option<bool> {
        if !prefix.is_empty() {
            return None;
        }
        let name = child_of_kind(node, "type").map(|t| self.text(t).to_ascii_lowercase())?;
        if ADMONITIONS.contains(&name.as_str()) {
            Some(true)
        } else if TITLED.contains(&name.as_str()) {
            Some(false)
        } else {
            None
        }
    }

    /// The text of an RST-bodied directive's content as its own document,
    /// and the row it starts on, or `None` if its layout is unusual.
    fn inner_document(&self, node: Node, args_in_body: bool) -> Option<(String, usize)> {
        let (options, last) = self.directive_options(node)?;
        let args = self.directive_args(node);
        if args_in_body && args.is_some() && !options.is_empty() {
            return None;
        }
        let mut lines = Vec::new();
        if args_in_body {
            if let Some(a) = &args {
                lines.push(a.clone());
            }
        }
        let rest = self.rows_after(node, last);
        // Without a blank line after the header and options, the next lines
        // continue the arguments.
        if !args_in_body && rest.first().is_some_and(|l| !l.trim().is_empty()) {
            return None;
        }
        if args_in_body && args.is_some() {
            // Lines right after the header continue the first paragraph.
            lines.extend(dedent_keep_lead(rest));
        } else {
            lines.extend(dedent(rest));
        }
        let row = self.row + node.start_position().row;
        Some((lines.join("\n") + "\n", row))
    }

    fn directive(&self, node: Node, width: usize, prefix: &str) -> Result<Vec<String>, MoldyError> {
        let ind = self.ind();
        let inner_width = width.saturating_sub(ind).max(1);
        if let Some(args_in_body) = self.rst_body(node, prefix) {
            if let Some((inner, row)) = self.inner_document(node, args_in_body) {
                let args = if args_in_body {
                    None
                } else {
                    self.directive_args(node)
                };
                let (options, _) = self.directive_options(node).unwrap_or_default();
                let body = if inner.trim().is_empty() {
                    Some(Vec::new())
                } else {
                    match render(&inner, self.cfg, inner_width, self.depth + 1, row) {
                        Ok(lines) => Some(lines),
                        Err(MoldyError::TooDeep { language, line }) => {
                            return Err(MoldyError::TooDeep { language, line })
                        }
                        Err(_) => None,
                    }
                };
                if let Some(body) = body {
                    let mut out = vec![self.directive_header(node, prefix, args.as_deref())];
                    out.extend(indent_lines(options, ind));
                    if !body.is_empty() {
                        out.push(String::new());
                        out.extend(indent_lines(body, ind));
                    }
                    return Ok(out);
                }
            }
        }
        Ok(self.directive_as_written(node, prefix))
    }

    /// A directive with its header and options normalized and its content
    /// kept as written, re-indented.
    fn directive_as_written(&self, node: Node, prefix: &str) -> Vec<String> {
        let ind = self.ind();
        let args = self.directive_args(node);
        let mut out = vec![self.directive_header(node, prefix, args.as_deref())];
        let last = match self.directive_options(node) {
            Some((options, last)) => {
                out.extend(indent_lines(options, ind));
                last
            }
            None => node.start_position().row,
        };
        let rest = self.rows_after(node, last);
        let blank_first = rest.first().is_some_and(|l| l.trim().is_empty());
        let content = dedent(rest);
        if !content.is_empty() {
            if blank_first {
                out.push(String::new());
            }
            out.extend(indent_lines(content, ind));
        }
        out
    }

    // ── Shape ────────────────────────────────────────────────────────────
    //
    // A document's structure and text with layout normalized: what
    // formatting must keep. The input's and the output's are compared.

    fn shape_container(&self, node: Node, out: &mut String) {
        self.shape_container_rows(node, out, None);
    }

    fn shape_container_rows(
        &self,
        node: Node,
        out: &mut String,
        mut rows: Option<&mut Vec<(usize, usize)>>,
    ) {
        out.push('{');
        let mut seq = Sequence::default();
        for run in self.runs(node) {
            if let Some(rows) = rows.as_deref_mut() {
                let row = match &run {
                    Run::Block(b) | Run::Joined(b, _) => b.start_position().row,
                    Run::Table((start, _), _) => *start,
                };
                rows.push((out.len(), row));
            }
            match run {
                Run::Table(rows, base) => {
                    seq = Sequence::default();
                    out.push_str(&format!("V {:?}", self.table_lines(rows, base)));
                }
                Run::Joined(markup, quote) => {
                    seq = Sequence::default();
                    out.push_str(&format!("J {:?}", self.joined_lines(markup, quote)));
                }
                Run::Block(block) => {
                    seq.reset_unless(block);
                    self.shape_block(block, out, &mut seq);
                }
            }
            out.push('\n');
        }
        out.push('}');
    }

    fn shape_block(&self, node: Node, out: &mut String, seq: &mut Sequence) {
        let kind = node.kind();
        if self.explicit_misread(node) {
            out.push_str(&format!("C {:?}", self.comment_lines(node)));
            return;
        }
        match kind {
            "document" | "body" | "definition" | "field_body" => self.shape_container(node, out),
            "section" => {
                let level = self
                    .section_style(node)
                    .and_then(|s| self.levels.get(&s))
                    .copied()
                    .unwrap_or(0);
                let title = child_of_kind(node, "title")
                    .map(|t| self.words(t.start_byte(), t.end_byte(), t).join(" "))
                    .unwrap_or_default();
                out.push_str(&format!("S{level} {title}"));
            }
            "paragraph" if self.keep_paragraph(node) => {
                out.push_str(&format!("V {:?}", self.verbatim(node)));
            }
            "paragraph" => {
                let (words, literal) = self.paragraph_parts(node);
                if !words.is_empty() {
                    out.push_str(&format!("P {}", words.join(" ")));
                }
                if let Some(lit) = literal {
                    if !words.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&format!("L {:?}", self.literal_content(lit)));
                }
            }
            "literal_block" => out.push_str(&format!("L {:?}", self.literal_content(node))),
            "bullet_list" | "enumerated_list" => {
                let items: Vec<Node> = children(node)
                    .into_iter()
                    .filter(|c| c.kind() == "list_item")
                    .collect();
                // A list that continues the previous one (the grammar splits
                // a list at blank lines) is part of it.
                let prev_bullet = seq.bullet.map(|b| b.0);
                let prev_numbering = seq.numbering.as_ref().map(|n| (n.style, n.next));
                let markers = self.markers(node, &items, seq).unwrap_or_default();
                let continued = if kind == "bullet_list" {
                    prev_bullet.is_some() && prev_bullet == seq.bullet.map(|b| b.0)
                } else {
                    let now = seq
                        .numbering
                        .as_ref()
                        .map(|n| (n.style, n.next - items.len()));
                    prev_numbering.is_some() && prev_numbering == now
                };
                if continued && out.ends_with('\n') {
                    out.pop();
                } else {
                    out.push_str(kind);
                }
                for (item, marker) in items.iter().zip(&markers) {
                    out.push_str(&format!("[{marker}]"));
                    match child_of_kind(*item, "body") {
                        Some(b) => self.shape_container(b, out),
                        None => out.push_str("{}"),
                    }
                }
            }
            "definition_list" if self.keep_definition_list(node) => {
                out.push_str(&format!("DV {:?}", self.verbatim(node)));
            }
            "definition_list" => {
                // Adjacent definition lists are one list; the grammar splits
                // a list at the blank lines between its items.
                if seq.definitions && out.ends_with('\n') {
                    out.pop();
                } else {
                    out.push('D');
                }
                seq.definitions = true;
                for item in children(node)
                    .into_iter()
                    .filter(|c| c.kind() == "list_item")
                {
                    out.push_str(&format!(" T {}", self.term_line(item)));
                    if let Some(def) = child_of_kind(item, "definition") {
                        self.shape_container(def, out);
                    }
                }
            }
            "field_list" => {
                out.push('F');
                for field in children(node).into_iter().filter(|c| c.kind() == "field") {
                    let name = child_of_kind(field, "field_name")
                        .map(|n| self.words(n.start_byte(), n.end_byte(), n).join(" "))
                        .unwrap_or_default();
                    out.push_str(&format!(" N {name}"));
                    if let Some(body) = child_of_kind(field, "field_body") {
                        self.shape_container(body, out);
                    }
                }
            }
            "block_quote" if self.has_misread_paragraph(node) => {
                out.push_str(&format!("QV {:?}", self.verbatim(node)));
            }
            "block_quote" => {
                out.push('Q');
                self.shape_container(node, out);
                if let Some(attr) = child_of_kind(node, "attribution") {
                    let words = self.words(attr.start_byte(), attr.end_byte(), attr);
                    out.push_str(&format!(" A {}", words.join(" ")));
                }
            }
            "transition" => out.push('R'),
            "comment" => out.push_str(&format!("C {:?}", self.comment_lines(node))),
            "target" => {
                let words = self.words(node.start_byte(), node.end_byte(), node);
                out.push_str(&format!("G {}", words.join(" ")));
            }
            "footnote" | "citation" => {
                let label = child_of_kind(node, "label").map_or("", |l| self.text(l));
                out.push_str(&format!("N {label}"));
                if let Some(body) = child_of_kind(node, "body") {
                    self.shape_container(body, out);
                }
            }
            "substitution_definition" => {
                let name = child_of_kind(node, "substitution").map_or("", |s| self.text(s));
                out.push_str(&format!("U {name}"));
                match child_of_kind(node, "directive") {
                    Some(d) => self.shape_directive(d, "|", out),
                    None => out.push_str(&format!(" {:?}", self.verbatim(node))),
                }
            }
            "directive" => self.shape_directive(node, "", out),
            _ => out.push_str(&format!("V {kind} {:?}", self.verbatim(node))),
        }
    }

    fn shape_directive(&self, node: Node, prefix: &str, out: &mut String) {
        let name = child_of_kind(node, "type").map_or("", |t| self.text(t));
        out.push_str(&format!("X {name}"));
        if let Some(args_in_body) = self.rst_body(node, prefix) {
            if let Some((inner, _)) = self.inner_document(node, args_in_body) {
                if !args_in_body {
                    out.push_str(&format!(" {:?}", self.directive_args(node)));
                }
                let (options, _) = self.directive_options(node).unwrap_or_default();
                out.push_str(&format!(" {options:?}"));
                match document_shape(&inner, self.cfg, self.depth + 1) {
                    Ok((shape, _)) => out.push_str(&shape),
                    Err(_) => out.push_str(&format!(
                        "{:?}",
                        dedent(inner.lines().map(String::from).collect())
                    )),
                }
                return;
            }
        }
        out.push_str(&format!("{:?}", self.directive_as_written(node, prefix)));
    }
}

/// `lines` without their common indentation, keeping a leading
/// non-blank line in place (a continuation of the first-line text).
fn dedent_keep_lead(lines: Vec<String>) -> Vec<String> {
    let lead_blank = lines.first().is_some_and(|l| l.trim().is_empty());
    let mut out = dedent(lines);
    if lead_blank {
        out.insert(0, String::new());
    }
    out
}
