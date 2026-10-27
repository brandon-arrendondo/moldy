// The token check for the C, C++, Rust and Python formatters. These
// formatters continue over syntax errors, so a misread can lead them to
// invent, drop or join tokens. Formatting may change layout only: the
// output is parsed again and its tokens compared with the input's, and a
// file whose tokens differ is declined. The only token edits allowed are
// the ones a formatting option makes, listed in `Edits`.

use std::borrow::Cow;
use std::collections::HashMap;
use tree_sitter::Node;

use crate::error::MoldyError;

/// The token edits a formatter may make.
pub(super) struct Edits {
    /// The control statements, by the grammar's kind, whose body may be
    /// given braces: `{` and `}` around exactly the one statement that
    /// was the body.
    pub braced_bodies: Vec<&'static str>,
    /// The tokens that may be deleted, by kind.
    pub delete: &'static [&'static str],
    /// Whether a trailing comma that changes nothing may be added or
    /// removed; see `is_optional_comma`.
    pub trailing_commas: bool,
}

impl Edits {
    pub const NONE: Edits = Edits {
        braced_bodies: Vec::new(),
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

/// `MoldyError::Unverified` unless `output` has the tokens of `source`,
/// apart from the `edits` allowed, and has no syntax error unless `source`
/// has one. Both are parsed with `language_key`'s grammar; tokens the
/// parser invented to recover from an error (zero width) are not part of
/// either text and are ignored. Comments may move past code but must stay
/// in order, except Rust doc comments, which are compared in place, and a
/// Python file's shebang or encoding line, which stays on its line.
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
    let braces = !edits.braced_bodies.is_empty();
    let source_tree = parse(source)?;
    let output_tree = parse(output)?;
    let mut before = tokens(source_tree.root_node(), source, layout, braces);
    let mut after = tokens(output_tree.root_node(), output, layout, braces);
    // A comma that changes nothing may come or go, so it is not compared.
    // Each side's own tree says which commas those are.
    if edits.trailing_commas {
        before.code.retain(|t| !t.optional_comma);
        after.code.retain(|t| !t.optional_comma);
    }
    let unverified = |row: usize| MoldyError::Unverified {
        language: language.to_string(),
        line: row + 1,
    };

    let moved = (0..before.comments.len().max(after.comments.len())).find(|&i| {
        before.comments.get(i).map(|t| &t.text) != after.comments.get(i).map(|t| &t.text)
    });
    if let Some(i) = moved {
        return Err(unverified(row_at(&before.comments, i)));
    }
    if layout == Layout::Indented {
        if let Some(row) = moved_header_line(&before, &after) {
            return Err(unverified(row));
        }
    }
    let (mut pairs, mut braces_added) = (Vec::new(), Vec::new());
    if let Some(i) = align(&before, &after, edits, &mut pairs, &mut braces_added) {
        return Err(unverified(row_at(&before.code, i)));
    }
    if let Some(row) = moved_comment(&before, &after, &pairs, &braces_added) {
        return Err(unverified(row));
    }
    // The same tokens can still read differently, if the output breaks
    // them into lines the grammar cannot read. The input token at the
    // index of the output's first token past the error is near it.
    if !source_tree.root_node().has_error() && output_tree.root_node().has_error() {
        let error_row = first_error_row(output_tree.root_node());
        let i = after.code.iter().position(|t| t.row >= error_row);
        return Err(unverified(row_at(
            &before.code,
            i.unwrap_or(before.code.len()),
        )));
    }
    Ok(())
}

/// Matches `after`'s code tokens to `before`'s, in one pass: each output
/// token must be the next input token, a brace `edits` allows, or follow
/// input tokens `edits` allows to be deleted. Returns the index of the
/// input token where they part, or `None` if they match. Adds the `seq`s
/// of each input token and the output token it matches to `pairs`, and
/// the `seq` of each brace added to `added`, with whether it opens.
///
/// One pass decides it exactly. A brace is added only where an input
/// body starts, which no input brace can, and closed by the brace of the
/// same block, which must come right after the body's last token; a
/// deleted token is skipped only where the texts differ, and output
/// tokens are never deleted nor input tokens inserted.
fn align(
    before: &Tokens,
    after: &Tokens,
    edits: &Edits,
    pairs: &mut Vec<(usize, usize)>,
    braces_added: &mut Vec<(usize, bool)>,
) -> Option<usize> {
    let (old, new) = (&before.code, &after.code);
    // The input tokens since the last match: the end of the body that
    // each open added brace holds, and the block that will close it.
    let mut added: Vec<(usize, usize)> = Vec::new();
    let mut last_seq = usize::MAX;
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        let (b, a) = (old.get(i), new.get(j));
        if let Some(a) = a {
            let block = a
                .block
                .filter(|(_, control)| edits.braced_bodies.contains(control));
            if let Some((id, control)) = block {
                if let Some(&(end, open)) = added.last().filter(|_| a.text == "}") {
                    if open == id {
                        if last_seq != end {
                            return Some(i);
                        }
                        added.pop();
                        braces_added.push((a.seq, false));
                        j += 1;
                        continue;
                    }
                }
                let body = b.and_then(|b| before.bodies.get(&b.seq));
                if a.text == "{" && b.is_some_and(|b| b.text != "{") {
                    match body.and_then(|bodies| bodies.iter().find(|(_, c)| *c == control)) {
                        Some(&(end, _)) => {
                            added.push((end, id));
                            braces_added.push((a.seq, true));
                            j += 1;
                            continue;
                        }
                        None => return Some(i),
                    }
                }
            }
        }
        match (b, a) {
            (Some(b), Some(a)) if b.text == a.text => {
                if b.seq != usize::MAX && a.seq != usize::MAX {
                    pairs.push((b.seq, a.seq));
                }
                last_seq = b.seq;
                i += 1;
                j += 1;
            }
            (Some(b), _) if edits.delete.contains(&b.kind) => i += 1,
            _ => return Some(i),
        }
    }
    if added.is_empty() {
        None
    } else {
        Some(i)
    }
}

/// The row of the first comment the output moves off the code it
/// describes. Each comment must stay between the same two code tokens,
/// on the line of the one before it only if it was, and on the line of
/// the one after it only if it was. A comment on a line of its own keeps
/// a blank line before it, unless the block opens there, and gains none
/// after it. Tokens an edit deletes are passed over; `pairs` holds the
/// `seq`s of the input and output tokens that match. A brace an edit
/// adds, in `braces_added`, bounds the body it is added to: a comment
/// before the body's first token comes after the added `{`, one after
/// the body's last token on its line before the added `}`, and any
/// other after it.
fn moved_comment(
    before: &Tokens,
    after: &Tokens,
    pairs: &[(usize, usize)],
    braces_added: &[(usize, bool)],
) -> Option<usize> {
    // Where the comment is among the matched tokens, and whether it is
    // on the line of the one before it and of the one after it.
    let place = |tokens: &Tokens, comment: &Token, side: fn(&(usize, usize)) -> usize| {
        let at = pairs.partition_point(|p| side(p) < comment.seq);
        let row_of = |i: Option<usize>| i.and_then(|i| pairs.get(i)).map(|p| tokens.rows[side(p)]);
        let joins_before = comment.trailing
            && row_of(at.checked_sub(1)).is_some_and(|(_, end)| end == comment.row);
        let joins_after =
            comment.leading && row_of(Some(at)).is_some_and(|(start, _)| start == comment.end_row);
        (at, joins_before, comment.trailing, joins_after)
    };
    // Code after a comment may come onto its line only if it was there,
    // and leave it only if the comment follows code too, which it then
    // describes.
    let leads_wrongly = |b: &Token, a: &Token| {
        (a.leading && !b.leading) || (b.leading && !a.leading && !b.trailing)
    };
    // Whether the output puts `comment` on the wrong side of a brace
    // added between the matched tokens around it.
    let crosses_brace = |b: &Token, a: &Token| {
        let at = pairs.partition_point(|p| p.1 < a.seq);
        let from = at.checked_sub(1).map_or(0, |i| pairs[i].1);
        let to = pairs.get(at).map_or(usize::MAX, |p| p.1);
        let first = braces_added.partition_point(|&(seq, _)| seq < from);
        braces_added[first..]
            .iter()
            .take_while(|&&(seq, _)| seq < to)
            .any(|&(seq, opens)| (seq < a.seq) != (opens || !b.trailing))
    };
    before
        .comments
        .iter()
        .zip(&after.comments)
        .find(|(b, a)| {
            let (b_place, a_place) = (place(before, b, |p| p.0), place(after, a, |p| p.1));
            b_place.0 != a_place.0
                || b_place.1 != a_place.1
                || b_place.2 != a_place.2
                || (b.leading && a.leading && b_place.3 != a_place.3)
                || leads_wrongly(b, a)
                || (b.blank_before && !a.blank_before)
                || (a.blank_after && !b.blank_after)
                || crosses_brace(b, a)
        })
        .map(|(b, _)| b.row)
}

/// The row of a Python shebang or encoding line that the output moves,
/// or puts code before, or a comment the output moves into the place of
/// one. Python reads them only on the first two lines, with no code
/// before them, so a comment that moves into that place takes effect.
fn moved_header_line(before: &Tokens, after: &Tokens) -> Option<usize> {
    let header = |tokens: &Tokens, comment: &Token| {
        let row = comment.row;
        row <= 1
            && tokens.code.first().is_none_or(|t| t.row > row)
            && ((row == 0 && comment.text.starts_with("#!"))
                || comment.text.contains("coding:")
                || comment.text.contains("coding="))
    };
    before
        .comments
        .iter()
        .zip(&after.comments)
        .take_while(|(b, a)| b.row <= 1 || a.row <= 1)
        .find(|(b, a)| {
            let (was, is) = (header(before, b), header(after, a));
            (was || is) && (was != is || a.row != b.row)
        })
        .map(|(b, _)| b.row)
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
#[derive(Clone, Copy, PartialEq)]
enum Layout {
    /// Nowhere (Rust).
    Free,
    /// At the end of a C preprocessor line.
    Directives,
    /// At the end of every logical line, and indentation too (Python).
    Indented,
}

struct Token<'a> {
    text: Cow<'a, str>,
    /// The grammar's kind for a leaf, which tells a bracket from the same
    /// text in a string; empty for other tokens.
    kind: &'static str,
    row: usize,
    col: usize,
    end_row: usize,
    /// The token's place among the text's tokens, before layout tokens
    /// are added (`usize::MAX` for those). For a comment, the place of
    /// the code token after it.
    seq: usize,
    /// A trailing comma that changes nothing; see `Edits::trailing_commas`.
    optional_comma: bool,
    /// A Python `:` that a block follows.
    opens_block: bool,
    /// For a brace of a block that is a control statement's body, the
    /// block's id and the control statement's kind.
    block: Option<(usize, &'static str)>,
    /// A comment that follows code on its line.
    trailing: bool,
    /// A comment that code other than a closing bracket follows on its
    /// last line.
    leading: bool,
    /// A comment on a line of its own with a blank line between it and
    /// the code or comment before it.
    blank_before: bool,
    /// A comment on a line of its own with a blank line between it and
    /// the code or comment after it.
    blank_after: bool,
}

struct Tokens<'a> {
    code: Vec<Token<'a>>,
    comments: Vec<Token<'a>>,
    /// The first and last row of each code token, by `seq`.
    rows: Vec<(usize, usize)>,
    /// The control statement bodies not in braces, by the `seq` of their
    /// first token: the `seq` of their last, and the control's kind.
    bodies: HashMap<usize, Vec<(usize, &'static str)>>,
}

/// The tokens of the tree at `root`. Code tokens are its leaves, a
/// preprocessor directive's text split into words, any text no leaf
/// covers, split at whitespace, Rust doc comments, and the line
/// structure `layout` gives. Comments are compared with their runs of
/// whitespace made one space, and carriage returns are not compared.
/// With `braces`, the bodies that could be given braces are found.
fn tokens<'a>(root: Node, source: &'a str, layout: Layout, braces: bool) -> Tokens<'a> {
    let mut walk = Walk {
        source,
        braces,
        out: Tokens {
            code: Vec::new(),
            comments: Vec::new(),
            rows: Vec::new(),
            bodies: HashMap::new(),
        },
    };
    walk.push_tokens(root);
    let mut out = walk.out;
    out.rows = out.code.iter().map(|t| (t.row, t.end_row)).collect();
    let blank: Vec<bool> = source.split('\n').map(|l| l.trim().is_empty()).collect();
    let any_blank = |rows: std::ops::Range<usize>| rows.into_iter().any(|r| blank[r]);
    for i in 0..out.comments.len() {
        let (before, rest) = out.comments.split_at_mut(i);
        let (comment, after) = rest.split_first_mut().expect("i is in range");
        let seq = comment.seq;
        // A closing bracket after a comment on its line may move to the
        // next line, or onto the comment's, as it describes nothing.
        comment.leading = out.code.get(seq).is_some_and(|t| {
            t.row == comment.end_row && !matches!(t.text.as_ref(), "}" | ")" | "]")
        });
        if comment.trailing {
            continue;
        }
        // The code or comment on either side.
        // A blank line after the `{` or `:` that opens a block, or the
        // bracket that opens a list, places nothing, so one that ends its
        // line is passed over.
        let opens = |p: usize| {
            let t = &out.code[p];
            matches!(t.text.as_ref(), "{" | ":" | "(" | "[")
                && out.code.get(p + 1).is_none_or(|n| n.row > t.end_row)
        };
        let previous = match before.last() {
            Some(c) if c.seq == seq => Some(c.end_row),
            _ => seq
                .checked_sub(1)
                .filter(|&p| !opens(p))
                .map(|p| out.code[p].end_row),
        };
        let next = match after.first() {
            Some(c) if c.seq == seq => Some(c.row),
            _ => out.code.get(seq).map(|t| t.row),
        };
        comment.blank_before = previous.is_some_and(|p| any_blank(p + 1..comment.row));
        comment.blank_after = next.is_some_and(|n| any_blank(comment.end_row + 1..n));
    }
    out.code = match layout {
        Layout::Free => out.code,
        Layout::Directives => mark_directive_ends(out.code, source),
        Layout::Indented => mark_logical_lines(out.code),
    };
    out
}

/// Where a node is in its tree. Asking tree-sitter for a node's parent or
/// siblings walks down from the root, so the walk carries them.
#[derive(Default)]
struct Place<'t, 'n> {
    parent: Option<Node<'t>>,
    grandparent: Option<Node<'t>>,
    /// The siblings before the node.
    previous: &'n [Node<'t>],
    /// The siblings after the node.
    next: &'n [Node<'t>],
    /// Whether the node is in a macro definition or a macro's tokens.
    in_macro: bool,
}

/// A node whose children the walk is in.
struct Open<'t> {
    node: Node<'t>,
    parent: Option<Node<'t>>,
    children: Vec<Node<'t>>,
    /// The next child.
    i: usize,
    /// The end of the text the walk has passed.
    at: usize,
    at_point: tree_sitter::Point,
    /// The index of the node's first code token.
    first: usize,
    in_macro: bool,
}

struct Walk<'a> {
    source: &'a str,
    braces: bool,
    out: Tokens<'a>,
}

impl<'a> Walk<'a> {
    fn push(&mut self, mut token: Token<'a>) {
        token.seq = self.out.code.len();
        self.out.code.push(token);
    }

    /// Pushes the tokens of the tree at `root`. The walk keeps its own
    /// stack, so no nesting exhausts the thread's.
    fn push_tokens(&mut self, root: Node) {
        let mut stack: Vec<Open> = self.enter(root, &Place::default()).into_iter().collect();
        while let Some(open) = stack.last_mut() {
            let Some(&child) = open.children.get(open.i) else {
                let open = stack.pop().expect("the loop holds an open node");
                self.leave(open);
                continue;
            };
            push_gap(
                self.source,
                open.at,
                open.at_point,
                child.start_byte(),
                &mut self.out.code,
            );
            if child.end_byte() > open.at {
                (open.at, open.at_point) = (child.end_byte(), child.end_position());
            }
            let place = Place {
                parent: Some(open.node),
                grandparent: open.parent,
                previous: &open.children[..open.i],
                next: &open.children[open.i + 1..],
                in_macro: open.in_macro,
            };
            let inner = self.enter(child, &place);
            open.i += 1;
            stack.extend(inner);
        }
    }

    /// Pushes the tokens of `node` if it has no children to walk, or
    /// returns it opened.
    fn enter<'t>(&mut self, node: Node<'t>, place: &Place<'t, '_>) -> Option<Open<'t>> {
        if node.start_byte() == node.end_byte() {
            return None;
        }
        let source = self.source;
        let kind = node.kind();
        let start = node.start_position();
        let end_row = node.end_position().row;
        let text = &source[node.byte_range()];
        if kind.ends_with("comment") {
            if is_doc_comment(node) {
                let mut doc = token(
                    without_cr(text.trim_end_matches(['\r', '\n'])),
                    start,
                    end_row,
                );
                doc.kind = "doc_comment";
                self.push(doc);
            } else {
                let words: Vec<&str> = text.split_whitespace().collect();
                let mut comment = token(Cow::Owned(words.join(" ")), start, end_row);
                comment.seq = self.out.code.len();
                comment.trailing = self.out.code.last().is_some_and(|t| t.end_row == start.row);
                self.out.comments.push(comment);
            }
            return None;
        }
        if is_literal(kind) {
            self.push(token(without_cr(text), start, end_row));
            return None;
        }
        if kind == "preproc_arg" {
            push_words(text, start.row, &mut self.out.code);
            return None;
        }
        if node.child_count() == 0 {
            if text.trim().is_empty() {
                return None;
            }
            // The C grammar reads `-1` as one number, which C reads as
            // `-` and `1`, the same as `- 1`.
            let (sign, number) = match text.as_bytes()[0] {
                b'-' | b'+' if kind == "number_literal" => text.split_at(1),
                _ => ("", text),
            };
            if !sign.is_empty() {
                self.push(token(Cow::Borrowed(sign), start, end_row));
            }
            let mut leaf = token(without_cr(number), start, end_row);
            leaf.kind = kind;
            leaf.optional_comma = is_optional_comma(node, place);
            leaf.opens_block =
                kind == ":" && place.next.first().is_some_and(|n| n.kind() == "block");
            if self.braces && matches!(kind, "{" | "}") {
                leaf.block = place.parent.and_then(|block| {
                    let control = body_of(block, place.grandparent)
                        .filter(|_| block.kind() == "compound_statement");
                    control.map(|control| (block.id(), control))
                });
            }
            self.push(leaf);
            return None;
        }
        let mut cursor = node.walk();
        Some(Open {
            node,
            parent: place.parent,
            children: node.children(&mut cursor).collect(),
            i: 0,
            at: node.start_byte(),
            at_point: start,
            first: self.out.code.len(),
            in_macro: place.in_macro || kind.starts_with("token_") || kind == "macro_definition",
        })
    }

    /// Pushes the text after `open`'s last child, and notes `open` if it
    /// is a body that could be given braces.
    fn leave(&mut self, open: Open) {
        let node = open.node;
        push_gap(
            self.source,
            open.at,
            open.at_point,
            node.end_byte(),
            &mut self.out.code,
        );
        let last = self.out.code.len();
        if self.braces && last > open.first && node.kind() != "compound_statement" {
            if let Some(control) = body_of(node, open.parent) {
                let bodies = self.out.bodies.entry(open.first).or_default();
                bodies.push((last - 1, control));
            }
        }
    }
}

/// The kind of the control statement whose body `node`, a child of
/// `parent`, is: the statement after an `if`, `else`, `while` or `for`
/// header.
fn body_of(node: Node, parent: Option<Node>) -> Option<&'static str> {
    let parent = parent?;
    let body = match parent.kind() {
        "if_statement" => parent.child_by_field_name("consequence"),
        "while_statement" | "for_statement" | "for_range_loop" => {
            parent.child_by_field_name("body")
        }
        "else_clause" => {
            let mut cursor = parent.walk();
            let body = parent
                .named_children(&mut cursor)
                .find(|n| !n.kind().ends_with("comment"));
            body
        }
        _ => None,
    };
    (body == Some(node)).then(|| parent.kind())
}

/// Whether `node`, a comment, is a Rust doc comment (`///`, `//!`, `/**`
/// or `/*!`), which is an attribute of the item it documents.
fn is_doc_comment(node: Node) -> bool {
    let mut cursor = node.walk();
    let doc = node
        .children(&mut cursor)
        .any(|c| c.kind().ends_with("doc_comment_marker"));
    doc
}

fn without_cr(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace('\r', ""))
    } else {
        Cow::Borrowed(text)
    }
}

fn token(text: Cow<'_, str>, start: tree_sitter::Point, end_row: usize) -> Token<'_> {
    Token {
        text,
        row: start.row,
        col: start.column,
        end_row,
        seq: usize::MAX,
        optional_comma: false,
        opens_block: false,
        block: None,
        trailing: false,
        leading: false,
        blank_before: false,
        blank_after: false,
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
/// closing bracket or at the end of a Rust `where` clause. Not one in a
/// macro definition or a macro's token tree, where it is matched or
/// expanded as written, the comma that makes a one-element tuple (`(a,)`),
/// one in a Python subscript or type subscript (`x[a,]`, `tuple[a,]`),
/// whose index it makes a tuple, or one after a Rust struct base or rest
/// pattern (`..base`, `..`), which may not have one.
fn is_optional_comma(node: Node, place: &Place) -> bool {
    if node.kind() != "," || node.is_missing() || place.in_macro {
        return false;
    }
    let Some(parent) = place.parent else {
        return false;
    };
    let kind = parent.kind();
    let next = place.next.iter().find(|n| !n.kind().ends_with("comment"));
    let previous = place
        .previous
        .iter()
        .rfind(|n| !n.kind().ends_with("comment"));
    if previous.is_some_and(|p| {
        matches!(
            p.kind(),
            "base_field_initializer" | "remaining_field_pattern"
        )
    }) {
        return false;
    }
    // A Rust `where` clause ends at the `{` or `;` after it.
    let closes = match next {
        Some(next) => matches!(next.kind(), ")" | "]" | "}" | ">"),
        None => kind == "where_clause",
    };
    if !closes || matches!(kind, "subscript" | "type_parameter") || parent.is_error() {
        return false;
    }
    let one_tuple = kind.starts_with("tuple") && {
        let mut cursor = parent.walk();
        let commas = parent
            .children(&mut cursor)
            .filter(|c| c.kind() == ",")
            .count();
        commas == 1
    };
    !one_tuple
}

/// Text from `start`, at `point`, to `end` that no leaf covers, split at
/// whitespace.
fn push_gap<'a>(
    source: &'a str,
    start: usize,
    point: tree_sitter::Point,
    end: usize,
    out: &mut Vec<Token<'a>>,
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
            let mut gap = token(
                Cow::Borrowed(word),
                tree_sitter::Point::new(row, line_col + offset),
                row,
            );
            gap.seq = out.len();
            if word == "\\" && ends_line {
                gap.kind = "line_continuation";
            }
            out.push(gap);
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
        let mut flushed = layout_token(std::mem::take(word), row);
        flushed.seq = out.len();
        out.push(flushed);
    }
}

fn layout_token<'a>(text: impl Into<Cow<'a, str>>, row: usize) -> Token<'a> {
    token(text.into(), tree_sitter::Point::new(row, 0), row)
}

/// `tokens` with a `NEWLINE` after each preprocessor line: one whose
/// first token starts with `#`, through its line continuations. A line
/// break there ends the directive, so one moved there changes the code.
fn mark_directive_ends<'a>(tokens: Vec<Token<'a>>, source: &str) -> Vec<Token<'a>> {
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

    fn braces() -> Edits {
        Edits {
            braced_bodies: vec![
                "if_statement",
                "else_clause",
                "while_statement",
                "for_statement",
            ],
            ..Edits::NONE
        }
    }

    fn commas() -> Edits {
        Edits {
            trailing_commas: true,
            ..Edits::NONE
        }
    }

    fn python() -> Edits {
        Edits {
            delete: &["line_continuation"],
            trailing_commas: true,
            ..Edits::NONE
        }
    }

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
            &python(),
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
        assert!(passes("c", &braces(), source, output));
        assert!(!passes("c", &Edits::NONE, source, output));
        assert!(!passes(
            "c",
            &braces(),
            "void f(void) { g(); }\n",
            "void f(void) g();\n"
        ));
    }

    #[test]
    fn comments_may_not_change() {
        let source = "void f(int a)\n{\n    if (a) // why\n    {\n        g();\n    }\n}\n";
        let kept = "void f(int a)\n{\n    if (a) // why\n    {\n        g();\n    }\n}\n";
        assert!(passes("c", &Edits::NONE, source, kept));
        assert!(!passes(
            "c",
            &Edits::NONE,
            source,
            &kept.replace("why", "who")
        ));
        assert!(!passes(
            "c",
            &Edits::NONE,
            source,
            &kept.replace("// why\n", "\n")
        ));
    }

    #[test]
    fn comments_stay_by_the_code_they_describe() {
        let source = "void f(int a)\n{\n    if (a) // why\n    {\n        g();\n    }\n}\n";
        // Into the braces, past the `{`.
        let moved = "void f(int a)\n{\n    if (a) {\n        // why\n        g();\n    }\n}\n";
        assert!(!passes("c", &Edits::NONE, source, moved));
        // A trailing comment moved onto the next line, before the next
        // enumerator, and back.
        let enumerators = "enum e {\n    A, /* first */\n    B\n};\n";
        let split = "enum e {\n    A,\n    /* first */\n    B\n};\n";
        assert!(!passes("c", &Edits::NONE, enumerators, split));
        assert!(!passes("c", &Edits::NONE, split, enumerators));
        // Off a directive's line.
        assert!(!passes(
            "c",
            &Edits::NONE,
            "#ifdef X\nint a;\n#endif /* X */\nint b;\n",
            "#ifdef X\nint a;\n#endif\n/* X */\nint b;\n"
        ));
        // A line joined across a comment, which then leads code.
        assert!(!passes(
            "c",
            &Edits::NONE,
            "int x = a || /* why a */\n    b;\n",
            "int x = a || /* why a */b;\n"
        ));
        // Code joined after a comment that leads it, or split from it.
        assert!(passes(
            "c",
            &Edits::NONE,
            "int x =   /* one */ 1 ;\n",
            "int x = /* one */ 1;\n"
        ));
        assert!(!passes(
            "c",
            &Edits::NONE,
            "int x;\n/* one */ int y;\n",
            "int x;\n/* one */\nint y;\n"
        ));
        // After code too, the comment describes that code, so what
        // follows it may take a line of its own; a closing bracket may
        // leave or join its line.
        assert!(passes(
            "c",
            &Edits::NONE,
            "struct p { int x; /* x coord */ int y; };\n",
            "struct p {\n    int x; /* x coord */\n    int y;\n};\n"
        ));
        assert!(passes(
            "c",
            &Edits::NONE,
            "struct p q = { .x = 1,\n    .y = 2 /* dy */ };\n",
            "struct p q = { .x = 1,\n    .y = 2 /* dy */\n};\n"
        ));
        // Comment columns may change.
        assert!(passes(
            "c",
            &Edits::NONE,
            "int a; /* a */\nint bb;   /* b */\n",
            "int a;  /* a */\nint bb; /* b */\n"
        ));
    }

    #[test]
    fn a_comment_keeps_its_blank_lines() {
        let source = "void f(int a)\n{\n    int b;\n\n    /* why */\n    g(a);\n}\n";
        assert!(passes("c", &Edits::NONE, source, source));
        // A blank line put between a comment and the code it leads.
        let split = "void f(int a)\n{\n    int b;\n\n    /* why */\n\n    g(a);\n}\n";
        assert!(!passes("c", &Edits::NONE, source, split));
        // Removed there, it still describes the code below it.
        assert!(passes("c", &Edits::NONE, split, source));
        // The blank line above it removed.
        let joined = "void f(int a)\n{\n    int b;\n    /* why */\n    g(a);\n}\n";
        assert!(!passes("c", &Edits::NONE, source, joined));
        // One added above it.
        assert!(passes("c", &Edits::NONE, joined, source));
    }

    #[test]
    fn a_comment_may_not_cross_an_added_brace() {
        // Into the body after its last statement.
        assert!(!passes(
            "c",
            &braces(),
            "void f(int a)\n{\n    if (a)\n        g();\n    /* about h */\n    h();\n}\n",
            "void f(int a)\n{\n    if (a) {\n        g();\n        /* about h */\n    }\n    h();\n}\n"
        ));
        // Out of the body, before its `{`.
        assert!(!passes(
            "c",
            &braces(),
            "void f(int a)\n{\n    if (a)\n        /* about g */\n        g();\n}\n",
            "void f(int a)\n{\n    if (a)\n    /* about g */\n    {\n        g();\n    }\n}\n"
        ));
        // Before the `else`, into the if's body.
        assert!(!passes(
            "c",
            &braces(),
            "void f(int a)\n{\n    if (a)\n        g();\n    // about else\n    else\n        h();\n}\n",
            "void f(int a)\n{\n    if (a) {\n        g();\n        // about else\n    }\n    else {\n        h();\n    }\n}\n"
        ));
        // After the `}` on the statement's line.
        assert!(!passes(
            "c",
            &braces(),
            "void f(int a)\n{\n    if (a) g(); /* g */\n}\n",
            "void f(int a)\n{\n    if (a) {\n        g(); } /* g */\n}\n"
        ));
        // Where they belong.
        assert!(passes(
            "c",
            &braces(),
            "void f(int a)\n{\n    if (a)\n        g();\n    // about else\n    else\n        h();\n}\n",
            "void f(int a)\n{\n    if (a) {\n        g();\n    }\n    // about else\n    else {\n        h();\n    }\n}\n"
        ));
    }

    #[test]
    fn added_braces_do_not_move_comments() {
        let source = "void f(int a)\n{\n    if (a) /* why */\n        g(); /* g */\n    h();\n}\n";
        let output =
            "void f(int a)\n{\n    if (a) { /* why */\n        g(); /* g */\n    }\n    h();\n}\n";
        assert!(passes("c", &braces(), source, output));
        // After the `}`, the comment is still after `g();` on its line.
        let after =
            "void f(int a)\n{\n    if (a) { /* why */\n        g();\n    } /* g */\n    h();\n}\n";
        assert!(!passes("c", &braces(), source, after));
    }

    #[test]
    fn string_contents_are_compared_exactly() {
        assert!(!passes(
            "c",
            &Edits::NONE,
            "char *s = \"true\";\n",
            "char *s = \" true \";\n"
        ));
        assert!(!passes("python", &python(), "s = 'a  b'\n", "s = 'a b'\n"));
    }

    #[test]
    fn trailing_commas_may_come_and_go_unless_they_make_a_tuple() {
        assert!(passes(
            "rust",
            &commas(),
            "fn f() { g(a, b,); }\n",
            "fn f() { g(a, b); }\n"
        ));
        assert!(passes(
            "rust",
            &commas(),
            "fn f() { g(a, b); }\n",
            "fn f() { g(a, b,); }\n"
        ));
        assert!(passes(
            "rust",
            &commas(),
            "fn f<T>() where T: X {}\n",
            "fn f<T>() where T: X, {}\n"
        ));
        assert!(!passes(
            "rust",
            &commas(),
            "fn f() { let t = (5,); }\n",
            "fn f() { let t = (5); }\n"
        ));
        assert!(!passes(
            "rust",
            &commas(),
            "fn f() { let t = (5); }\n",
            "fn f() { let t = (5,); }\n"
        ));
        assert!(!passes(
            "rust",
            &commas(),
            "fn f() { g(a, b); }\n",
            "fn f() { g(a b); }\n"
        ));
        assert!(!passes("python", &python(), "x = d[a,]\n", "x = d[a]\n"));
    }

    #[test]
    fn python_indentation_is_compared() {
        let source = "if a:\n  b = 1\n  c = 2\n";
        assert!(passes(
            "python",
            &python(),
            source,
            "if a:\n    b = 1\n    c = 2\n"
        ));
        assert!(!passes(
            "python",
            &python(),
            source,
            "if a:\n    b = 1\nc = 2\n"
        ));
        assert!(!passes(
            "python",
            &python(),
            "def g(a):\n    if a\n        return a\n",
            "def g(a): if a return a\n"
        ));
    }

    #[test]
    fn python_forms_that_read_the_same_pass() {
        assert!(passes(
            "python",
            &python(),
            "a = 1; b = 2\n",
            "a = 1\nb = 2\n"
        ));
        assert!(passes(
            "python",
            &python(),
            "if a: b = 1\nc = 2\n",
            "if a:\n    b = 1\nc = 2\n"
        ));
        assert!(passes(
            "python",
            &python(),
            "x = 1 + \\\n    2\n",
            "x = 1 + 2\n"
        ));
        assert!(!passes(
            "python",
            &python(),
            "if a: b = 1; c = 2\n",
            "if a:\n    b = 1\nc = 2\n"
        ));
    }

    #[test]
    fn a_comment_does_not_hide_a_one_element_tuple() {
        for (source, output) in [
            (
                "fn f() { let t = (5 /* c */,); }\n",
                "fn f() { let t = (5 /* c */); }\n",
            ),
            (
                "fn f() { let t = (5, // c\n); }\n",
                "fn f() { let t = (5 // c\n); }\n",
            ),
            ("type T = (u8 /* c */,);\n", "type T = (u8 /* c */);\n"),
            (
                "fn f(t: T) { let (a /* c */,) = t; }\n",
                "fn f(t: T) { let (a /* c */) = t; }\n",
            ),
        ] {
            assert!(!passes("rust", &commas(), source, output), "{source}");
            assert!(!passes("rust", &commas(), output, source), "{output}");
        }
        let (source, output) = ("x = (1,  # c\n)\n", "x = (1  # c\n)\n");
        assert!(!passes("python", &python(), source, output));
        assert!(!passes("python", &python(), output, source));
    }

    #[test]
    fn macro_definition_commas_are_compared() {
        for (source, output) in [
            (
                "macro_rules! m { ($a:expr,) => {} }\n",
                "macro_rules! m { ($a:expr) => {} }\n",
            ),
            (
                "macro_rules! m { ($(,)?) => {} }\n",
                "macro_rules! m { ($()?) => {} }\n",
            ),
            (
                "macro_rules! m { ($($a:expr),*) => { f($($a,)*) } }\n",
                "macro_rules! m { ($($a:expr),*) => { f($($a)*) } }\n",
            ),
        ] {
            assert!(!passes("rust", &commas(), source, output), "{source}");
        }
    }

    #[test]
    fn a_python_type_subscript_keeps_its_comma() {
        assert!(!passes(
            "python",
            &python(),
            "x: tuple[int,] = 1\n",
            "x: tuple[int] = 1\n"
        ));
        assert!(passes("python", &python(), "x = [1, 2,]\n", "x = [1, 2]\n"));
    }

    #[test]
    fn many_added_braces_are_checked_in_one_pass() {
        let source: String = (0..5000)
            .map(|n| format!("void f{n}(int a) {{ if (a) g(); while (a) h(); }}\n"))
            .collect();
        let output: String = (0..5000)
            .map(|n| {
                format!(
                    "void f{n}(int a)\n{{\n    if (a) {{\n        g();\n    }}\n    \
                     while (a) {{\n        h();\n    }}\n}}\n"
                )
            })
            .collect();
        assert!(passes("c", &braces(), &source, &output));
        let dropped = output.replacen("h();", "", 1);
        assert!(!passes("c", &braces(), &source, &dropped));
    }

    #[test]
    fn rust_doc_comments_stay_with_their_item() {
        let source = "/// a\nfn x() {}\nfn y() {}\n";
        assert!(passes(
            "rust",
            &commas(),
            source,
            "/// a\nfn x() {}\n\nfn y() {}\n"
        ));
        assert!(!passes(
            "rust",
            &commas(),
            source,
            "fn x() {}\n/// a\nfn y() {}\n"
        ));
        assert!(!passes(
            "rust",
            &commas(),
            "mod m {\n    //! a\n}\n",
            "//! a\nmod m {}\n"
        ));
        assert!(!passes(
            "rust",
            &commas(),
            "/// let x = 1;\nfn x() {}\n",
            "/// letx=1;\nfn x() {}\n"
        ));
        assert!(passes(
            "rust",
            &commas(),
            "mod m {\n/// a\n  fn x() {}\n}\n",
            "mod m {\n    /// a\n    fn x() {}\n}\n"
        ));
    }

    #[test]
    fn added_braces_must_hold_exactly_the_body() {
        let wider = (
            "void f(int a) { if (a) g(); h(); }\n",
            "void f(int a) { if (a) { g(); h(); } }\n",
        );
        assert!(!passes("c", &braces(), wider.0, wider.1));
        let dangling = (
            "void f(int a, int b) { if (a) if (b) x(); else y(); }\n",
            "void f(int a, int b) { if (a) { if (b) x(); } else y(); }\n",
        );
        assert!(!passes("c", &braces(), dangling.0, dangling.1));
        let nested = (
            "void f(int a, int b) { if (a) if (b) x(); else y(); }\n",
            "void f(int a, int b) { if (a) { if (b) { x(); } else { y(); } } }\n",
        );
        assert!(passes("c", &braces(), nested.0, nested.1));
        let initializer = ("int a[] = 1, 2;\n", "int a[] = { 1, 2 };\n");
        assert!(!passes("c", &braces(), initializer.0, initializer.1));
        let only_while = Edits {
            braced_bodies: vec!["while_statement"],
            ..Edits::NONE
        };
        let source = "void f(int a) { if (a) g(); while (a) g(); }\n";
        let output = "void f(int a) { if (a) g(); while (a) { g(); } }\n";
        assert!(passes("c", &only_while, source, output));
        assert!(!passes(
            "c",
            &only_while,
            source,
            "void f(int a) { if (a) { g(); } while (a) g(); }\n"
        ));
    }

    #[test]
    fn comment_words_keep_their_spaces() {
        let source = "/* a b */\nint x;\n";
        assert!(!passes("c", &Edits::NONE, source, "/* ab */\nint x;\n"));
        assert!(passes(
            "c",
            &Edits::NONE,
            "void f(void)\n{\n  /* a\n   * b */\n}\n",
            "void f(void)\n{\n    /* a\n     * b */\n}\n"
        ));
    }

    #[test]
    fn a_python_encoding_line_stays_on_its_line() {
        let source = "#!/usr/bin/env python\n# -*- coding: latin-1 -*-\nx = 1\n";
        assert!(passes("python", &python(), source, source));
        assert!(!passes(
            "python",
            &python(),
            source,
            "#!/usr/bin/env python\n\n# -*- coding: latin-1 -*-\nx = 1\n"
        ));
        assert!(!passes(
            "python",
            &python(),
            "# -*- coding: latin-1 -*-\nx = 1\n",
            "x = 1\n# -*- coding: latin-1 -*-\n"
        ));
        // After code, it is an ordinary comment.
        let late = "\"\"\"Doc.\"\"\"\n# -*- coding: utf-8 -*-\nx = 1\n";
        assert!(passes("python", &python(), late, late));
    }

    #[test]
    fn a_python_encoding_line_moved_up_is_found() {
        // Two lines down it is a comment; on line 1 it decodes the file.
        assert!(!passes(
            "python",
            &python(),
            "\n\n# -*- coding: latin-1 -*-\nx = 1\n",
            "# -*- coding: latin-1 -*-\nx = 1\n"
        ));
        assert!(!passes(
            "python",
            &python(),
            "x = 1\n# coding: latin-1\n",
            "# coding: latin-1\nx = 1\n"
        ));
    }

    #[test]
    fn a_shebang_moved_up_is_found() {
        assert!(!passes(
            "python",
            &python(),
            "\n#!/usr/bin/env python\nx = 1\n",
            "#!/usr/bin/env python\nx = 1\n"
        ));
    }

    #[test]
    fn header_lines_that_stay_put_pass() {
        assert!(passes(
            "python",
            &python(),
            "#!/usr/bin/env python\n# -*- coding: latin-1 -*-\nx=( 1,2 )\n",
            "#!/usr/bin/env python\n# -*- coding: latin-1 -*-\nx = (1, 2)\n"
        ));
    }

    #[test]
    fn rust_struct_bases_take_no_comma() {
        assert!(!passes(
            "rust",
            &commas(),
            "fn f() { let x = S { a: 1, ..b }; }\n",
            "fn f() { let x = S { a: 1, ..b, }; }\n"
        ));
        assert!(!passes(
            "rust",
            &commas(),
            "fn f() { let S { a, .. } = s; }\n",
            "fn f() { let S { a, .., } = s; }\n"
        ));
        assert!(passes(
            "rust",
            &commas(),
            "fn f() { let x = S { a: 1 }; }\n",
            "fn f() { let x = S { a: 1, }; }\n"
        ));
    }

    #[test]
    fn a_sign_may_join_its_number() {
        assert!(passes(
            "c",
            &Edits::NONE,
            "int y = - 1;\nfloat x = + 1.0f;\n",
            "int y = -1;\nfloat x = +1.0f;\n"
        ));
        assert!(!passes(
            "c",
            &Edits::NONE,
            "int y = - -1;\n",
            "int y = --1;\n"
        ));
    }

    #[test]
    fn deep_nesting_does_not_exhaust_the_stack() {
        let deep = format!("int x = {}1{};\n", "(".repeat(100_000), ")".repeat(100_000));
        assert!(passes("c", &Edits::NONE, &deep, &deep));
    }

    #[test]
    fn a_long_tuple_is_checked_in_one_pass() {
        let big = format!("x = ({})\n", "1, ".repeat(20000));
        assert!(passes("python", &python(), &big, &big));
    }
}
