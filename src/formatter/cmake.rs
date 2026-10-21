// CMake formatter (`CMakeLists.txt`, `.cmake`): the structural target is
// gersemi at its defaults (4-space indent, line length 80).
//
// Bodies of `if`/`foreach`/`while`/`function`/`macro`/`block` are indented
// one level; one blank line between commands is kept; a trailing comment
// keeps one space before `#`. A built-in command (one with a signature in
// `cmake_signatures.rs`) has its name lowercased and stays on one line when
// it fits. Otherwise each argument group takes its own line: positional
// arguments and flags alone, a keyword with its value, and a multi-value
// keyword with its values on one line when they fit, else one per line
// below it. Conditions (`if`, `elseif`, `while`) break before each
// top-level `AND`/`OR`, and `set` keeps its variable on the opening line.
//
// As in gersemi, a command without a signature (user functions and
// macros) keeps its arguments as written; only its indentation, and the
// closing parenthesis of a multi-line call, change.
//
// Known gaps (documented rather than half-implemented):
//   - A file with any syntax error is returned unchanged.
//   - Signatures cover common built-in commands rather than all of them;
//     subcommand forms (`string`, `file`, `list`) are positional.

use super::cmake_signatures::{fixed_front, signature, Signature};
use crate::config::Config;
use crate::error::MoldyError;
use tree_sitter::Node;

pub fn format(source: &str, config: &Config) -> Result<String, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key("cmake")
        .ok_or_else(|| MoldyError::UnsupportedLanguage("cmake".to_string()))?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))?;

    let root = tree.root_node();
    if root.has_error() {
        return Err(super::syntax_error("CMake", root));
    }
    super::check_depth("CMake", root)?;
    if let Some(row) = deep_parens(root) {
        return Err(super::too_deep("CMake", row));
    }
    if let Some(row) = rejected(root, source) {
        return Err(MoldyError::Syntax {
            language: "CMake".to_string(),
            line: row + 1,
        });
    }

    let mut fmt = Fmt {
        src: source,
        config,
        lines: Vec::new(),
        last_row: None,
    };
    fmt.items(root, 0);
    let mut out = fmt.lines.join("\n").replace("\r\n", "\n");
    let trimmed = out.trim_ascii_end().len();
    out.truncate(trimmed);
    if config.newlines.final_newline && !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

/// The deepest parenthesized group the layout accepts. The grammar puts
/// a group's parentheses beside its arguments, but the layout nests a
/// level for each.
const MAX_PAREN_DEPTH: usize = 100;

/// The row of the first `(` nested more than `MAX_PAREN_DEPTH` deep.
fn deep_parens(root: Node) -> Option<usize> {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let mut depth = 0usize;
        for kid in children(node) {
            match kid.kind() {
                "(" => depth += 1,
                ")" => depth = depth.saturating_sub(1),
                _ => stack.push(kid),
            }
            if depth > MAX_PAREN_DEPTH {
                return Some(kid.start_position().row);
            }
        }
    }
    None
}

/// The row of the first construct the grammar accepts but CMake rejects:
/// a `\` before a line break outside quotes, or an argument touching the
/// end of a bracket comment or a bracket argument it should be separated
/// from ("Argument not separated").
fn rejected(root: Node, src: &str) -> Option<usize> {
    let mut stack = vec![root];
    let mut first: Option<usize> = None;
    while let Some(node) = stack.pop() {
        let text = &src[node.start_byte()..node.end_byte()];
        let bad = match node.kind() {
            // In a quoted argument it continues the line.
            "escape_sequence" => {
                text.ends_with(['\n', '\r'])
                    && node
                        .parent()
                        .is_some_and(|p| p.kind() == "unquoted_argument")
            }
            // An argument may end where a bracket comment starts, but not
            // start where one ends.
            "bracket_comment" => node
                .next_sibling()
                .is_some_and(|s| s.kind() == "argument" && s.start_byte() == node.end_byte()),
            // `(a)[[b]]`, `"a"[[b]]`, `[[a]]b`: only unquoted text may
            // touch a bracket argument before it.
            "argument" => node.prev_sibling().is_some_and(|prev| {
                prev.end_byte() == node.start_byte()
                    && (is_bracket(prev)
                        || (is_bracket(node)
                            && (prev.kind() == ")"
                                || prev.child(0).is_some_and(|c| c.kind() == "quoted_argument"))))
            }),
            _ => false,
        };
        if bad {
            let row = node.start_position().row;
            first = Some(first.map_or(row, |f| f.min(row)));
        }
        stack.extend(children(node));
    }
    first
}

fn is_bracket(node: Node) -> bool {
    node.kind() == "argument"
        && node
            .child(0)
            .is_some_and(|c| c.kind() == "bracket_argument")
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

fn is_comment(kind: &str) -> bool {
    matches!(kind, "line_comment" | "bracket_comment")
}

/// One argument (a parenthesized condition group counts as one), or an
/// own-line comment between arguments.
#[derive(Clone, Default)]
struct Arg {
    text: String,
    trailing: Option<String>,
    comment: bool,
    /// The arguments inside a parenthesized group.
    inner: Option<Vec<Arg>>,
}

impl Arg {
    fn new(text: String, comment: bool) -> Arg {
        Arg {
            text,
            comment,
            ..Arg::default()
        }
    }

    /// Whether the argument breaks any line it is on (a comment, or a
    /// multi-line value).
    fn breaks(&self) -> bool {
        self.comment
            || self.trailing.is_some()
            || self.text.contains('\n')
            || self
                .inner
                .as_ref()
                .is_some_and(|i| i.iter().any(Arg::breaks))
    }

    fn inline(&self) -> Option<&str> {
        (!self.breaks()).then_some(self.text.as_str())
    }
}

/// How a keyword's values are laid out when they do not fit on its line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// One value, kept beside the keyword.
    One,
    /// One value per line.
    List,
    /// Two values (name and value) per line.
    Pairs,
    /// As many values per line as fit (a command line).
    Pack,
    /// The first value heads the rest: `PROPERTY name` then the values.
    Headed,
}

enum Group {
    /// A positional argument, a flag, or an own-line comment.
    Item(Arg),
    Keyword {
        keyword: Arg,
        values: Vec<Arg>,
        layout: Layout,
    },
    Section {
        keyword: Arg,
        groups: Vec<Group>,
    },
}

/// The layout of value-taking keyword `word`, if `sig` has one.
fn layout(sig: &Signature, word: &str) -> Option<Layout> {
    if sig.one_value.contains(&word) {
        Some(Layout::One)
    } else if sig.multi_value.contains(&word) {
        Some(Layout::List)
    } else if sig.pairs.contains(&word) {
        Some(Layout::Pairs)
    } else if sig.packed.contains(&word) {
        Some(Layout::Pack)
    } else if sig.headed.contains(&word) {
        Some(Layout::Headed)
    } else {
        None
    }
}

fn is_keyword(sig: &Signature, word: &str) -> bool {
    sig.options.contains(&word)
        || layout(sig, word).is_some()
        || sig.sections.iter().any(|(k, _)| *k == word)
}

fn keyword_at(sig: &Signature, args: &[Arg], i: usize) -> bool {
    !args[i].comment && is_keyword(sig, &args[i].text)
}

/// Groups `args` by `sig`'s keywords.
fn parse_groups(mut args: Vec<Arg>, sig: &Signature) -> Vec<Group> {
    let mut groups = Vec::new();
    while !args.is_empty() {
        let word = args[0].text.clone();
        let is_kw = !args[0].comment;
        let section = sig.sections.iter().find(|(k, _)| is_kw && *k == word);
        let layout = layout(sig, &word).filter(|_| is_kw);
        let end = match (section, layout) {
            (Some((_, sub)), _) => (1..args.len())
                .find(|&j| keyword_at(sig, &args, j) && !keyword_at(sub, &args, j))
                .unwrap_or(args.len()),
            (None, Some(Layout::One)) => 2.min(args.len()),
            (None, Some(_)) => (1..args.len())
                .find(|&j| keyword_at(sig, &args, j))
                .unwrap_or(args.len()),
            (None, None) => 1,
        };
        let mut taken: Vec<Arg> = args.drain(..end).collect();
        let first = taken.remove(0);
        groups.push(match (section, layout) {
            (Some((_, sub)), _) => Group::Section {
                keyword: first,
                groups: parse_groups(taken, sub),
            },
            (None, Some(layout)) => Group::Keyword {
                keyword: first,
                values: taken,
                layout,
            },
            (None, None) => Group::Item(first),
        });
    }
    groups
}

fn group_inline(group: &Group) -> Option<String> {
    match group {
        Group::Item(arg) => arg.inline().map(str::to_string),
        Group::Keyword {
            keyword, values, ..
        } => {
            let mut parts = vec![keyword.inline()?.to_string()];
            for value in values {
                parts.push(value.inline()?.to_string());
            }
            Some(parts.join(" "))
        }
        Group::Section { keyword, groups } => {
            let mut parts = vec![keyword.inline()?.to_string()];
            for group in groups {
                parts.push(group_inline(group)?);
            }
            Some(parts.join(" "))
        }
    }
}

struct Fmt<'a> {
    src: &'a str,
    config: &'a Config,
    lines: Vec<String>,
    last_row: Option<usize>,
}

impl<'a> Fmt<'a> {
    fn text(&self, node: Node) -> &'a str {
        &self.src[node.start_byte()..node.end_byte()]
    }

    fn indent(&self, depth: usize) -> String {
        " ".repeat(depth * self.config.cmake.indent_width as usize)
    }

    fn fits(&self, line: &str) -> bool {
        line.chars().count() <= self.config.cmake.max_width as usize
    }

    /// The commands and comments of a file or block body.
    fn items(&mut self, container: Node, depth: usize) {
        let mut first = true;
        for kid in children(container) {
            if !kid.is_named() {
                continue;
            }
            let row = kid.start_position().row;
            if is_comment(kid.kind()) && self.last_row == Some(row) {
                let text = self.text(kid).trim_ascii_end().to_string();
                if let Some(line) = self.lines.last_mut() {
                    line.push(' ');
                    line.push_str(&text);
                }
                self.last_row = Some(last_row(kid));
                continue;
            }
            if !first && self.last_row.is_some_and(|r| row > r + 1) {
                self.lines.push(String::new());
            }
            first = false;
            match kid.kind() {
                "line_comment" | "bracket_comment" => {
                    let text = self.text(kid).trim_ascii_end();
                    self.push_text(depth, text);
                }
                "normal_command" => self.command(kid, depth),
                "body" => self.items(kid, depth + 1),
                k if k.ends_with("_condition") || k.ends_with("_loop") || k.ends_with("_def") => {
                    for part in children(kid) {
                        if part.kind() == "body" {
                            self.items(part, depth + 1);
                        } else if part.is_named() {
                            self.command(part, depth);
                            self.last_row = Some(last_row(part));
                        }
                    }
                }
                _ => {
                    let text = self.text(kid).trim_ascii_end();
                    self.push_text(depth, text);
                }
            }
            self.last_row = Some(last_row(kid));
        }
    }

    /// Pushes possibly multi-line `text`, indenting only its first line.
    fn push_text(&mut self, depth: usize, text: &str) {
        let indent = self.indent(depth);
        for (i, line) in text.split('\n').enumerate() {
            let line = line.trim_end_matches('\r');
            self.lines.push(if i == 0 {
                format!("{indent}{line}")
            } else {
                line.to_string()
            });
        }
    }

    fn args(&self, nodes: &[Node]) -> Vec<Arg> {
        // A parenthesized group is one argument, `(A OR B)`, that keeps its
        // inner arguments for layout.
        let mut stack: Vec<Vec<Arg>> = vec![Vec::new()];
        let mut prev_row = None;
        let mut prev_end = None;
        for &kid in nodes {
            let text = super::unescaped_trim_end(self.text(kid)).to_string();
            let nested = stack.len() > 1;
            let current = stack.last_mut().expect("stack is never empty");
            let touching = prev_end == Some(kid.start_byte());
            prev_end = Some(kid.end_byte());
            match kid.kind() {
                // Arguments with nothing between them, `A="${B}"`, are one
                // argument to CMake; text touching a group's `)` is not.
                "argument"
                    if touching
                        && current
                            .last()
                            .is_some_and(|a| !a.comment && a.inner.is_none()) =>
                {
                    if let Some(arg) = current.last_mut() {
                        arg.text.push_str(&text);
                    }
                }
                // After a trailing `#[[x]]`, a comment takes its own line,
                // as in gersemi.
                k if is_comment(k) => match current.last_mut() {
                    Some(arg)
                        if prev_row == Some(kid.start_position().row) && arg.trailing.is_none() =>
                    {
                        arg.trailing = Some(text);
                    }
                    _ => current.push(Arg::new(text, true)),
                },
                "(" => stack.push(Vec::new()),
                ")" if nested => {
                    let inner = stack.pop().expect("checked above");
                    let words: Vec<&str> = inner.iter().map(|a| a.text.as_str()).collect();
                    let group = Arg {
                        text: format!("({})", words.join(" ")),
                        inner: Some(inner),
                        ..Arg::default()
                    };
                    stack.last_mut().expect("outer level").push(group);
                }
                _ => current.push(Arg::new(text, false)),
            }
            prev_row = Some(last_row(kid));
        }
        while stack.len() > 1 {
            let inner = stack.pop().expect("checked above");
            stack.last_mut().expect("outer level").extend(inner);
        }
        stack.pop().unwrap_or_default()
    }

    fn command(&mut self, node: Node, depth: usize) {
        let kids = children(node);
        let Some(name_node) = kids.first() else {
            return;
        };
        let name = self.text(*name_node);
        let lower = name.to_ascii_lowercase();
        let indent = self.indent(depth);
        let sig = signature(&lower);
        let (Some(open), Some(close)) = (
            kids.iter().find(|k| k.kind() == "("),
            kids.iter().rev().find(|k| k.kind() == ")"),
        ) else {
            self.push_text(depth, super::unescaped_trim_end(self.text(node)));
            return;
        };

        let Some(sig) = sig else {
            self.unknown_command(node, &lower, open.end_byte(), close.start_byte(), depth);
            return;
        };

        let arg_nodes = match kids.iter().find(|k| k.kind() == "argument_list") {
            Some(list) => children(*list),
            // `endforeach(var)` has its arguments directly under the command.
            None => kids
                .iter()
                .filter(|k| k.start_byte() >= open.end_byte() && k.end_byte() <= close.start_byte())
                .copied()
                .collect(),
        };
        let mut args = self.args(&arg_nodes);
        if lower == "string" && args.len() > 1 && args[0].text == "REGEX" {
            // `REGEX MATCH` / `REGEX REPLACE` is one subcommand.
            let mode = args.remove(1);
            args[0].text = format!("REGEX {}", mode.text);
        }
        let condition = matches!(lower.as_str(), "if" | "elseif" | "while");
        let inline: Option<Vec<&str>> = args.iter().map(Arg::inline).collect();
        let line = inline.map(|words| format!("{indent}{lower}({})", words.join(" ")));

        if condition {
            match line {
                Some(line) if self.fits(&line) => self.lines.push(line),
                _ => {
                    self.lines.push(format!("{indent}{lower}("));
                    self.condition(&args, depth + 1);
                    self.lines.push(format!("{indent})"));
                }
            }
            return;
        }

        if lower == "file" && args.len() > 1 && args[0].text.starts_with("GLOB") {
            // `GLOB <variable>` is one group.
            let var = args.remove(1);
            args[0].text = format!("{} {}", args[0].text, var.text);
        }
        let mut opening = format!("{lower}(");
        let front = sig.front_inline && args.first().is_some_and(|a| a.inline().is_some());
        let front_arg = front.then(|| args.remove(0));
        let groups = parse_groups(args, sig);
        // gersemi expands a long argument list even when it fits.
        let positional = groups
            .iter()
            .take_while(|g| matches!(g, Group::Item(_)))
            .filter(|g| matches!(g, Group::Item(a) if !a.comment && !sig.options.contains(&a.text.as_str())))
            .count()
            + usize::from(front);
        let long_list = groups.iter().any(|g| matches!(g, Group::Keyword { values, layout, .. } if *layout != Layout::One && values.len() > 4));
        if let Some(line) = line {
            if self.fits(&line) && positional <= fixed_front(&lower) + 4 && !long_list {
                self.lines.push(line);
                return;
            }
        }
        if let Some(arg) = front_arg {
            opening.push_str(&arg.text);
        }
        self.push_text(depth, &opening);
        for group in &groups {
            self.group(group, depth + 1);
        }
        self.lines.push(format!("{indent})"));
    }

    /// A command without a signature keeps its arguments as written. Its
    /// continuation lines move as far as its first line does, except lines
    /// inside a multi-line quoted or bracket argument, whose text is the
    /// argument's value.
    fn unknown_command(&mut self, node: Node, name: &str, open: usize, close: usize, depth: usize) {
        let indent = self.indent(depth);
        let inner = &self.src[open..close];
        if !inner.contains('\n') {
            // gersemi keeps a one-line call as written unless it is too long.
            let line = format!("{indent}{name}({inner})");
            if self.fits(&line) {
                self.lines.push(line);
            } else {
                self.lines.push(format!(
                    "{indent}{name}({})",
                    super::unescaped_trim_end(inner)
                ));
            }
            return;
        }
        let inner = super::unescaped_trim_end(inner);
        let first_row = node.start_position().row;
        let mut protected = std::collections::HashSet::new();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if matches!(
                n.kind(),
                "quoted_argument" | "bracket_argument" | "bracket_comment"
            ) {
                protected.extend(n.start_position().row + 1..=n.end_position().row);
            } else {
                stack.extend(children(n));
            }
        }
        let shift = indent.len() as isize - node.start_position().column as isize;
        let mut text = format!("{indent}{name}(");
        for (i, line) in inner.split('\n').enumerate() {
            let line = line.trim_end_matches('\r');
            if i == 0 {
                text.push_str(line);
                continue;
            }
            text.push('\n');
            let spaces = line.len() - line.trim_start_matches(' ').len();
            if protected.contains(&(first_row + i)) || line.trim_ascii().is_empty() {
                text.push_str(line);
            } else {
                let moved = (spaces as isize + shift).max(0) as usize;
                text.push_str(&" ".repeat(moved));
                text.push_str(&line[spaces..]);
            }
        }
        text.push('\n');
        text.push_str(&indent);
        text.push(')');
        self.lines.extend(text.split('\n').map(str::to_string));
    }

    /// A condition, one top-level `AND` / `OR` operand per line. An operand
    /// that does not fit breaks after its first word, or, when it is a
    /// parenthesized group, is laid out the same way inside the group.
    fn condition(&mut self, args: &[Arg], depth: usize) {
        let indent = self.indent(depth);
        for group in condition_groups(args) {
            let words: Option<Vec<&str>> = group.iter().map(|a| a.inline()).collect();
            if let Some(words) = words {
                let line = format!("{indent}{}", words.join(" "));
                // gersemi measures an operand without its leading `AND`,
                // `OR` and `NOT` words.
                let joiner: usize = words
                    .iter()
                    .take_while(|w| matches!(**w, "AND" | "OR" | "NOT"))
                    .map(|w| w.len() + 1)
                    .sum();
                if self.fits(&line[joiner..]) {
                    self.lines.push(line);
                    continue;
                }
            }
            let prefix_len = group
                .iter()
                .take_while(|a| !a.comment && matches!(a.text.as_str(), "AND" | "OR" | "NOT"))
                .count();
            if let [prefix @ .., last] = group.as_slice() {
                if prefix.len() == prefix_len {
                    // gersemi keeps a group of single-word operands, such
                    // as `(A OR B OR C)`, on one line however long.
                    // A group holding a comment is expanded too, so the
                    // comment keeps its place.
                    let compound = last.inner.as_ref().is_some_and(|inner| {
                        inner.iter().any(Arg::breaks)
                            || condition_groups(inner).iter().any(|g| {
                                g.iter()
                                    .filter(|a| !matches!(a.text.as_str(), "AND" | "OR" | "NOT"))
                                    .count()
                                    > 1
                                    || g.iter().any(|a| a.inner.is_some())
                            })
                    });
                    if let Some(inner) = last.inner.as_ref().filter(|_| compound) {
                        // Words up to a trailing comment keep their line,
                        // and the group continues one level deeper.
                        let mut words: Vec<&str> = Vec::new();
                        let mut level = depth;
                        for word in prefix {
                            words.push(&word.text);
                            if let Some(comment) = &word.trailing {
                                self.push_text(level, &format!("{} {comment}", words.join(" ")));
                                words.clear();
                                level = depth + 1;
                            }
                        }
                        let open = if words.is_empty() {
                            "(".to_string()
                        } else {
                            format!("{} (", words.join(" "))
                        };
                        self.push_text(level, &open);
                        self.condition(inner, level + 1);
                        self.push_text(
                            level,
                            &match &last.trailing {
                                Some(comment) => format!(") {comment}"),
                                None => ")".to_string(),
                            },
                        );
                        continue;
                    }
                }
            }
            let simple = group.iter().all(|a| {
                !a.comment && !a.text.contains('\n') && !a.inner.iter().flatten().any(Arg::breaks)
            });
            if simple
                && (group.len() == prefix_len + 1 || group.iter().any(|a| a.trailing.is_some()))
            {
                self.commented_words(&group, depth);
                continue;
            }
            // The operator words and the first operand share a line until
            // a trailing comment or a group holding one; the rest
            // continue one level deeper.
            let head = (prefix_len + 1).min(group.len());
            let mut words: Vec<&str> = Vec::new();
            let mut level = depth;
            for word in &group[..head] {
                if expands(word) {
                    if !words.is_empty() {
                        self.push_text(level, &words.join(" "));
                        words.clear();
                    }
                    self.condition_word(word, level);
                    level = depth + 1;
                    continue;
                }
                words.push(&word.text);
                if let Some(comment) = &word.trailing {
                    self.push_text(level, &format!("{} {comment}", words.join(" ")));
                    words.clear();
                    level = depth + 1;
                }
            }
            if !words.is_empty() {
                self.push_text(level, &words.join(" "));
            }
            for value in &group[head..] {
                self.condition_word(value, depth + 1);
            }
        }
    }

    /// One condition word; a parenthesized group holding a comment is
    /// expanded so the comment keeps its place.
    fn condition_word(&mut self, word: &Arg, depth: usize) {
        match word.inner.as_ref().filter(|_| expands(word)) {
            Some(inner) => {
                self.push_text(depth, "(");
                self.condition(inner, depth + 1);
                self.push_text(
                    depth,
                    &match &word.trailing {
                        Some(comment) => format!(") {comment}"),
                        None => ")".to_string(),
                    },
                );
            }
            None => self.arg(word, depth),
        }
    }

    /// Words on one line until one carries a trailing comment; the words
    /// after it continue one level deeper, as gersemi lays out
    /// `AND # note` before its operand.
    fn commented_words(&mut self, words: &[Arg], depth: usize) {
        let mut line: Vec<&str> = Vec::new();
        let mut level = depth;
        for word in words {
            line.push(&word.text);
            if let Some(comment) = &word.trailing {
                self.push_text(level, &format!("{} {comment}", line.join(" ")));
                line.clear();
                level = depth + 1;
            }
        }
        if !line.is_empty() {
            self.push_text(level, &line.join(" "));
        }
    }

    /// Fills lines with as many of `values` as fit.
    fn pack(&mut self, values: &[Arg], depth: usize) {
        let indent = self.indent(depth);
        let mut line = String::new();
        for value in values {
            match value.inline() {
                Some(text) => {
                    let candidate = format!("{line} {text}");
                    if line.is_empty() {
                        line = format!("{indent}{text}");
                    } else if self.fits(&candidate) {
                        line = candidate;
                    } else {
                        self.lines
                            .push(std::mem::replace(&mut line, format!("{indent}{text}")));
                    }
                }
                None => {
                    if !line.is_empty() {
                        self.lines.push(std::mem::take(&mut line));
                    }
                    self.arg(value, depth);
                }
            }
        }
        if !line.is_empty() {
            self.lines.push(line);
        }
    }

    fn arg(&mut self, arg: &Arg, depth: usize) {
        if let Some(inner) = arg.inner.as_ref().filter(|_| expands(arg)) {
            // A parenthesized group holding a comment: one argument per
            // line, so the comment keeps its place.
            self.push_text(depth, "(");
            for value in inner {
                self.arg(value, depth + 1);
            }
            self.push_text(
                depth,
                &match &arg.trailing {
                    Some(comment) => format!(") {comment}"),
                    None => ")".to_string(),
                },
            );
            return;
        }
        self.push_text(depth, &arg.text);
        if let (Some(comment), Some(line)) = (&arg.trailing, self.lines.last_mut()) {
            line.push(' ');
            line.push_str(comment);
        }
    }

    fn group(&mut self, group: &Group, depth: usize) {
        let indent = self.indent(depth);
        if let Some(inline) = group_inline(group) {
            let line = format!("{indent}{inline}");
            if self.fits(&line) || matches!(group, Group::Item(_)) {
                self.lines.push(line);
                return;
            }
        }
        match group {
            Group::Item(arg) => self.arg(arg, depth),
            Group::Keyword {
                keyword,
                values,
                layout,
            } => {
                if *layout == Layout::One
                    && values.len() == 1
                    && keyword.inline().is_some()
                    && !expands(&values[0])
                {
                    let value = &values[0];
                    let first = value.text.split('\n').next().unwrap_or("");
                    let rest = &value.text[first.len()..];
                    self.push_text(depth, &format!("{} {first}{rest}", keyword.text));
                    if let (Some(c), Some(line)) = (&value.trailing, self.lines.last_mut()) {
                        line.push(' ');
                        line.push_str(c);
                    }
                    return;
                }
                self.arg(keyword, depth);
                if *layout == Layout::Headed && !values.is_empty() {
                    let head = Group::Keyword {
                        keyword: values[0].clone(),
                        values: values[1..].to_vec(),
                        layout: Layout::List,
                    };
                    self.group(&head, depth + 1);
                } else if *layout == Layout::Pack {
                    self.pack(values, depth + 1);
                } else if *layout == Layout::Pairs {
                    let mut iter = values.iter().peekable();
                    while let Some(name) = iter.next() {
                        let value = iter.next_if(|v| !v.comment && name.inline().is_some());
                        match value.and_then(|v| v.inline().map(|t| (v, t))) {
                            Some((_, text)) => {
                                let line = format!("{} {text}", name.text);
                                self.push_text(depth + 1, &line);
                            }
                            None => {
                                self.arg(name, depth + 1);
                                if let Some(v) = value {
                                    self.arg(v, depth + 1);
                                }
                            }
                        }
                    }
                } else {
                    for value in values {
                        self.arg(value, depth + 1);
                    }
                }
            }
            Group::Section { keyword, groups } => {
                self.arg(keyword, depth);
                for group in groups {
                    self.group(group, depth + 1);
                }
            }
        }
    }
}

/// Whether `arg` is a parenthesized group with a comment or a multi-line
/// value inside.
fn expands(arg: &Arg) -> bool {
    arg.inner
        .as_ref()
        .is_some_and(|i| i.iter().any(Arg::breaks))
}

/// A condition split before each top-level `AND` / `OR`; an own-line
/// comment is a group of its own.
fn condition_groups(args: &[Arg]) -> Vec<Vec<Arg>> {
    let mut groups: Vec<Vec<Arg>> = Vec::new();
    for arg in args {
        let starts = arg.comment
            || groups.is_empty()
            || matches!(arg.text.as_str(), "AND" | "OR")
            || groups.last().is_some_and(|g| g.iter().any(|a| a.comment));
        if starts {
            groups.push(vec![arg.clone()]);
        } else if let Some(group) = groups.last_mut() {
            group.push(arg.clone());
        }
    }
    groups
}
