mod c_cpp;

use std::path::Path;

use crate::config::Config;
use crate::error::MoldyError;

pub trait Formatter {
    fn format(&self, source: &str, config: &Config) -> Result<String, MoldyError>;
}

/// Dispatch: detect language from path, parse with tree-sitter, route to the
/// appropriate per-language formatter.
pub fn format_source(path: &Path, source: &str, config: &Config) -> Result<String, MoldyError> {
    let info = lang_parsing_substrate::language_info_for_file(path)
        .ok_or_else(|| MoldyError::UnsupportedLanguage(path.display().to_string()))?;

    match info.key {
        "c" | "cpp" => c_cpp::format(source, info.key, config),
        key => Err(MoldyError::UnsupportedLanguage(key.to_string())),
    }
}

/// Print the tree-sitter CST to stdout (debug aid, analogous to funky's
/// --dump-tokens).
pub fn dump_tree(path: &Path, source: &str) -> Result<(), MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_file(path)
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
