// JSON formatter (`.json`, `.jsonc`): every non-empty object and array is
// expanded one member per line, the layout of `jq .` and Python's
// `json.dumps(indent=2)`. Empty containers stay `{}` / `[]`. Keys, strings
// and numbers are emitted verbatim from source; whitespace is the only
// thing this formatter changes.
//
// `.jsonc` comments are kept. A comment that starts on the same line as the
// token before it stays on that line, after the comma; any other comment
// gets its own line ahead of the member that follows it. One blank line
// between members is preserved.
//
// Known gaps (documented rather than half-implemented):
//   - A file with any syntax error (including a trailing comma, which the
//     grammar rejects) is returned unchanged rather than partially
//     reformatted.
//   - A member with a comment inside it (between key and value) is emitted
//     verbatim from source.

use super::output::OutputOps;
use crate::config::Config;
use crate::error::MoldyError;
use tree_sitter::Node;

pub fn format(source: &str, config: &Config) -> Result<String, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key("json")
        .ok_or_else(|| MoldyError::UnsupportedLanguage("json".to_string()))?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))?;

    let root = tree.root_node();
    if root.has_error() {
        return Err(super::syntax_error("JSON", root));
    }
    super::check_depth("JSON", root)?;

    let mut fmt = Fmt {
        src: source,
        config,
        out: String::new(),
        depth: 0,
    };
    fmt.emit_document(root);
    Ok(fmt.finish())
}

/// One value of a container (or of the document), with the comments that
/// belong to it.
struct Entry<'t> {
    value: Node<'t>,
    leading: Vec<Node<'t>>,
    trailing: Vec<Node<'t>>,
    blank_before: bool,
}

/// Groups `children` (values, commas and comments, brackets excluded) into
/// entries. Returns comments that trail the opening bracket, the entries,
/// and comments left after the last value (with whether a blank line
/// precedes them).
#[allow(clippy::type_complexity)]
fn group<'t>(
    children: &[Node<'t>],
    open_row: Option<usize>,
) -> (Vec<Node<'t>>, Vec<Entry<'t>>, (Vec<Node<'t>>, bool)) {
    let mut open_trailing = Vec::new();
    let mut entries: Vec<Entry<'t>> = Vec::new();
    let mut pending: Vec<Node<'t>> = Vec::new();
    let mut pending_blank = false;
    // Row of the last token seen, for trailing-comment and blank-line checks.
    let mut last_row = open_row;

    for &child in children {
        let start = child.start_position().row;
        let blank = last_row.is_some_and(|r| start > r + 1);
        match child.kind() {
            "," => {}
            "comment" => {
                if pending.is_empty() && last_row == Some(start) {
                    match entries.last_mut() {
                        Some(e) => e.trailing.push(child),
                        None => open_trailing.push(child),
                    }
                } else {
                    if pending.is_empty() {
                        pending_blank = blank;
                    }
                    pending.push(child);
                }
            }
            _ => {
                let blank_before = if pending.is_empty() {
                    blank
                } else {
                    pending_blank
                };
                entries.push(Entry {
                    value: child,
                    leading: std::mem::take(&mut pending),
                    trailing: Vec::new(),
                    blank_before: blank_before && !entries.is_empty(),
                });
                pending_blank = false;
            }
        }
        last_row = Some(child.end_position().row);
    }
    (open_trailing, entries, (pending, pending_blank))
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

    fn indent_str_at(&self, d: u32) -> String {
        " ".repeat(self.config.json.indent_width as usize * d as usize)
    }
}

impl<'a> Fmt<'a> {
    fn children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
        let mut cursor = node.walk();
        node.children(&mut cursor).collect()
    }

    fn comment(&mut self, node: Node) {
        let text = self.node_text(node).trim_ascii_end().to_string();
        self.raw(&text);
    }

    fn trailing_comments(&mut self, comments: &[Node]) {
        for &c in comments {
            self.raw(" ");
            self.comment(c);
        }
    }

    fn own_line_comments(&mut self, comments: &[Node]) {
        for &c in comments {
            self.ensure_nl();
            self.emit_indent();
            self.comment(c);
        }
    }

    fn emit_document(&mut self, root: Node) {
        let children = Self::children(root);
        let (_, entries, (dangling, dangling_blank)) = group(&children, None);
        for entry in &entries {
            self.ensure_nl();
            if entry.blank_before {
                self.nl();
            }
            self.own_line_comments(&entry.leading);
            self.ensure_nl();
            self.emit_value(entry.value);
            self.trailing_comments(&entry.trailing);
        }
        self.ensure_nl();
        if dangling_blank && !entries.is_empty() {
            self.nl();
        }
        self.own_line_comments(&dangling);
    }

    fn emit_value(&mut self, node: Node) {
        match node.kind() {
            "object" | "array" => self.emit_container(node),
            "pair" => self.emit_pair(node),
            _ => {
                let text = self.node_text(node);
                self.raw(text);
            }
        }
    }

    fn emit_pair(&mut self, node: Node) {
        let children = Self::children(node);
        if children.iter().any(|c| c.kind() == "comment") {
            let text = self.node_text(node);
            self.raw(text);
            return;
        }
        let (Some(key), Some(value)) = (
            node.child_by_field_name("key"),
            node.child_by_field_name("value"),
        ) else {
            let text = self.node_text(node);
            self.raw(text);
            return;
        };
        let key = self.node_text(key);
        self.raw(key);
        self.raw(": ");
        self.emit_value(value);
    }

    fn emit_container(&mut self, node: Node) {
        let children = Self::children(node);
        let (Some(open), Some(close)) = (children.first(), children.last()) else {
            return;
        };
        let open_text = self.node_text(*open);
        let close_text = self.node_text(*close);
        let inner = &children[1..children.len() - 1];
        if inner.is_empty() {
            self.raw(open_text);
            self.raw(close_text);
            return;
        }

        let (open_trailing, entries, (dangling, dangling_blank)) =
            group(inner, Some(open.end_position().row));
        self.raw(open_text);
        self.trailing_comments(&open_trailing);
        self.depth += 1;
        let last = entries.len().saturating_sub(1);
        for (i, entry) in entries.iter().enumerate() {
            self.nl();
            if entry.blank_before {
                self.nl();
            }
            self.own_line_comments(&entry.leading);
            self.ensure_nl();
            self.emit_indent();
            self.emit_value(entry.value);
            if i < last {
                self.raw(",");
            }
            self.trailing_comments(&entry.trailing);
        }
        if !dangling.is_empty() {
            self.nl();
            if dangling_blank && !entries.is_empty() {
                self.nl();
            }
            self.own_line_comments(&dangling);
        }
        self.depth -= 1;
        self.nl();
        self.emit_indent();
        self.raw(close_text);
    }
}
