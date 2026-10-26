// The token check for the C, C++, Rust and Python formatters. These
// formatters continue over syntax errors, so a misread can lead them to
// invent, drop or join tokens. Formatting may change layout only: the
// output is parsed again and its tokens compared with the input's, and a
// file whose tokens differ is declined. The only token edits allowed are
// the ones a formatting option makes, listed in `Edits`.

use similar::{capture_diff_slices_deadline, Algorithm, DiffOp};
use std::time::{Duration, Instant};
use tree_sitter::Node;

use crate::error::MoldyError;

/// The tokens a formatter may insert or delete, by the grammar's kind.
pub(super) struct Edits {
    pub insert: &'static [&'static str],
    pub delete: &'static [&'static str],
    /// Whether a trailing comma that changes nothing may be added or
    /// removed; see `is_optional_comma`.
    pub trailing_commas: bool,
}

impl Edits {
    pub const NONE: Edits = Edits {
        insert: &[],
        delete: &[],
        trailing_commas: false,
    };
}

/// The end of a logical line, where a line break is part of the code: a
/// C preprocessor line, or any Python line outside brackets. Real tokens
/// never are, or start with, whitespace, so these cannot match one.
const NEWLINE: &str = "\n";
const INDENT: &str = " indent";
const DEDENT: &str = " dedent";
/// A Python line that dedents to no enclosing indentation.
const MISALIGNED: &str = " misaligned";

/// How long the token diff may run before it settles for a coarser one.
/// A coarser diff can only decline more files, never fewer.
const DIFF_DEADLINE: Duration = Duration::from_secs(2);

/// `MoldyError::Unverified` unless `output` has the tokens of `source`,
/// apart from the `edits` allowed, and has no syntax error unless `source`
/// has one. Both are parsed with `language_key`'s grammar; tokens the
/// parser invented to recover from an error (zero width) are not part of
/// either text and are ignored. Comments may move past code but must stay
/// in order.
pub(super) fn check_tokens(
    language: &str,
    language_key: &str,
    source: &str,
    output: &str,
    edits: &Edits,
) -> Result<(), MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key(language_key)
        .ok_or_else(|| MoldyError::UnsupportedLanguage(language_key.to_string()))?;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;
    let mut parse = |text: &str| {
        parser
            .parse(text, None)
            .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))
    };
    let layout = match language_key {
        "c" | "cpp" => Layout::Directives,
        "python" => Layout::Indented,
        _ => Layout::Free,
    };
    let source_tree = parse(source)?;
    let output_tree = parse(output)?;
    let (mut before, before_comments) = tokens(source_tree.root_node(), source, layout);
    let (mut after, after_comments) = tokens(output_tree.root_node(), output, layout);
    // A comma that changes nothing may come or go, so it is not compared.
    // Each side's own tree says which commas those are.
    if edits.trailing_commas {
        before.retain(|t| !t.optional_comma);
        after.retain(|t| !t.optional_comma);
    }
    let unverified = |row: usize| MoldyError::Unverified {
        language: language.to_string(),
        line: row + 1,
    };

    let moved = (0..before_comments.len().max(after_comments.len())).find(|&i| {
        before_comments.get(i).map(|t| &t.text) != after_comments.get(i).map(|t| &t.text)
    });
    if let Some(i) = moved {
        return Err(unverified(row_at(&before_comments, i)));
    }

    let old: Vec<&str> = before.iter().map(|t| t.text.as_str()).collect();
    let new: Vec<&str> = after.iter().map(|t| t.text.as_str()).collect();
    let deadline = Instant::now() + DIFF_DEADLINE;
    for op in capture_diff_slices_deadline(Algorithm::Myers, &old, &new, Some(deadline)) {
        let (old_index, deleted, inserted) = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index, old_len, ..
            } => (
                old_index,
                &before[old_index..old_index + old_len],
                &after[..0],
            ),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (
                old_index,
                &before[..0],
                &after[new_index..new_index + new_len],
            ),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (
                old_index,
                &before[old_index..old_index + old_len],
                &after[new_index..new_index + new_len],
            ),
        };
        let allowed = deleted.iter().all(|t| edits.delete.contains(&t.kind))
            && inserted.iter().all(|t| edits.insert.contains(&t.kind));
        if !allowed {
            return Err(unverified(row_at(&before, old_index)));
        }
    }
    // The same tokens can still read differently, if the output breaks
    // them into lines the grammar cannot read. The input token at the
    // index of the output's first token past the error is near it.
    if !source_tree.root_node().has_error() && output_tree.root_node().has_error() {
        let error_row = first_error_row(output_tree.root_node());
        let i = after.iter().position(|t| t.row >= error_row);
        return Err(unverified(row_at(&before, i.unwrap_or(before.len()))));
    }
    Ok(())
}

/// The row of the first `ERROR` or `MISSING` node under `root`.
fn first_error_row(root: Node) -> usize {
    let mut node = root;
    'down: loop {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.is_error() || child.is_missing() {
                return child.start_position().row;
            }
            if child.has_error() {
                node = child;
                continue 'down;
            }
        }
        return node.start_position().row;
    }
}

/// The row of `tokens[i]`, or of the last token when `i` is past the end.
fn row_at(tokens: &[Token], i: usize) -> usize {
    tokens.get(i).or(tokens.last()).map_or(0, |t| t.row)
}

/// Where a line break is part of the code.
#[derive(Clone, Copy)]
enum Layout {
    /// Nowhere (Rust).
    Free,
    /// At the end of a C preprocessor line.
    Directives,
    /// At the end of every logical line, and indentation too (Python).
    Indented,
}

struct Token {
    text: String,
    /// The grammar's kind for a leaf, which tells a bracket from the same
    /// text in a string; empty for other tokens.
    kind: &'static str,
    row: usize,
    col: usize,
    end_row: usize,
    /// A trailing comma that changes nothing; see `Edits::trailing_commas`.
    optional_comma: bool,
    /// A Python `:` that a block follows.
    opens_block: bool,
}

/// The code tokens of the tree at `root`, in order, and its comments.
/// Code tokens are its leaves, a preprocessor directive's text split into
/// words, any text no leaf covers, split at whitespace, and the line
/// structure `layout` gives. Whitespace inside a comment, and carriage
/// returns, are not compared.
fn tokens(root: Node, source: &str, layout: Layout) -> (Vec<Token>, Vec<Token>) {
    let mut code = Vec::new();
    let mut comments = Vec::new();
    push_tokens(root, source, &mut code, &mut comments);
    let code = match layout {
        Layout::Free => code,
        Layout::Directives => mark_directive_ends(code, source),
        Layout::Indented => mark_logical_lines(code),
    };
    (code, comments)
}

fn push_tokens(node: Node, source: &str, out: &mut Vec<Token>, comments: &mut Vec<Token>) {
    if node.start_byte() == node.end_byte() {
        return;
    }
    let start = node.start_position();
    if node.kind().ends_with("comment") {
        let text = source[node.byte_range()]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        comments.push(token(text, start, node.end_position().row));
        return;
    }
    if is_literal(node.kind()) {
        let text = source[node.byte_range()].replace('\r', "");
        out.push(token(text, start, node.end_position().row));
        return;
    }
    if node.kind() == "preproc_arg" {
        push_words(&source[node.byte_range()], start.row, out);
        return;
    }
    if node.child_count() == 0 {
        let text = source[node.byte_range()].replace('\r', "");
        if !text.trim().is_empty() {
            let mut leaf = token(text, start, node.end_position().row);
            leaf.kind = node.kind();
            leaf.optional_comma = is_optional_comma(node);
            leaf.opens_block =
                node.kind() == ":" && node.next_sibling().is_some_and(|n| n.kind() == "block");
            out.push(leaf);
        }
        return;
    }
    let (mut at, mut at_point) = (node.start_byte(), start);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        push_gap(source, at, at_point, child.start_byte(), out);
        push_tokens(child, source, out, comments);
        if child.end_byte() > at {
            (at, at_point) = (child.end_byte(), child.end_position());
        }
    }
    push_gap(source, at, at_point, node.end_byte(), out);
}

fn token(text: String, start: tree_sitter::Point, end_row: usize) -> Token {
    Token {
        text,
        row: start.row,
        col: start.column,
        end_row,
        optional_comma: false,
        opens_block: false,
        kind: "",
    }
}

/// Whether a node of `kind` is a string or character literal, compared
/// as one token, exactly. Python's holds f-string code too, which the
/// formatter keeps as written.
fn is_literal(kind: &str) -> bool {
    matches!(
        kind,
        "string" | "string_literal" | "raw_string_literal" | "char_literal" | "system_lib_string"
    )
}

/// Whether `node` is a trailing comma that changes nothing: before a
/// closing bracket or at the end of a Rust `where` clause, outside a
/// macro's token tree, and not the comma that makes a one-element tuple
/// (`(a,)`, or a Python subscript `x[a,]`).
fn is_optional_comma(node: Node) -> bool {
    if node.kind() != "," || node.is_missing() {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    let kind = parent.kind();
    let mut next = node.next_sibling();
    while next.is_some_and(|n| n.kind().ends_with("comment")) {
        next = next.and_then(|n| n.next_sibling());
    }
    // A Rust `where` clause ends at the `{` or `;` after it.
    let closes = match next {
        Some(next) => matches!(next.kind(), ")" | "]" | "}" | ">"),
        None => kind == "where_clause",
    };
    let one_tuple = kind.starts_with("tuple") && parent.named_child_count() <= 1;
    closes && !one_tuple && !matches!(kind, "token_tree" | "subscript") && !parent.is_error()
}

/// Text from `start`, at `point`, to `end` that no leaf covers, split at
/// whitespace.
fn push_gap(
    source: &str,
    start: usize,
    point: tree_sitter::Point,
    end: usize,
    out: &mut Vec<Token>,
) {
    if start >= end {
        return;
    }
    for (i, line) in source[start..end].split('\n').enumerate() {
        let line_col = if i == 0 { point.column } else { 0 };
        let mut offset = 0;
        for word in line.split_whitespace() {
            offset += line[offset..].find(word).unwrap_or(0);
            let row = point.row + i;
            // A `\` that ends a line, which the grammar leaves out of the
            // tree in places, continues it.
            let ends_line = line[offset + word.len()..].trim().is_empty();
            out.push(Token {
                text: word.to_string(),
                row,
                col: line_col + offset,
                end_row: row,
                optional_comma: false,
                opens_block: false,
                kind: if word == "\\" && ends_line {
                    "line_continuation"
                } else {
                    ""
                },
            });
            offset += word.len();
        }
    }
}

/// The words of a preprocessor directive's text, which the grammar keeps
/// as one token: split at whitespace and line continuations, except inside
/// a string or character literal.
fn push_words(text: &str, row: usize, out: &mut Vec<Token>) {
    let mut word = String::new();
    let mut quote = None;
    let mut chars = text.chars().filter(|&c| c != '\r').peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                word.push(c);
                if c == '\\' {
                    word.extend(chars.next());
                } else if c == q || c == '\n' {
                    quote = None;
                }
            }
            None if c == '\\' && chars.peek() == Some(&'\n') => {
                chars.next();
                flush_word(&mut word, row, out);
            }
            None if c.is_whitespace() => flush_word(&mut word, row, out),
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                }
                word.push(c);
            }
        }
    }
    flush_word(&mut word, row, out);
}

fn flush_word(word: &mut String, row: usize, out: &mut Vec<Token>) {
    if !word.is_empty() {
        out.push(Token {
            text: std::mem::take(word),
            row,
            col: 0,
            end_row: row,
            optional_comma: false,
            opens_block: false,
            kind: "",
        });
    }
}

fn layout_token(text: &str, row: usize) -> Token {
    Token {
        text: text.to_string(),
        row,
        col: 0,
        end_row: row,
        optional_comma: false,
        opens_block: false,
        kind: "",
    }
}

/// `tokens` with a `NEWLINE` after each preprocessor line: one whose
/// first token starts with `#`, through its line continuations. A line
/// break there ends the directive, so one moved there changes the code.
fn mark_directive_ends(tokens: Vec<Token>, source: &str) -> Vec<Token> {
    // The row each row's logical line starts on.
    let mut logical = Vec::new();
    let mut continued = false;
    for (row, line) in source.split('\n').enumerate() {
        let start = if continued { logical[row - 1] } else { row };
        logical.push(start);
        continued = line.trim_end_matches('\r').ends_with('\\');
    }
    let mut out = Vec::with_capacity(tokens.len());
    let mut directive: Option<usize> = None;
    let mut previous = None;
    for token in tokens {
        let line = logical.get(token.row).copied().unwrap_or(token.row);
        if let Some(row) = directive.filter(|&d| d != line) {
            out.push(layout_token(NEWLINE, row));
            directive = None;
        }
        if previous != Some(line) && token.text.starts_with('#') {
            directive = Some(line);
        }
        previous = Some(line);
        out.push(token);
    }
    if let Some(row) = directive {
        out.push(layout_token(NEWLINE, row));
    }
    out
}

/// `tokens` with Python's line structure: a `NEWLINE` before each logical
/// line, followed by the `INDENT` or `DEDENT`s its indentation makes, as
/// Python's tokenizer reads them. A logical line starts on a new row,
/// outside brackets and after no line continuation. Two forms that read
/// the same are made the same: statements joined by `;` are separate
/// lines, and a block on the line of its `:` (`if a: b`) is indented.
fn mark_logical_lines(tokens: Vec<Token>) -> Vec<Token> {
    // Indentation columns; `None` for a block on the line of its `:`.
    let mut levels: Vec<Option<usize>> = vec![Some(0)];
    let mut out = Vec::with_capacity(tokens.len());
    let mut brackets = 0usize;
    let mut previous: Option<(usize, bool)> = None;
    // A `;` or block `:` seen; what follows on its line starts a statement.
    let mut split: Option<bool> = None;
    for token in tokens {
        let new_line = previous
            .is_some_and(|(end_row, continues)| brackets == 0 && token.row > end_row && !continues);
        if new_line {
            out.push(layout_token(NEWLINE, token.row));
            while levels.last() == Some(&None) {
                levels.pop();
                out.push(layout_token(DEDENT, token.row));
            }
            let level = levels.last().copied().flatten().unwrap_or(0);
            if token.col > level {
                levels.push(Some(token.col));
                out.push(layout_token(INDENT, token.row));
            }
            while levels.len() > 1 && token.col < levels.last().copied().flatten().unwrap_or(0) {
                levels.pop();
                out.push(layout_token(DEDENT, token.row));
            }
            if levels.last() != Some(&Some(token.col)) {
                out.push(layout_token(MISALIGNED, token.row));
            }
        } else if let Some(opens_block) = split {
            out.push(layout_token(NEWLINE, token.row));
            if opens_block {
                levels.push(None);
                out.push(layout_token(INDENT, token.row));
            }
        }
        split = None;
        previous = Some((token.end_row, token.kind == "line_continuation"));
        match token.kind {
            "(" | "[" | "{" => brackets += 1,
            ")" | "]" | "}" => brackets = brackets.saturating_sub(1),
            ";" if brackets == 0 => {
                split = Some(false);
                continue;
            }
            _ if token.opens_block && brackets == 0 => split = Some(true),
            _ => {}
        }
        out.push(token);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{check_tokens, Edits};

    const BRACES: Edits = Edits {
        insert: &["{", "}"],
        ..Edits::NONE
    };
    const COMMAS: Edits = Edits {
        trailing_commas: true,
        ..Edits::NONE
    };
    const PYTHON: Edits = Edits {
        delete: &["line_continuation"],
        trailing_commas: true,
        ..Edits::NONE
    };

    fn passes(key: &str, edits: &Edits, source: &str, output: &str) -> bool {
        check_tokens("test", key, source, output, edits).is_ok()
    }

    #[test]
    fn layout_changes_pass() {
        assert!(passes(
            "c",
            &Edits::NONE,
            "int  f(void){return 1;}\n",
            "int f(void)\n{\n    return 1;\n}\n"
        ));
        assert!(passes(
            "rust",
            &Edits::NONE,
            "fn f(){g( 1,2 )}\n",
            "fn f() {\n    g(1, 2)\n}\n"
        ));
    }

    #[test]
    fn an_invented_token_is_found() {
        assert!(!passes(
            "c",
            &Edits::NONE,
            "int f(void) { return 1 }\n",
            "int f(void) { return 1; }\n"
        ));
        assert!(!passes(
            "python",
            &PYTHON,
            "def f(:\n    pass\n",
            "def f():\n    pass\n"
        ));
    }

    #[test]
    fn a_dropped_token_is_found() {
        assert!(!passes(
            "c",
            &Edits::NONE,
            "void f(void) { a(); b(); }\n",
            "void f(void) { a(); }\n"
        ));
    }

    #[test]
    fn joined_tokens_are_found() {
        assert!(!passes(
            "c",
            &Edits::NONE,
            "#ifdef X\n#endif\n",
            "#ifdefX\n#endif\n"
        ));
        assert!(!passes(
            "c",
            &Edits::NONE,
            "int a = b - > c;\n",
            "int a = b -> c;\n"
        ));
    }

    #[test]
    fn a_line_break_in_a_directive_is_found() {
        let source = "#define A 1\nint x;\n";
        assert!(!passes("c", &Edits::NONE, source, "#define A\n1\nint x;\n"));
        assert!(!passes("c", &Edits::NONE, source, "#define A 1 int x;\n"));
        assert!(passes(
            "c",
            &Edits::NONE,
            "#define A \\\n    1\n",
            "#define A \\\n 1\n"
        ));
    }

    #[test]
    fn braces_may_be_added_only_when_allowed() {
        let source = "void f(int a) { if (a) g(); }\n";
        let output = "void f(int a)\n{\n    if (a) {\n        g();\n    }\n}\n";
        assert!(passes("c", &BRACES, source, output));
        assert!(!passes("c", &Edits::NONE, source, output));
        assert!(!passes(
            "c",
            &BRACES,
            "void f(void) { g(); }\n",
            "void f(void) g();\n"
        ));
    }

    #[test]
    fn comments_may_move_but_not_change() {
        let source = "void f(int a)\n{\n    if (a) // why\n    {\n        g();\n    }\n}\n";
        let moved = "void f(int a)\n{\n    if (a) {\n        // why\n        g();\n    }\n}\n";
        assert!(passes("c", &Edits::NONE, source, moved));
        assert!(!passes(
            "c",
            &Edits::NONE,
            source,
            &moved.replace("why", "who")
        ));
        assert!(!passes(
            "c",
            &Edits::NONE,
            source,
            &moved.replace("// why\n", "")
        ));
    }

    #[test]
    fn string_contents_are_compared_exactly() {
        assert!(!passes(
            "c",
            &Edits::NONE,
            "char *s = \"true\";\n",
            "char *s = \" true \";\n"
        ));
        assert!(!passes("python", &PYTHON, "s = 'a  b'\n", "s = 'a b'\n"));
    }

    #[test]
    fn trailing_commas_may_come_and_go_unless_they_make_a_tuple() {
        assert!(passes(
            "rust",
            &COMMAS,
            "fn f() { g(a, b,); }\n",
            "fn f() { g(a, b); }\n"
        ));
        assert!(passes(
            "rust",
            &COMMAS,
            "fn f() { g(a, b); }\n",
            "fn f() { g(a, b,); }\n"
        ));
        assert!(passes(
            "rust",
            &COMMAS,
            "fn f<T>() where T: X {}\n",
            "fn f<T>() where T: X, {}\n"
        ));
        assert!(!passes(
            "rust",
            &COMMAS,
            "fn f() { let t = (5,); }\n",
            "fn f() { let t = (5); }\n"
        ));
        assert!(!passes(
            "rust",
            &COMMAS,
            "fn f() { let t = (5); }\n",
            "fn f() { let t = (5,); }\n"
        ));
        assert!(!passes(
            "rust",
            &COMMAS,
            "fn f() { g(a, b); }\n",
            "fn f() { g(a b); }\n"
        ));
        assert!(!passes("python", &PYTHON, "x = d[a,]\n", "x = d[a]\n"));
    }

    #[test]
    fn python_indentation_is_compared() {
        let source = "if a:\n  b = 1\n  c = 2\n";
        assert!(passes(
            "python",
            &PYTHON,
            source,
            "if a:\n    b = 1\n    c = 2\n"
        ));
        assert!(!passes(
            "python",
            &PYTHON,
            source,
            "if a:\n    b = 1\nc = 2\n"
        ));
        assert!(!passes(
            "python",
            &PYTHON,
            "def g(a):\n    if a\n        return a\n",
            "def g(a): if a return a\n"
        ));
    }

    #[test]
    fn python_forms_that_read_the_same_pass() {
        assert!(passes(
            "python",
            &PYTHON,
            "a = 1; b = 2\n",
            "a = 1\nb = 2\n"
        ));
        assert!(passes(
            "python",
            &PYTHON,
            "if a: b = 1\nc = 2\n",
            "if a:\n    b = 1\nc = 2\n"
        ));
        assert!(passes(
            "python",
            &PYTHON,
            "x = 1 + \\\n    2\n",
            "x = 1 + 2\n"
        ));
        assert!(!passes(
            "python",
            &PYTHON,
            "if a: b = 1; c = 2\n",
            "if a:\n    b = 1\nc = 2\n"
        ));
    }
}
