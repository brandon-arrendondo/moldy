// Bash formatter (`.sh`, `.bash`): the structural target is Prettier's
// shell plugin (prettier-plugin-sh) at its defaults, which prints with
// shfmt's printer: 2-space indentation, binary operators at the start of a
// continuation line, a space after redirection operators, and indented
// `case` items.
//
// Statements take one line each (`a; b` becomes two lines) and one blank
// line between them is kept. A compound command (`if`, `for`, `while`,
// `case`, `{ }`, `( )`, a function body) stays on one line only when it was
// written on one line and each of its bodies holds a single statement;
// otherwise its bodies are indented on their own lines. A `&&` / `||` / `|`
// chain that the source breaks over lines breaks before the operator,
// with a backslash. Heredoc bodies are emitted verbatim after the line that
// starts them.
//
// Words are verbatim except command substitutions (backticks become
// `$( )`, and the commands inside are formatted, including inside
// double-quoted strings), arithmetic (`$((1 + 2))`, `((i++))`), arrays
// (`(a b c)`, keeping their line breaks and comments when written over
// several lines), and `[ ]` / `[[ ]]` tests, whose operands are separated
// by single spaces. A condition that continues past its keyword's line
// gets its own lines, with `then` / `do` on a line of its own.
//
// Statements the grammar reads differently from bash are kept as written
// (`joins_lines`, `misparsed`): a command run on across lines or holding
// a comment, an assignment touching the next word, a `#` after a
// redirection, and a misread heredoc.
//
// Known gaps (documented rather than half-implemented):
//   - A file with any syntax error is returned unchanged.
//   - Backticks containing `\\`, `\$`, `` \` `` or `\"` are kept, since
//     those escapes mean something else inside `$( )`.
//   - A multi-line command substitution holding more than one statement, a
//     comment or a heredoc is emitted verbatim.
//   - Arguments written after a redirection stay after it; Prettier
//     sometimes moves them before.

use crate::config::Config;
use crate::error::MoldyError;
use tree_sitter::Node;

pub fn format(source: &str, config: &Config) -> Result<String, MoldyError> {
    let (out, swallowed) = format_once(source, config)?;
    // The grammar may read the output differently from the source when a
    // statement kept as written holds part of a compound, and lay it out
    // again: then the file is left unchanged.
    if let Some(row) = swallowed {
        if !format_once(&out, config).is_ok_and(|(again, _)| again == out) {
            return Err(MoldyError::Unverified {
                language: "Bash".to_string(),
                line: row + 1,
            });
        }
    }
    Ok(out)
}

/// The formatted text, and the row of the first statement kept as
/// written that holds part of a compound command.
fn format_once(source: &str, config: &Config) -> Result<(String, Option<usize>), MoldyError> {
    let tree = parse(source)?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(super::syntax_error("Bash", root));
    }
    super::check_depth("Bash", root)?;
    let (skipped, misread) = misread_text(root, source);
    if let Some(row) = misread {
        return Err(MoldyError::Unverified {
            language: "Bash".to_string(),
            line: row + 1,
        });
    }

    let mut fmt = Fmt {
        src: source,
        config,
        out: String::new(),
        heredocs: Vec::new(),
        heredoc_end: 0,
        depth: 0,
        covered: vec![false; skipped.len()],
        skipped,
        inlined: std::collections::HashMap::new(),
        swallowed: None,
    };
    fmt.block(&children(root), 0, false);
    fmt.flush_heredocs();
    // Skipped text outside any statement kept as written.
    if let Some(i) = fmt.covered.iter().position(|c| !c) {
        return Err(MoldyError::Unverified {
            language: "Bash".to_string(),
            line: source[..fmt.skipped[i]].matches('\n').count() + 1,
        });
    }
    let mut out = align_comments(&fmt.out.replace("\r\n", "\n"));
    let trimmed = super::unescaped_trim_end(&out).len();
    out.truncate(trimmed);
    if config.newlines.final_newline && !out.is_empty() {
        out.push('\n');
    }
    Ok((out, fmt.swallowed))
}

/// Marks where a trailing comment starts in the output, until
/// `align_comments` replaces it with padding.
const TRAILING: char = '\u{1}';

/// Aligns the trailing comments of consecutive lines one space past the
/// longest code among them, as shfmt does. A line without a trailing
/// comment ends the run.
fn align_comments(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].contains(TRAILING) {
            out.push(lines[i].to_string());
            i += 1;
            continue;
        }
        let run_end = (i..lines.len())
            .find(|&j| !lines[j].contains(TRAILING))
            .unwrap_or(lines.len());
        let width = lines[i..run_end]
            .iter()
            .map(|l| {
                super::unescaped_trim_end(l.split(TRAILING).next().unwrap_or(""))
                    .chars()
                    .count()
            })
            .max()
            .unwrap_or(0);
        for line in &lines[i..run_end] {
            let (code, comment) = line.split_once(TRAILING).unwrap_or((line, ""));
            let code = super::unescaped_trim_end(code);
            let pad = width - code.chars().count() + 1;
            out.push(format!("{code}{}{comment}", " ".repeat(pad)));
        }
        i = run_end;
    }
    out.join("\n")
}

fn parse(source: &str) -> Result<tree_sitter::Tree, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key("bash")
        .ok_or_else(|| MoldyError::UnsupportedLanguage("bash".to_string()))?;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;
    parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))
}

/// Text the grammar reads differently from bash. First, the offsets of
/// text between tokens that it skips (an escaped blank, `f\<TAB>` or `\ `
/// before a line break, a `-` before a redirection) or reads as a word
/// break (a line continuation inside a word). Second, the row of a
/// misreading no statement can keep: a comment right after a line
/// continuation, which bash joins to the line before (`a\` then `#b` is
/// the word `a#b`), a heredoc end marker with more text on its line
/// (`EOF&&`), which does not end the heredoc, a `` ` `` substitution
/// inside another, which bash would close at the inner `` ` ``, or a
/// command named by a reserved word that closes or continues a compound.
fn misread_text(root: Node, src: &str) -> (Vec<usize>, Option<usize>) {
    let mut leaves = Vec::new();
    let mut stack = vec![root];
    if let Some(row) = nested_backticks(root).or_else(|| reserved_command(root, src)) {
        return (Vec::new(), Some(row));
    }
    while let Some(node) = stack.pop() {
        // Text inside these is the node's own, between its children.
        let whole = matches!(
            node.kind(),
            "heredoc_body" | "string" | "translated_string" | "word" | "regex"
        );
        if node.child_count() == 0 || whole {
            leaves.push(node);
        } else {
            let mut kids = children(node);
            kids.reverse();
            stack.extend(kids);
        }
    }
    let mut prev_end = 0;
    let mut skipped_at = Vec::new();
    for leaf in leaves {
        let start = leaf.start_byte();
        if start < prev_end {
            continue;
        }
        let gap = &src[prev_end..start];
        let mut bytes = gap.bytes().peekable();
        let mut skipped = false;
        while let Some(b) = bytes.next() {
            match b {
                b' ' | b'\t' | b'\r' | b'\n' => {}
                b'\\' => {
                    bytes.next_if_eq(&b'\r');
                    if bytes.next_if_eq(&b'\n').is_none() {
                        skipped = true;
                    }
                }
                _ => skipped = true,
            }
        }
        // After each `\` and line break is removed, `#` touches the word
        // before.
        let continued = !gap.is_empty()
            && gap
                .split_inclusive('\n')
                .all(|line| matches!(line, "\\\n" | "\\\r\n"));
        let glued = leaf.kind() == "comment" && continued;
        // And text on both sides is one word (`echo a\` then `b` is
        // `echo ab`, and so is `$(echo a)\` then `b`), which the grammar
        // reads as two.
        let operator = |c: char| c.is_ascii_whitespace() || ";&|(<>".contains(c);
        if continued
            && src[..prev_end]
                .chars()
                .next_back()
                .is_some_and(|c| !operator(c))
            && src[start..].chars().next().is_some_and(|c| !operator(c))
        {
            skipped = true;
        }
        let marker = leaf.kind() == "heredoc_end"
            && !src[leaf.end_byte()..].starts_with(['\n', '\r'])
            && leaf.end_byte() < src.len();
        if marker || glued {
            return (skipped_at, Some(leaf.start_position().row));
        }
        if skipped {
            skipped_at.push(prev_end);
        }
        prev_end = leaf.end_byte();
    }
    (skipped_at, None)
}

/// The row of a `` ` `` substitution inside another, which bash would
/// close at the inner `` ` `` (nested ones are written `` \` ``).
fn nested_backticks(root: Node) -> Option<usize> {
    let backticks =
        |n: Node| n.kind() == "command_substitution" && n.child(0).is_some_and(|c| c.kind() == "`");
    let mut stack = vec![(root, false)];
    while let Some((node, inside)) = stack.pop() {
        let here = backticks(node);
        if here && inside {
            return Some(node.start_position().row);
        }
        stack.extend(children(node).into_iter().map(|k| (k, inside || here)));
    }
    None
}

/// The row of a command named by a reserved word that bash rejects there,
/// so the grammar has read part of a compound as plain words (a `[ ]` test
/// run on past its `]`, or assignments run on past a comment). A closing
/// or continuing word (`fi`, `then`, ...) counts where it starts its
/// command; an opening one (`{`, `for`, `function`, ...) where it starts
/// its line, since the grammar also reads `! {` and `coproc P {` groups as
/// commands, which keeps their layout. After an assignment on the same
/// line (`x=1 fi`) the word is an ordinary command name.
fn reserved_command(root: Node, src: &str) -> Option<usize> {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let text = &src[node.byte_range()];
        let starts_command = node
            .prev_sibling()
            .is_none_or(|p| p.end_position().row < node.start_position().row);
        let starts_line = || {
            src[..node.start_byte()]
                .rsplit('\n')
                .next()
                .is_some_and(|before| before.trim_ascii().is_empty())
        };
        if node.kind() == "command_name"
            && node.child_count() == 1
            && starts_command
            && (matches!(
                text,
                "fi" | "done" | "esac" | "then" | "do" | "else" | "elif"
            ) || matches!(
                text,
                "{" | "}"
                    | "if"
                    | "for"
                    | "while"
                    | "until"
                    | "case"
                    | "select"
                    | "function"
                    | "[["
            ) && starts_line())
        {
            return Some(node.start_position().row);
        }
        stack.extend(children(node).into_iter().rev());
    }
    None
}

/// Whether `node`, a statement kept as written, holds part of a compound
/// command read as plain words at the start of lines: a header and body
/// it does not end (`if` and `then` without `fi`), or a keyword of the
/// compound around it (`else`). The grammar has read the rest as separate
/// statements.
fn swallows_compound(node: Node, src: &str) -> bool {
    let mut open = 0;
    let mut body = false;
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() != "word" {
            stack.extend(children(n));
            continue;
        }
        let start = n.start_byte();
        let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
        let continued = src[..line_start]
            .trim_end_matches(['\n', '\r'])
            .ends_with('\\');
        if line_start <= node.start_byte()
            || continued
            || !src[line_start..start].trim_ascii().is_empty()
        {
            continue;
        }
        match &src[start..n.end_byte()] {
            "if" | "case" | "for" | "select" | "while" | "until" => open += 1,
            "fi" | "esac" | "done" => open -= 1,
            "then" | "do" | "else" | "elif" if open == 0 => return true,
            "then" | "do" => body = true,
            _ => {}
        }
    }
    open > 0 && body
}

/// Whether `before` ends with a `{name}` or a number touching the
/// redirection after it, which bash reads as its file descriptor
/// (`exec {fd}>file`, `exec 0<&5`).
fn named_descriptor(before: &str) -> bool {
    let word_start = before
        .rfind([' ', '\t', '\n', ';', '&', '|', '('])
        .map_or(0, |i| i + 1);
    let word = &before[word_start..];
    if !word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    let Some(name) = word.strip_prefix('{').and_then(|w| w.strip_suffix('}')) else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn last_row(node: Node) -> usize {
    let end = node.end_position();
    if end.column == 0 && end.row > node.start_position().row {
        end.row - 1
    } else {
        end.row
    }
}

/// Whether `node` is a statement in a statement list (rather than syntax
/// such as `;`, `then`, `do` or a comment).
fn is_statement(node: Node) -> bool {
    node.is_named() && node.kind() != "comment"
}

fn contains_kind(node: Node, kinds: &[&str]) -> bool {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if kinds.contains(&n.kind()) {
            return true;
        }
        stack.extend(children(n));
    }
    false
}

struct Fmt<'a> {
    src: &'a str,
    config: &'a Config,
    out: String,
    /// Heredoc bodies (with their end markers) to emit after the current
    /// line.
    heredocs: Vec<String>,
    /// End of the last heredoc taken from the source.
    heredoc_end: usize,
    /// Depth of the statement being emitted, for substitutions inside it.
    depth: usize,
    /// Offsets of text the grammar skipped, and whether a statement kept
    /// as written holds each.
    skipped: Vec<usize>,
    covered: Vec<bool>,
    /// `inline` results by node.
    inlined: std::collections::HashMap<usize, Option<String>>,
    /// The row of a statement kept as written that holds part of a
    /// compound command.
    swallowed: Option<usize>,
}

impl<'a> Fmt<'a> {
    fn text(&self, node: Node) -> &'a str {
        &self.src[node.start_byte()..node.end_byte()]
    }

    fn indent(&self, depth: usize) -> String {
        " ".repeat(depth * self.config.bash.indent_width as usize)
    }

    fn flush_heredocs(&mut self) {
        for body in std::mem::take(&mut self.heredocs) {
            self.out.push('\n');
            self.out.push_str(&body);
        }
    }

    /// Ends the current line (after any pending heredoc bodies) and indents
    /// the next one.
    fn nl(&mut self, depth: usize) {
        self.flush_heredocs();
        self.out.push('\n');
        let indent = self.indent(depth);
        self.out.push_str(&indent);
    }

    /// One statement per line, keeping one blank line between statements
    /// (and after the keyword that opens the body) and comments on the line
    /// they trail. Returns the last source row emitted.
    fn block(&mut self, nodes: &[Node], depth: usize, mut started: bool) -> Option<usize> {
        let mut prev: Option<Node> = None;
        for &node in nodes {
            if node.kind() == "&" {
                self.out.push_str(" &");
                continue;
            }
            if node.kind() == "comment" {
                let row = node.start_position().row;
                // A comment can trail the previous statement, or the keyword
                // that opens the body (`then # note`).
                let trails = match prev {
                    Some(p) if self.heredocs.is_empty() => last_row(p) == row,
                    Some(p) => p.start_position().row == row,
                    None => started && node.prev_sibling().is_some_and(|k| last_row(k) == row),
                };
                if trails && !self.after_comment() {
                    self.out.push(TRAILING);
                } else {
                    self.line_for(node, self.prev_row(node, prev, started), depth, started);
                    started = true;
                }
                self.out.push_str(self.text(node).trim_ascii_end());
                prev = Some(node);
                continue;
            }
            if !is_statement(node) {
                continue;
            }
            self.line_for(node, self.prev_row(node, prev, started), depth, started);
            started = true;
            self.stmt(node, depth);
            prev = Some(node);
        }
        prev.map(last_row)
    }

    /// The source row before `node`: the previous item, or the keyword that
    /// opens the body.
    fn prev_row(&self, node: Node, prev: Option<Node>, started: bool) -> Option<usize> {
        match prev {
            Some(p) => Some(last_row(p)),
            None if started => node.prev_sibling().map(last_row),
            None => None,
        }
    }

    /// Starts the line of a closing or continuing keyword on `row`, keeping
    /// one blank line after the body's last item (`last`) when the source
    /// has one.
    fn close(&mut self, depth: usize, last: Option<usize>, row: usize) {
        if last.is_some_and(|r| row > r + 1) {
            self.flush_heredocs();
            self.out.push('\n');
        }
        self.nl(depth);
    }

    fn line_for(&mut self, node: Node, prev_row: Option<usize>, depth: usize, started: bool) {
        if !started && self.out.is_empty() {
            return;
        }
        if !started {
            self.nl(depth);
            return;
        }
        let blank = prev_row.is_some_and(|r| node.start_position().row > r + 1);
        if blank {
            self.flush_heredocs();
            self.out.push('\n');
        }
        self.nl(depth);
    }

    /// Renders `node` on its own, returning `None` when it needs more than
    /// one line.
    fn inline(&mut self, node: Node) -> Option<String> {
        // Each compound asks this of its bodies while rendering itself, so
        // the answer is kept for nested compounds.
        if let Some(known) = self.inlined.get(&node.id()) {
            return known.clone();
        }
        let text = if contains_kind(node, &["heredoc_redirect", "comment"]) {
            None
        } else {
            let saved = std::mem::take(&mut self.out);
            self.stmt(node, 0);
            let text = std::mem::replace(&mut self.out, saved);
            (!text.contains('\n')).then_some(text)
        };
        self.inlined.insert(node.id(), text.clone());
        text
    }

    fn stmt(&mut self, node: Node, depth: usize) {
        // A statement already rendered on one line, where the line so far
        // gives it the same start (see `continuation`).
        if self.out.is_empty() || self.out.ends_with([' ', '\n']) {
            if let Some(Some(text)) = self.inlined.get(&node.id()) {
                let text = text.clone();
                self.out.push_str(&text);
                return;
            }
        }
        let saved = std::mem::replace(&mut self.depth, depth);
        self.stmt_at(node, depth);
        self.depth = saved;
    }

    fn stmt_at(&mut self, node: Node, depth: usize) {
        let skipped = self.holds_skipped(node);
        if skipped || self.joins_lines(node) {
            // The grammar has read later lines (statements, comments,
            // keywords) as words of one command; keep the statement
            // exactly as written.
            let text = super::unescaped_trim_end(self.text(node)).replace("\r\n", "\n");
            if swallows_compound(node, self.src) {
                self.swallowed.get_or_insert(node.start_position().row);
            }
            self.out.push_str(&text);
            return;
        }
        match node.kind() {
            "command" | "declaration_command" | "unset_command" => self.words(node, depth),
            "variable_assignment" => {
                let text = self.word(node);
                self.out.push_str(&text);
            }
            "variable_assignments" => self.words(node, depth),
            "redirected_statement" if self.misparsed(node) => {
                // The grammar has read text from later lines as part of a
                // redirection; keep the statement exactly as written.
                let text = super::unescaped_trim_end(self.text(node)).replace("\r\n", "\n");
                self.out.push_str(&text);
            }
            "redirected_statement" => {
                for kid in children(node) {
                    match kid.kind() {
                        "file_redirect" | "heredoc_redirect" | "herestring_redirect" => {
                            self.continuation(kid, depth);
                            self.redirect(kid);
                        }
                        _ if kid.is_named() => self.stmt(kid, depth),
                        _ => {}
                    }
                }
            }
            "pipeline" | "list" => self.chain(node, depth),
            "negated_command" => {
                self.out.push_str("! ");
                if let Some(inner) = node.named_child(0) {
                    self.stmt(inner, depth);
                }
            }
            "subshell" => self.grouped(node, "(", ")", depth),
            "compound_statement" if node.child(0).is_some_and(|c| c.kind() == "((") => {
                // `(( i++ ))`, an arithmetic command.
                let inner: Vec<String> = children(node)
                    .into_iter()
                    .filter(|k| k.is_named())
                    .map(|k| self.arith(k))
                    .collect();
                self.out.push_str(&format!("(({}))", inner.join(", ")));
            }
            "compound_statement" => self.grouped(node, "{", "}", depth),
            "function_definition" => self.function(node, depth),
            "if_statement" => self.if_statement(node, depth),
            "for_statement"
            | "select_statement"
            | "while_statement"
            | "until_statement"
            | "c_style_for_statement" => self.looping(node, depth),
            "case_statement" => self.case(node, depth),
            "test_command" => {
                let text = self.test(node);
                self.out.push_str(&text);
            }
            "arithmetic_expansion" => {
                let text = self.word(node);
                self.out.push_str(&text);
            }
            _ => {
                let text = self.word(node);
                self.out.push_str(&text);
            }
        }
    }

    /// A backslash-newline between `prev` and `node` in the source is kept,
    /// with the continuation indented one level.
    fn continuation(&mut self, node: Node, depth: usize) {
        let before = &self.src[..node.start_byte()];
        let gap_start = before.trim_end_matches([' ', '\t']).len();
        if named_descriptor(before) {
            // `{fd}>file`: the name is part of the redirection, which the
            // grammar reads as a word before it.
        } else if before[..gap_start].ends_with("\\\n") || before[..gap_start].ends_with("\\\r\n") {
            self.out.push_str(" \\");
            self.nl(depth + 1);
        } else if !self.out.is_empty() && !self.out.ends_with([' ', '\n']) {
            self.out.push(' ');
        }
    }

    /// A simple command: its words separated by single spaces.
    fn words(&mut self, node: Node, depth: usize) {
        let mut first = true;
        let mut prev_end = None;
        for kid in children(node) {
            if kid.kind() == "comment" {
                continue;
            }
            // Words with nothing between them (`a$B.c`) are one argument.
            if first {
                first = false;
            } else if prev_end != Some(kid.start_byte()) {
                self.separator(kid, depth);
            }
            prev_end = Some(kid.end_byte());
            match kid.kind() {
                "file_redirect" | "heredoc_redirect" | "herestring_redirect" => self.redirect(kid),
                _ => {
                    let text = self.word(kid);
                    self.out.push_str(&text);
                }
            }
        }
    }

    /// A space between words, or a kept backslash-newline. A plain newline
    /// between words is kept too: the grammar can read `a=1` on one line
    /// and `cmd` on the next as one command, which Bash does not.
    fn separator(&mut self, node: Node, depth: usize) {
        let before = &self.src[..node.start_byte()];
        let gap = before.trim_end_matches([' ', '\t']);
        if gap.ends_with("\\\n") || gap.ends_with("\\\r\n") {
            self.out.push_str(" \\");
            self.nl(depth + 1);
        } else if gap.ends_with('\n') {
            self.nl(depth);
        } else {
            self.out.push(' ');
        }
    }

    fn redirect(&mut self, node: Node) {
        let mut op = String::new();
        let mut target = String::new();
        // The grammar puts the rest of a heredoc's line (`|| true`,
        // `| grep x`) inside the redirect, after its start marker.
        let mut tail: Vec<Node> = Vec::new();
        let mut started = false;
        let mut prev_end = None;
        let mut rest = String::new();
        for kid in children(node) {
            match kid.kind() {
                "file_descriptor" => op.push_str(self.text(kid)),
                "heredoc_start" => {
                    target = self.text(kid).to_string();
                    started = true;
                }
                "heredoc_body" | "heredoc_end" | "heredoc_content" => {}
                _ if started => tail.push(kid),
                _ if !kid.is_named() => op.push_str(self.text(kid)),
                // The target, and pieces of it written without a space.
                _ if target.is_empty()
                    || (rest.is_empty() && prev_end == Some(kid.start_byte())) =>
                {
                    target.push_str(&self.word(kid));
                    prev_end = Some(kid.end_byte());
                }
                // The grammar puts arguments written after a redirect
                // (`>&2 "msg"`) inside it.
                _ => {
                    if prev_end != Some(kid.start_byte()) {
                        rest.push(' ');
                    }
                    rest.push_str(&self.word(kid));
                    prev_end = Some(kid.end_byte());
                }
            }
        }
        // `>&2` keeps its target attached; other operators take a space.
        let attached = (op.ends_with(">&") || op.ends_with("<&"))
            && target.chars().all(|c| c.is_ascii_digit() || c == '-');
        self.out.push_str(&op);
        if !attached && !target.is_empty() {
            self.out.push(' ');
        }
        self.out.push_str(&target);
        self.out.push_str(&rest);
        self.heredoc_tail(&tail);
        if node.kind() == "heredoc_redirect" {
            // The body starts on the line after the `<<` (or after the
            // previous heredoc, when one line starts several), so leading
            // blank lines and `<<-` tabs, which the body node leaves out, are
            // kept. It runs through the end marker.
            let kids = children(node);
            let marker = kids.iter().find(|k| k.kind() == "heredoc_start");
            let end = kids.iter().find(|k| k.kind() == "heredoc_end");
            if let (Some(marker), Some(end)) = (marker, end) {
                let after_line = self.src[marker.end_byte()..]
                    .find('\n')
                    .map_or(self.src.len(), |i| marker.end_byte() + i + 1);
                let after_previous = self.src[self.heredoc_end..]
                    .find('\n')
                    .map_or(self.src.len(), |i| self.heredoc_end + i + 1);
                let start = if self.heredoc_end > marker.end_byte() {
                    after_previous
                } else {
                    after_line
                };
                let start = start.min(end.start_byte());
                self.heredocs
                    .push(self.src[start..end.end_byte()].to_string());
                self.heredoc_end = end.end_byte();
            }
        }
    }

    /// Whether `node`, a statement that is not a compound command, holds
    /// text the grammar skipped, which it then marks as kept. A heredoc
    /// whose node runs over the rest of its line cannot be kept alone.
    fn holds_skipped(&mut self, node: Node) -> bool {
        if matches!(
            node.kind(),
            "function_definition"
                | "compound_statement"
                | "subshell"
                | "if_statement"
                | "while_statement"
                | "for_statement"
                | "c_style_for_statement"
                | "select_statement"
        ) || node
            .next_sibling()
            .is_some_and(|s| s.start_byte() < node.end_byte())
        {
            return false;
        }
        let range = node.start_byte()..node.end_byte();
        let mut held = false;
        for (i, at) in self.skipped.iter().enumerate() {
            if range.start < *at && *at < range.end {
                self.covered[i] = true;
                held = true;
            }
        }
        held
    }

    /// Whether a command inside `node` holds a comment, has words on
    /// different lines with no backslash continuation between them, or has
    /// an assignment touching the word after it. Bash reads these
    /// differently from the grammar: it ends a command at an unescaped
    /// newline, and an assignment and the text touching it are one word.
    /// Statements inside a compound command are checked when they are
    /// emitted, so only the statement holding the command is kept; a
    /// `case` item's statement touches its `;;`, so a `case` is kept whole.
    fn joins_lines(&self, node: Node) -> bool {
        if matches!(
            node.kind(),
            "command" | "declaration_command" | "unset_command"
        ) {
            let kids = children(node);
            // A comment, or one inside a redirection (`> # c`, invalid to
            // bash) that runs on into the next line.
            let commented = |k: &Node| k.kind() == "comment";
            if kids.iter().any(|k| {
                commented(k) || k.kind() == "file_redirect" && children(*k).iter().any(commented)
            }) {
                return true;
            }
            let joined = kids.windows(2).any(|pair| {
                // `a="x"`cmd`` is one word to bash, but the grammar reads an
                // assignment and a separate command.
                if pair[0].kind() == "variable_assignment"
                    && pair[0].end_byte() == pair[1].start_byte()
                {
                    return true;
                }
                self.broken(pair[0], pair[1])
            });
            if joined {
                return true;
            }
        }
        // A redirection whose words run on to later lines (`> out` then
        // `echo done` read as more file names), or that starts a line of
        // its own but is read as part of the statement before.
        if node.kind() == "file_redirect"
            && children(node)
                .windows(2)
                .any(|pair| self.broken(pair[0], pair[1]))
        {
            return true;
        }
        if node.kind() == "redirected_statement"
            && children(node).windows(2).any(|pair| {
                matches!(
                    pair[1].kind(),
                    "file_redirect" | "heredoc_redirect" | "herestring_redirect"
                ) && self.broken(pair[0], pair[1])
            })
        {
            return true;
        }
        // A comment inside `[[ ]]` or `(( ))` ends the line for bash; the
        // grammar reads on past it.
        let arithmetic = node.child(0).is_some_and(|c| c.kind() == "((");
        if (arithmetic || matches!(node.kind(), "test_command" | "arithmetic_expansion"))
            && contains_kind(node, &["comment"])
        {
            return true;
        }
        if matches!(
            node.kind(),
            "function_definition"
                | "compound_statement"
                | "subshell"
                | "if_statement"
                | "while_statement"
                | "for_statement"
                | "c_style_for_statement"
                | "select_statement"
        ) {
            return false;
        }
        children(node).into_iter().any(|k| self.joins_lines(k))
    }

    /// Whether the text between `a` and `b` holds a line break without a
    /// backslash before it, which ends a command for bash.
    fn broken(&self, a: Node, b: Node) -> bool {
        self.src[a.end_byte()..b.start_byte()]
            .split('\n')
            .rev()
            .skip(1)
            .any(|before| !before.trim_end_matches('\r').ends_with('\\'))
    }

    /// Whether the grammar has misread a redirection: heredoc body text
    /// parsed as part of the `<<` line, or a `#` right after a redirection
    /// operator (a comment to bash, so the line is invalid) whose parse
    /// swallows the next line.
    fn misparsed(&self, node: Node) -> bool {
        let commented = children(node)
            .iter()
            .filter(|k| k.kind() == "file_redirect")
            .any(|r| children(*r).iter().any(|k| k.kind() == "comment"));
        commented
            || children(node)
                .iter()
                .filter(|k| k.kind() == "heredoc_redirect")
                .any(|h| {
                    let kids = children(*h);
                    let Some(marker) = kids.iter().find(|k| k.kind() == "heredoc_start") else {
                        return false;
                    };
                    let row = marker.start_position().row;
                    kids.iter()
                        .filter(|k| {
                            !matches!(k.kind(), "heredoc_body" | "heredoc_end" | "heredoc_content")
                        })
                        .any(|k| last_row(*k) > row)
                })
    }

    fn heredoc_tail(&mut self, nodes: &[Node]) {
        for &n in nodes {
            if !n.is_named() {
                self.out.push(' ');
                self.out.push_str(self.text(n));
            } else if matches!(n.kind(), "pipeline" | "list")
                && n.child(0).is_some_and(|c| !c.is_named())
            {
                self.heredoc_tail(&children(n));
            } else if n.kind() == "comment" {
                self.out.push(TRAILING);
                self.out.push_str(self.text(n).trim_ascii_end());
            } else {
                // `<<EOF 1>&2 0<&3`: the grammar reads the `0` as part of
                // the redirect before it.
                if !named_descriptor(&self.src[..n.start_byte()]) {
                    self.out.push(' ');
                }
                self.stmt(n, 0);
            }
        }
    }

    /// `a && b || c`, `a | b`: operands joined by their operators; a break
    /// in the source moves the operator to the start of the next line.
    fn chain(&mut self, node: Node, depth: usize) {
        let mut parts: Vec<Node> = Vec::new();
        fn flatten<'t>(node: Node<'t>, kind: &str, parts: &mut Vec<Node<'t>>) {
            for kid in children(node) {
                if kid.kind() == kind {
                    flatten(kid, kind, parts);
                } else {
                    parts.push(kid);
                }
            }
        }
        flatten(node, node.kind(), &mut parts);
        let mut prev_end_row = None;
        let mut after_comment = false;
        for (i, part) in parts.iter().enumerate() {
            if part.kind() == "comment" {
                // As in Prettier: the operator ends its line, and the
                // comment and the next operand take their own lines.
                self.nl(depth + 1);
                self.out.push_str(self.text(*part).trim_ascii_end());
                after_comment = true;
            } else if part.is_named() {
                if after_comment {
                    self.nl(depth + 1);
                    after_comment = false;
                } else if i > 0 {
                    self.out.push(' ');
                }
                self.stmt(*part, depth);
                prev_end_row = Some(last_row(*part));
            } else {
                let next_row = parts.get(i + 1).map(|n| n.start_position().row);
                let broken = prev_end_row.is_some_and(|r| {
                    part.start_position().row > r || next_row.is_some_and(|n| n > r)
                });
                if broken || parts.get(i + 1).is_some_and(|n| n.kind() == "comment") {
                    self.out.push_str(" \\");
                    self.nl(depth + 1);
                    self.out.push_str(self.text(*part));
                } else {
                    self.out.push(' ');
                    self.out.push_str(self.text(*part));
                }
            }
        }
    }

    /// Statements of a compound body: the named children between `open` and
    /// `close` keywords.
    fn body_between<'t>(nodes: &[Node<'t>], open: &[&str], close: &[&str]) -> Vec<Node<'t>> {
        let start = nodes
            .iter()
            .position(|n| open.contains(&n.kind()))
            .map_or(0, |i| i + 1);
        let end = nodes[start..]
            .iter()
            .position(|n| close.contains(&n.kind()))
            .map_or(nodes.len(), |i| start + i);
        nodes[start..end].to_vec()
    }

    /// Whether a compound written on one line keeps its one-line form:
    /// every body holds at most one statement, which itself fits on a line.
    fn one_line(&mut self, node: Node, bodies: &[&[Node]]) -> bool {
        node.start_position().row == last_row(node)
            && !contains_kind(node, &["heredoc_redirect", "comment"])
            && bodies.iter().all(|b| {
                let statements: Vec<Node> =
                    b.iter().filter(|n| is_statement(**n)).copied().collect();
                statements.len() <= 1 && statements.iter().all(|n| self.inline(*n).is_some())
            })
    }

    /// Statements on one line, separated by `; `, or by ` & ` where the
    /// source runs one in the background.
    fn inline_stmts(&mut self, nodes: &[Node]) {
        let mut first = true;
        for &n in nodes {
            if n.kind() == "&" {
                self.out.push_str(" &");
            } else if is_statement(n) {
                if !first {
                    self.terminate();
                }
                first = false;
                self.stmt(n, 0);
            }
        }
    }

    /// Ends a statement on a line that continues: `; `, or only a space
    /// after a statement already ended by `&`.
    fn terminate(&mut self) {
        if self.out.ends_with(" &") {
            self.out.push(' ');
        } else {
            self.out.push_str("; ");
        }
    }

    fn grouped(&mut self, node: Node, open: &str, close: &str, depth: usize) {
        let kids = children(node);
        let body = Self::body_between(&kids, &[open], &[close]);
        if self.one_line(node, &[&body]) {
            if open == "(" {
                self.out.push('(');
                self.inline_stmts(&body);
                self.out.push(')');
            } else {
                self.out.push_str("{ ");
                self.inline_stmts(&body);
                self.terminate();
                self.out.push('}');
            }
            return;
        }
        self.out.push_str(open);
        let last = self.block(&body, depth + 1, true);
        let close_row = kids.last().map_or(0, |k| k.start_position().row);
        self.close(depth, last, close_row);
        self.out.push_str(close);
    }

    fn function(&mut self, node: Node, depth: usize) {
        let kids = children(node);
        let keyword = kids.iter().any(|k| k.kind() == "function");
        let parens = kids.iter().any(|k| k.kind() == "(");
        let name = node
            .child_by_field_name("name")
            .map_or("", |n| self.text(n));
        if keyword {
            self.out.push_str("function ");
        }
        self.out.push_str(name);
        if parens || !keyword {
            self.out.push_str("()");
        }
        self.out.push(' ');
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let comments: Vec<&str> = kids
            .iter()
            .filter(|k| k.kind() == "comment" && k.end_byte() <= body.start_byte())
            .map(|k| super::unescaped_trim_end(self.text(*k)))
            .collect();
        if comments.is_empty() {
            self.stmt(body, depth);
            self.body_redirects(&kids, body, depth);
            return;
        }
        // A comment before the body moves after its `{`, as Prettier does.
        let start = self.out.len();
        self.stmt(body, depth);
        let rendered = self.out[start..].to_string();
        match rendered.strip_prefix('{') {
            Some(rest) if rest.starts_with('\n') => {
                self.out.truncate(start);
                self.out
                    .push_str(&format!("{{ {}{rest}", comments.join(" ")));
                self.body_redirects(&kids, body, depth);
            }
            _ => {
                // A one-line or non-brace body cannot take a comment
                // without commenting out code; keep the definition as
                // written.
                let line_start = self.out[..start].rfind('\n').map_or(0, |i| i + 1);
                let prefix = self.out[line_start..start].len()
                    - self.out[line_start..start].trim_ascii_start().len();
                self.out.truncate(line_start + prefix);
                self.out
                    .push_str(&super::unescaped_trim_end(self.text(node)).replace("\r\n", "\n"));
            }
        }
    }

    /// Redirections the grammar attaches to a function definition after
    /// its body (`f() { ...; } 7>&2`).
    fn body_redirects(&mut self, kids: &[Node], body: Node, depth: usize) {
        for kid in kids.iter().filter(|k| k.start_byte() >= body.end_byte()) {
            if matches!(
                kid.kind(),
                "file_redirect" | "heredoc_redirect" | "herestring_redirect"
            ) {
                self.continuation(*kid, depth);
                self.redirect(*kid);
            }
        }
    }

    fn if_statement(&mut self, node: Node, depth: usize) {
        let kids = children(node);
        let cond = Self::body_between(&kids, &["if"], &["then"]);
        let body = Self::body_between(&kids, &["then"], &["elif_clause", "else_clause", "fi"]);
        let clauses: Vec<Node> = kids
            .iter()
            .filter(|k| matches!(k.kind(), "elif_clause" | "else_clause"))
            .copied()
            .collect();
        // A clause's body runs on past the clause node: comments after it
        // (before the next clause or `fi`) are children of the `if`.
        let clause_bodies: Vec<Vec<Node>> = clauses
            .iter()
            .map(|c| {
                let ck = children(*c);
                let mut body = if c.kind() == "elif_clause" {
                    Self::body_between(&ck, &["then"], &[])
                } else {
                    Self::body_between(&ck, &["else"], &[])
                };
                body.extend(
                    kids.iter()
                        .skip_while(|k| k.start_byte() < c.end_byte())
                        .take_while(|k| k.kind() == "comment"),
                );
                body
            })
            .collect();
        let fi = kids.last().copied().unwrap_or(node);
        // Each body's opening keyword (`then`, `else`) and the keyword that
        // follows it decide whether it stays on one line.
        let then_kw = kids
            .iter()
            .find(|k| k.kind() == "then")
            .copied()
            .unwrap_or(node);
        let mut next_kw: Vec<Node> = clauses.clone();
        next_kw.push(fi);

        self.condition(kids[0], &cond, "then", depth);
        let mut inline = self.body_inline(then_kw, &body, next_kw[0]) && !self.after_comment();
        let mut last = self.part(&body, depth, inline);
        for (i, (clause, cbody)) in clauses.iter().zip(&clause_bodies).enumerate() {
            self.separate(depth, inline, last, clause.start_position().row);
            let ck = children(*clause);
            let keyword = ck
                .iter()
                .find(|k| matches!(k.kind(), "then" | "else"))
                .copied()
                .unwrap_or(*clause);
            if clause.kind() == "elif_clause" {
                let ck = children(*clause);
                let ccond = Self::body_between(&ck, &["elif"], &["then"]);
                self.condition(ck[0], &ccond, "then", depth);
            } else {
                self.out.push_str("else");
            }
            inline = self.body_inline(keyword, cbody, next_kw[i + 1]) && !self.after_comment();
            last = self.part(cbody, depth, inline);
        }
        self.separate(depth, inline, last, fi.start_position().row);
        self.out.push_str("fi");
    }

    /// Whether a body stays on its keyword's line, as in shfmt: a single
    /// statement that fits on a line, written on the keyword's line, with
    /// the next keyword (`else`, `fi`, `done`) on the line it ends.
    fn body_inline(&mut self, keyword: Node, body: &[Node], next: Node) -> bool {
        let statements: Vec<Node> = body.iter().filter(|n| is_statement(**n)).copied().collect();
        match statements.as_slice() {
            [only] => {
                !body.iter().any(|n| n.kind() == "comment")
                    && only.start_position().row == last_row(keyword)
                    && next.start_position().row == last_row(*only)
                    && self.inline(*only).is_some()
            }
            _ => false,
        }
    }

    /// `if`, `elif`, `while` or `until` and its condition, then `close`
    /// (`then`, `do`). A condition written on the keyword's line stays on
    /// it; one that continues on later lines (more statements, comments)
    /// gets its own lines one level deeper, with `close` on a line of its
    /// own, as shfmt prints it.
    fn condition(&mut self, keyword: Node, cond: &[Node], close: &str, depth: usize) {
        let row = keyword.start_position().row;
        self.out.push_str(keyword.kind());
        if !cond
            .iter()
            .any(|n| n.is_named() && n.start_position().row > row)
        {
            self.out.push(' ');
            self.inline_stmts(cond);
            if cond
                .iter()
                .any(|n| contains_kind(*n, &["heredoc_redirect"]))
            {
                // The grammar reads `<< EOF; then` as the marker `EOF;`, so
                // `close` goes on the line after the heredoc, which also
                // ends a heredoc kept as written.
                self.nl(depth);
            } else {
                self.terminate();
            }
            self.out.push_str(close);
            self.header_comments(cond, depth);
            return;
        }
        let mut rest = cond;
        if let Some((first, after)) = cond.split_first() {
            if first.kind() == "comment" && first.start_position().row == row {
                self.out.push(' ');
                self.out.push_str(self.text(*first).trim_ascii_end());
                rest = after;
            }
        }
        self.block(rest, depth + 1, true);
        self.nl(depth);
        self.out.push_str(close);
    }

    /// Comments in a header (`while [ x ]; # note`) trail its `then` /
    /// `do`, as in Prettier.
    /// Further comments take their own lines in the body, at `depth + 1`.
    fn header_comments(&mut self, nodes: &[Node], depth: usize) {
        let trailed = self.after_comment();
        for (i, c) in nodes.iter().filter(|n| n.kind() == "comment").enumerate() {
            if i == 0 && !trailed {
                self.out.push(TRAILING);
            } else {
                self.nl(depth + 1);
            }
            self.out.push_str(self.text(*c).trim_ascii_end());
        }
    }

    /// Whether the current output line ends in a comment.
    fn after_comment(&self) -> bool {
        self.out
            .rsplit('\n')
            .next()
            .is_some_and(|line| line.contains(TRAILING) || line.trim_ascii_start().starts_with('#'))
    }

    /// A compound's body after its opening keyword: on the same line when
    /// `inline`, else indented below it.
    fn part(&mut self, body: &[Node], depth: usize, inline: bool) -> Option<usize> {
        if inline {
            if body.iter().any(|n| is_statement(*n)) {
                self.out.push(' ');
                self.inline_stmts(body);
            }
            None
        } else {
            self.block(body, depth + 1, true)
        }
    }

    /// Before a closing or continuing keyword (`else`, `fi`, `done`) on
    /// source row `row`.
    fn separate(&mut self, depth: usize, inline: bool, last: Option<usize>, row: usize) {
        if inline {
            self.terminate();
        } else {
            self.close(depth, last, row);
        }
    }

    fn looping(&mut self, node: Node, depth: usize) {
        let kids = children(node);
        let Some(group) = kids.iter().find(|k| k.kind() == "do_group").copied() else {
            let text = self.text(node).to_string();
            self.out.push_str(&text);
            return;
        };
        let gk = children(group);
        let body = Self::body_between(&gk, &["do"], &["done"]);
        let do_kw = gk.first().copied().unwrap_or(group);
        let done_kw = gk.last().copied().unwrap_or(group);
        let mut inline = self.body_inline(do_kw, &body, done_kw);
        let header: Vec<Node> = kids
            .iter()
            .filter(|k| k.start_byte() < group.start_byte())
            .copied()
            .collect();
        match node.kind() {
            "c_style_for_statement" => {
                let inside = Self::body_between(&header, &["(("], &["))"]);
                let mut parts = vec![String::new()];
                for n in inside {
                    if n.kind() == ";" {
                        parts.push(String::new());
                    } else if n.is_named() {
                        let text = self.arith(n);
                        let last = parts.last_mut().expect("never empty");
                        if !last.is_empty() {
                            last.push_str(", ");
                        }
                        last.push_str(&text);
                    }
                }
                self.out.push_str(&format!("for (({}))", parts.join("; ")));
            }
            "for_statement" | "select_statement" => {
                let mut words: Vec<String> = Vec::new();
                // Values written without a space are one word; the grammar
                // splits `$1/$2` before the `2`.
                let mut value_end = None;
                for n in &header {
                    match n.kind() {
                        "for" | "select" | "in" => words.push(n.kind().to_string()),
                        ";" | "comment" => {}
                        _ if n.is_named() => {
                            let text = self.word(*n);
                            match words.last_mut() {
                                Some(last) if value_end == Some(n.start_byte()) => {
                                    last.push_str(&text)
                                }
                                _ => words.push(text),
                            }
                            value_end = Some(n.end_byte());
                            continue;
                        }
                        _ => {}
                    }
                    value_end = None;
                }
                self.out.push_str(&words.join(" "));
            }
            _ => {
                let keyword = header.first().copied().unwrap_or(node);
                let cond = Self::body_between(&header, &[keyword.kind()], &[]);
                self.condition(keyword, &cond, "do", depth);
            }
        }
        if matches!(
            node.kind(),
            "c_style_for_statement" | "for_statement" | "select_statement"
        ) {
            self.out.push_str("; do");
            self.header_comments(&header, depth);
        }
        // A body after a header comment cannot share its line.
        inline &= !self.after_comment();
        let last = self.part(&body, depth, inline);
        let done_row = gk.last().map_or(0, |k| k.start_position().row);
        self.separate(depth, inline, last, done_row);
        self.out.push_str("done");
    }

    fn case(&mut self, node: Node, depth: usize) {
        let kids = children(node);
        let value = node
            .child_by_field_name("value")
            .map(|v| self.word(v))
            .unwrap_or_default();
        let items: Vec<Node> = kids
            .iter()
            .filter(|k| k.kind() == "case_item")
            .copied()
            .collect();
        let item_bodies: Vec<Vec<Node>> = items
            .iter()
            .map(|i| Self::body_between(&children(*i), &[")"], &[";;", ";&", ";;&"]))
            .collect();
        let bodies: Vec<&[Node]> = item_bodies.iter().map(Vec::as_slice).collect();
        let inline = self.one_line(node, &bodies);
        self.out.push_str(&format!("case {value} in"));
        // A comment before `in` trails it.
        let header: Vec<Node> = kids
            .iter()
            .take_while(|k| k.kind() != "in")
            .copied()
            .collect();
        self.header_comments(&header, depth);
        let mut item_bodies = item_bodies.iter();
        let mut prev_row = kids
            .iter()
            .find(|k| k.kind() == "in")
            .map(|k| k.start_position().row);
        let after_in = kids.iter().skip_while(|k| k.kind() != "in").skip(1);
        for kid in after_in {
            match kid.kind() {
                "case_item" => {
                    if inline {
                        self.out.push(' ');
                    } else {
                        self.close(depth + 1, prev_row, kid.start_position().row);
                    }
                    let body = item_bodies.next().map(Vec::as_slice).unwrap_or(&[]);
                    self.case_item(*kid, body, depth + 1, inline);
                    prev_row = Some(last_row(*kid));
                }
                // A comment after an item belongs to that item's body, as in
                // Prettier; one before the first item sits with the items.
                "comment" => {
                    let item_seen = kids
                        .iter()
                        .any(|k| k.kind() == "case_item" && k.end_byte() <= kid.start_byte());
                    let level = if item_seen { depth + 2 } else { depth + 1 };
                    self.comment_after(*kid, prev_row, level);
                    prev_row = Some(kid.start_position().row);
                }
                _ => {}
            }
        }
        if inline {
            self.out.push_str(" esac");
        } else {
            let esac_row = kids.last().map_or(0, |k| k.start_position().row);
            self.close(depth, prev_row, esac_row);
            self.out.push_str("esac");
        }
    }

    /// A comment trailing the line that ended on `prev_row`, or on a line
    /// of its own.
    fn comment_after(&mut self, comment: Node, prev_row: Option<usize>, depth: usize) {
        if prev_row == Some(comment.start_position().row) {
            self.out.push(TRAILING);
        } else {
            self.nl(depth);
        }
        self.out.push_str(self.text(comment).trim_ascii_end());
    }

    fn case_item(&mut self, item: Node, body: &[Node], depth: usize, case_inline: bool) {
        let kids = children(item);
        let close = kids.iter().position(|k| k.kind() == ")").unwrap_or(0);
        // Alternatives are separated by `|`; the nodes of one alternative
        // (`*"$x"*`) are joined as written.
        let mut patterns: Vec<String> = vec![String::new()];
        let mut prev_end = None;
        for k in &kids[..close] {
            match k.kind() {
                "|" => {
                    // `\|` is a literal `|` inside one pattern, though the
                    // grammar splits there.
                    let last = patterns.last_mut().expect("never empty");
                    let slashes = last.len() - last.trim_end_matches('\\').len();
                    if prev_end == Some(k.start_byte()) && slashes % 2 == 1 {
                        last.push('|');
                        prev_end = Some(k.end_byte());
                    } else {
                        patterns.push(String::new());
                        prev_end = None;
                    }
                }
                "(" | "comment" => {}
                _ => {
                    let text = self.word(*k);
                    let last = patterns.last_mut().expect("never empty");
                    if prev_end.is_some_and(|e| e != k.start_byte()) {
                        last.push_str(&self.src[prev_end.unwrap_or(0)..k.start_byte()]);
                    }
                    last.push_str(&text);
                    prev_end = Some(k.end_byte());
                }
            }
        }
        let terminator = kids
            .iter()
            .find(|k| matches!(k.kind(), ";;" | ";&" | ";;&"))
            .map_or(";;", |t| self.text(*t));
        // The grammar can take the blank after `\\` into a pattern.
        let patterns: Vec<&str> = patterns
            .iter()
            .map(|p| super::unescaped_trim_end(p))
            .collect();
        self.out.push_str(&patterns.join(" | "));
        self.out.push(')');
        let statements = body.iter().filter(|n| is_statement(**n)).count();
        let item_inline = case_inline || self.one_line(item, &[body]);
        if item_inline {
            if statements > 0 {
                self.out.push(' ');
                self.inline_stmts(body);
            }
            self.out.push(' ');
            self.out.push_str(terminator);
        } else {
            let last = self.block(body, depth + 1, true);
            let term_row = kids
                .iter()
                .find(|k| matches!(k.kind(), ";;" | ";&" | ";;&"))
                .map_or(last.unwrap_or(0), |t| t.start_position().row);
            self.close(depth + 1, last, term_row);
            self.out.push_str(terminator);
        }
        // Comments after the terminator, inside the item.
        let mut prev_row = kids
            .iter()
            .find(|k| matches!(k.kind(), ";;" | ";&" | ";;&"))
            .map(|t| t.start_position().row);
        if let Some(term) = kids
            .iter()
            .position(|k| matches!(k.kind(), ";;" | ";&" | ";;&"))
        {
            for c in kids[term + 1..].iter().filter(|k| k.kind() == "comment") {
                self.comment_after(*c, prev_row, depth + 1);
                prev_row = Some(c.start_position().row);
            }
        }
    }

    /// `[ ... ]` and `[[ ... ]]` with single spaces between operands.
    fn test(&mut self, node: Node) -> String {
        let parts: Vec<String> = children(node)
            .into_iter()
            .filter(|k| k.kind() != "comment")
            .map(|k| self.test_part(k))
            .collect();
        parts.join(" ")
    }

    fn test_part(&mut self, node: Node) -> String {
        match node.kind() {
            "binary_expression" | "unary_expression" | "negation_expression" => {
                // Tokens with nothing between them (`-*=`) stay together.
                let mut text = String::new();
                let mut prev_end = None;
                for kid in children(node) {
                    if prev_end.is_some_and(|e| e != kid.start_byte()) {
                        text.push(' ');
                    }
                    text.push_str(&self.test_part(kid));
                    prev_end = Some(kid.end_byte());
                }
                text
            }
            "parenthesized_expression" => {
                let inner: Vec<String> = children(node)
                    .into_iter()
                    .filter(|k| k.is_named())
                    .map(|k| self.test_part(k))
                    .collect();
                format!("( {} )", inner.join(" "))
            }
            _ => self.word(node),
        }
    }

    /// Arithmetic with spaces around binary operators: `1 + 2`, `i++`.
    fn arith(&mut self, node: Node) -> String {
        match node.kind() {
            "binary_expression" => {
                let kids = children(node);
                let parts: Vec<String> = kids.iter().map(|k| self.arith(*k)).collect();
                parts.join(" ")
            }
            "unary_expression" | "postfix_expression" => {
                children(node).into_iter().map(|k| self.arith(k)).collect()
            }
            "parenthesized_expression" => {
                let inner: Vec<String> = children(node)
                    .into_iter()
                    .filter(|k| k.is_named())
                    .map(|k| self.arith(k))
                    .collect();
                format!("({})", inner.join(" "))
            }
            "ternary_expression" => {
                let parts: Vec<String> =
                    children(node).into_iter().map(|k| self.arith(k)).collect();
                parts.join(" ")
            }
            _ => self.word(node),
        }
    }

    /// An array written over several lines keeps its line breaks, as
    /// shfmt does: the elements of each source line stay together one
    /// level deeper, comments stay where they are, and `(` and `)` keep
    /// or drop their own lines as written.
    fn multiline_array(&mut self, node: Node) -> String {
        let items: Vec<Node> = children(node)
            .into_iter()
            .filter(|k| k.is_named())
            .collect();
        let open_row = node.start_position().row;
        let close_row = last_row(node);
        let inner = self.indent(self.depth + 1);
        let mut text = String::from("(");
        let mut row = open_row;
        let mut line_has_items = false;
        let mut prev_end = None;
        for item in &items {
            let item_row = item.start_position().row;
            if prev_end == Some(item.start_byte()) && item.kind() != "comment" {
                // Written without a space: part of the item before.
            } else if item_row > row {
                if item_row > row + 1 && row != open_row {
                    text.push('\n');
                }
                text.push('\n');
                text.push_str(&inner);
            } else if line_has_items {
                text.push(if item.kind() == "comment" {
                    TRAILING
                } else {
                    ' '
                });
            }
            if item.kind() == "comment" {
                text.push_str(self.text(*item).trim_ascii_end());
            } else {
                text.push_str(&self.word(*item));
            }
            line_has_items = true;
            row = last_row(*item);
            prev_end = Some(item.end_byte());
        }
        if close_row > row {
            text.push('\n');
            text.push_str(&self.indent(self.depth));
        }
        text.push(')');
        text
    }

    fn word(&mut self, node: Node) -> String {
        match node.kind() {
            "command_substitution" => {
                let kids = children(node);
                let backtick = kids.first().is_some_and(|k| k.kind() == "`");
                // `\\`, `\$`, `\`` (and, inside double quotes, `\"`) mean
                // something different inside backticks than inside `$( )`,
                // so a substitution using them keeps its backticks.
                // A backtick inside the substitution means the grammar
                // has paired the wrong backticks (`a` `b` read as one).
                let text = self.text(node);
                let nested = text.len() > 2 && text[1..text.len() - 1].contains('`');
                if backtick
                    && (nested
                        || ["\\\\", "\\$", "\\`", "\\\""]
                            .iter()
                            .any(|e| text.contains(e)))
                {
                    return text.to_string();
                }
                let inner: Vec<Node> = kids.iter().filter(|k| k.is_named()).copied().collect();
                let multi_line = node.start_position().row != last_row(node);
                if !multi_line {
                    let mut inner = String::new();
                    for &n in &kids {
                        if n.kind() == "&" {
                            inner.push_str(" &");
                        } else if is_statement(n) {
                            match self.inline(n) {
                                Some(text) if inner.is_empty() => inner = text,
                                Some(text) if inner.ends_with(" &") => {
                                    inner.push(' ');
                                    inner.push_str(&text);
                                }
                                Some(text) => {
                                    inner.push_str("; ");
                                    inner.push_str(&text);
                                }
                                None => return self.text(node).to_string(),
                            }
                        }
                    }
                    // `$( (a) )` must not become `$((a))`, which is arithmetic.
                    let space = if inner.starts_with('(') { " " } else { "" };
                    return format!("$({space}{inner})");
                }
                // A multi-line `$( )` holding one statement is laid out like
                // that statement, continuing at the enclosing depth.
                let statements: Vec<Node> = inner
                    .iter()
                    .filter(|n| is_statement(**n))
                    .copied()
                    .collect();
                if let [only] = statements.as_slice() {
                    if !backtick && !contains_kind(node, &["heredoc_redirect", "comment"]) {
                        let saved = std::mem::take(&mut self.out);
                        let depth = self.depth;
                        self.stmt(*only, depth);
                        let text = std::mem::replace(&mut self.out, saved);
                        let space = if text.starts_with('(') { " " } else { "" };
                        return format!("$({space}{text})");
                    }
                }
                self.text(node).to_string()
            }
            "arithmetic_expansion" => {
                let inner: Vec<String> = children(node)
                    .into_iter()
                    .filter(|k| k.is_named())
                    .map(|k| self.arith(k))
                    .collect();
                format!("$(({}))", inner.join(", "))
            }
            "array" if node.start_position().row != last_row(node) => self.multiline_array(node),
            "array" => {
                let mut items: Vec<String> = Vec::new();
                let mut prev_end = None;
                for k in children(node) {
                    if !k.is_named() || k.kind() == "comment" {
                        continue;
                    }
                    let text = self.word(k);
                    // Items written without a space are one item; the
                    // grammar splits `$1/$2` before the `2`.
                    match items.last_mut() {
                        Some(last) if prev_end == Some(k.start_byte()) => last.push_str(&text),
                        _ => items.push(text),
                    }
                    prev_end = Some(k.end_byte());
                }
                format!("({})", items.join(" "))
            }
            "variable_assignment" => {
                let mut text = String::new();
                for kid in children(node) {
                    if kid.is_named() {
                        text.push_str(&self.word(kid));
                    } else {
                        text.push_str(self.text(kid));
                    }
                }
                text
            }
            "string"
                if children(node).iter().any(|k| {
                    matches!(k.kind(), "command_substitution" | "arithmetic_expansion")
                }) =>
            {
                // Substitutions inside double quotes are formatted too.
                let mut text = String::new();
                let mut pos = node.start_byte();
                for kid in children(node) {
                    text.push_str(&self.src[pos..kid.start_byte()]);
                    if matches!(kid.kind(), "command_substitution" | "arithmetic_expansion") {
                        // Inside a string the grammar can include the space
                        // before `$((` in the expansion's first token.
                        let raw = self.text(kid);
                        let word = self.word(kid);
                        if !word.starts_with([' ', '\t']) {
                            text.push_str(&raw[..raw.len() - raw.trim_ascii_start().len()]);
                        }
                        text.push_str(&word);
                    } else {
                        text.push_str(self.text(kid));
                    }
                    pos = kid.end_byte();
                }
                text.push_str(&self.src[pos..node.end_byte()]);
                text
            }
            "concatenation" => {
                let mut text = String::new();
                let mut pos = node.start_byte();
                for kid in children(node) {
                    text.push_str(&self.src[pos..kid.start_byte()]);
                    let raw = self.text(kid);
                    let word = self.word(kid);
                    // A word kept as written already has its leading space.
                    if !word.starts_with([' ', '\t']) {
                        text.push_str(&raw[..raw.len() - raw.trim_ascii_start().len()]);
                    }
                    text.push_str(&word);
                    pos = kid.end_byte();
                }
                text.push_str(&self.src[pos..node.end_byte()]);
                text
            }
            _ => self.text(node).to_string(),
        }
    }
}
