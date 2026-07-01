// Python formatter: currently a stub, following the same bootstrapping
// pattern c_cpp.rs started from — parse with tree-sitter, pass source
// through unchanged. Corpus tests (tests/python_corpus/) already diff
// against `ruff format` output, so as constructs get real handling here the
// tests drive green one at a time instead of needing to be written after
// the fact.
//
// Two style targets, unlike c_cpp's single funky parity target:
//   - default Config: PEP8 (flake8-clean — 79-column lines, standard blank
//     line rules)
//   - `--preset black` (presets/python/black.toml): matches `ruff format`
//     (Black-compatible: 88 columns, double-quote-preferring, trailing
//     commas in exploded literals)
//
// `ruff format` is the structural reference (there is no PEP8-only
// formatter to diff against — PEP8 is a style guide, not a tool). `flake8`
// is a lint gate: formatted output should trigger zero pycodestyle/pyflakes
// warnings when checked against the PEP8-target config.

use crate::config::Config;
use crate::error::MoldyError;

pub fn format(source: &str, config: &Config) -> Result<String, MoldyError> {
    let ts_lang = lang_parsing_substrate::language_for_key("python")
        .ok_or_else(|| MoldyError::UnsupportedLanguage("python".to_string()))?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&ts_lang)
        .map_err(|e| MoldyError::Parse(e.to_string()))?;

    let _tree = parser
        .parse(source, None)
        .ok_or_else(|| MoldyError::Parse("tree-sitter returned no tree".into()))?;

    // `config.python.max_width` (79 for PEP8, 88 via `--preset black`) will
    // drive line-wrapping once this stub grows real emission logic.
    let _ = config.python.max_width;
    Ok(source.to_string())
}
