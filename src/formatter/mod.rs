mod bash;
mod c_cpp;
mod cmake;
mod cmake_signatures;
mod json;
mod line_endings;
mod markdown;
mod markdown_check;
mod markdown_emphasis;
mod output;

mod python;
mod rust;
mod verify;
mod yaml;

pub use line_endings::LineEndings;

use std::path::Path;

use crate::config::Config;
use crate::error::MoldyError;

/// A per-language formatter implementation.
pub trait Formatter {
    /// Reformat `source` per `config`, returning the formatted text.
    fn format(&self, source: &str, config: &Config) -> Result<String, MoldyError>;
}

/// Dispatch: detect language from path, parse with tree-sitter, route to the
/// appropriate per-language formatter.
pub fn format_source(path: &Path, source: &str, config: &Config) -> Result<String, MoldyError> {
    format_source_with_line_endings(path, source, config, LineEndings::Preserve)
}

/// Format with an explicit ending policy. Parser trees and source slices use
/// the original input; mapped endings are restored after layout and alignment.
pub fn format_source_with_line_endings(
    path: &Path,
    source: &str,
    config: &Config,
    policy: LineEndings,
) -> Result<String, MoldyError> {
    let info = lang_parsing_substrate::language_info_for_file(path)
        .ok_or_else(|| MoldyError::UnsupportedLanguage(path.display().to_string()))?;

    let key = match info.key {
        "c" if header_is_cpp(path, source) => "cpp",
        key => key,
    };

    let formatted = match key {
        "c" | "cpp" => c_cpp::format(source, key, config),
        "rust" => rust::format(source, config),
        "python" => python::format(source, config),
        "json" => json::format(source, config),
        "yaml" => yaml::format(source, config),
        "markdown" => markdown::format(source, config),
        "cmake" => cmake::format(source, config),
        "bash" => bash::format(source, config),
        key => Err(MoldyError::UnsupportedLanguage(key.to_string())),
    }?;
    if let Some((language, edits)) = token_edits(key, config) {
        verify::check_tokens(language, key, source, &formatted, &edits)?;
    }
    Ok(line_endings::restore(source, &formatted, policy))
}

/// For a formatter that continues over syntax errors, its language's
/// name and the token edits its options allow; the output's other tokens
/// must be the input's. The other formatters decline any syntax error.
fn token_edits(key: &str, config: &Config) -> Option<(&'static str, verify::Edits)> {
    let braces = &config.braces;
    let edits = match key {
        "c" | "cpp"
            if braces.add_braces_to_if
                || braces.add_braces_to_while
                || braces.add_braces_to_for =>
        {
            verify::Edits {
                insert: &["{", "}"],
                ..verify::Edits::NONE
            }
        }
        "c" | "cpp" => verify::Edits::NONE,
        "rust" => verify::Edits {
            trailing_commas: true,
            ..verify::Edits::NONE
        },
        // A line continuation may go where the lines are joined.
        "python" => verify::Edits {
            delete: &["line_continuation"],
            trailing_commas: true,
            ..verify::Edits::NONE
        },
        _ => return None,
    };
    let language = match key {
        "c" => "C",
        "cpp" => "C++",
        "rust" => "Rust",
        _ => "Python",
    };
    Some((language, edits))
}

/// Whether `path` is a `.h` header whose content can only be C++, by the
/// substrate's sniffing. Other files keep the language of their extension.
fn header_is_cpp(path: &Path, source: &str) -> bool {
    lang_parsing_substrate::language_for_header_content(path, source.as_bytes())
        == lang_parsing_substrate::language_for_key("cpp")
}

/// `text` without trailing ASCII whitespace, keeping a character escaped
/// by a backslash (`a\ ` ends in an escaped space).
fn unescaped_trim_end(mut text: &str) -> &str {
    while let Some(rest) = text.strip_suffix([' ', '\t', '\r', '\n']) {
        if (rest.len() - rest.trim_end_matches('\\').len()) % 2 == 1 {
            break;
        }
        text = rest;
    }
    text
}

/// The deepest syntax tree the JSON, YAML, Markdown, CMake and Bash
/// formatters accept. Their emitters recurse at least once per level.
const MAX_TREE_DEPTH: usize = 400;

/// `MoldyError::TooDeep` if `root` nests more than `MAX_TREE_DEPTH` levels.
fn check_depth(language: &str, root: tree_sitter::Node) -> Result<(), MoldyError> {
    let mut cursor = root.walk();
    let mut depth = 0;
    loop {
        if depth > MAX_TREE_DEPTH {
            return Err(too_deep(language, cursor.node().start_position().row));
        }
        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return Ok(());
            }
            depth -= 1;
        }
    }
}

/// The deepest block nesting the Markdown and YAML formatters accept, and
/// the deepest Markdown emphasis formatted, checked before parsing.
const MAX_BLOCK_DEPTH: usize = 240;

/// `MoldyError::TooDeep` for the first line of `source` on which
/// `estimate`, an upper bound on the blocks open there, passes
/// `MAX_BLOCK_DEPTH`. Checked before parsing.
fn check_block_depth(
    language: &str,
    source: &str,
    estimate: impl Fn(&str) -> usize,
) -> Result<(), MoldyError> {
    match source
        .lines()
        .position(|line| estimate(line) > MAX_BLOCK_DEPTH)
    {
        Some(row) => Err(too_deep(language, row)),
        None => Ok(()),
    }
}

fn too_deep(language: &str, row: usize) -> MoldyError {
    MoldyError::TooDeep {
        language: language.to_string(),
        line: row + 1,
    }
}

/// The error for a tree with a syntax error, naming the line of the first
/// `ERROR` or `MISSING` node.
fn syntax_error(language: &str, root: tree_sitter::Node) -> MoldyError {
    let mut stack = vec![root];
    let mut first: Option<usize> = None;
    while let Some(node) = stack.pop() {
        if !node.has_error() {
            continue;
        }
        if node.is_error() || node.is_missing() {
            let row = node.start_position().row;
            first = Some(first.map_or(row, |f| f.min(row)));
            continue;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    MoldyError::Syntax {
        language: language.to_string(),
        line: first.map_or(1, |row| row + 1),
    }
}

/// Print the tree-sitter CST to stdout (debug aid, analogous to funky's
/// --dump-tokens).
pub fn dump_tree(path: &Path, source: &str) -> Result<(), MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_header_content(path, source.as_bytes())
        .ok_or_else(|| MoldyError::UnsupportedLanguage(path.display().to_string()))?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))?;

    print_node(tree.root_node(), source, 0);
    Ok(())
}

fn print_node(node: tree_sitter::Node<'_>, source: &str, depth: usize) {
    let indent = "  ".repeat(depth);
    let named = if node.is_named() { "" } else { "*" };
    let leaf = if node.child_count() == 0 {
        format!(" {:?}", &source[node.start_byte()..node.end_byte()])
    } else {
        String::new()
    };
    println!(
        "{}{}{}  [{}-{}]{}",
        indent,
        named,
        node.kind(),
        node.start_byte(),
        node.end_byte(),
        leaf
    );
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        print_node(child, source, depth + 1);
    }
}
