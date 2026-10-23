# Architecture and substrate use

Moldy owns formatting policy. `lang-parsing-substrate` owns the language registry, tree-sitter grammar access, extension and file-name predicates, Markdown container prefixes, and the `PathIgnore` glob matcher used for file selection. Every grammar, including the Markdown inline grammar, comes through substrate features; Moldy depends directly only on the `tree-sitter` runtime crate, which Cargo shares with the substrate.

Moldy uses `lang-parsing-substrate` 0.12.0. `default-features = false` keeps the compiled registry limited to `lang-c`, `lang-cpp`, `lang-rust`, `lang-python`, `lang-json`, `lang-yaml`, `lang-markdown`, `lang-cmake`, and `lang-bash`. Substrate defaults otherwise enable languages Moldy cannot format, causing recursive discovery to select unsupported files. Adding a grammar feature must accompany a formatter and dispatch arm.

## Processing pipeline

1. The CLI loads a preset or Moldy config and appends shared ignore paths.
2. Directory walks select files with `is_discoverable_file`, which matches registry extensions and file names such as `CMakeLists.txt`. Explicit paths bypass that predicate. Both routes apply `PathIgnore`.
3. Files are read as UTF-8. `format_source` obtains `language_info_for_file` and dispatches on its canonical key.
4. Each formatter obtains its grammar through `language_for_key` and creates a tree-sitter parser. The debug `dump_tree` route uses `language_for_file`.
5. Formatters emit text under `Config`. The CLI prints, compares, or writes it.

Moldy currently uses extension-based `.h` detection and preserves its explicit-only discovery policy. Knots uses `is_parseable_extension` to include headers in recursive analysis and uses content sniffing for C++ `.h` files; these are separate consumer decisions.

## Formatter responsibilities

`src/formatter/c_cpp.rs` implements recursive structural emission with expression leaf walking and output alignment passes. It targets funky's C/C++ policy. `rust.rs` and `python.rs` use structural handlers with generic child emission and pairwise token-spacing rules. `output.rs` provides shared output operations, and `line_endings.rs` maps source line endings onto the output.

`json.rs`, `yaml.rs`, `markdown.rs`, `cmake.rs` and `bash.rs` each target an external formatter's default output (`jq`/`json.dumps`, Prettier, Prettier with `proseWrap: preserve`, gersemi, and Prettier's shell plugin). Unlike the C/C++, Rust and Python formatters, they return `MoldyError::Syntax` for a file with any syntax error, and `MoldyError::TooDeep` for one nested past their limits (checked on the tree, and for Markdown and YAML on each line's prefix before parsing); the CLI leaves the file unchanged, reports it as declined, and ends the run incomplete (exit 3). `markdown.rs` re-parses inline text with the substrate's `markdown_inline` grammar, pairs emphasis with CommonMark's algorithm in `markdown_emphasis.rs` (the grammar's pairing differs on some input), re-parses its output and compares the document's shape with the input's, and reads both with pulldown-cmark in `markdown_check.rs` as an independent CommonMark check, returning `MoldyError::Unverified` on a mismatch, strips container prefixes with the substrate's `markdown::ContentMap`, and formats front matter and `yaml` fences through `yaml.rs`. `cmake_signatures.rs` holds Moldy's own table of built-in CMake command signatures, written from the CMake documentation rather than copied from gersemi, whose table is MPL-2.0.

Tree-sitter nodes retain byte offsets into the original source. Text is sliced using those offsets; whitespace is reconstructed by the formatter. Unknown constructs may emit source text verbatim. Formatting does not reject all tree-sitter syntax errors.

Substrate dead-code blanking, call extraction, metrics, control-flow graphs, and fingerprints are analysis facilities and are not part of Moldy's formatting path. Blanking source before emission would discard text a formatter must preserve, including preprocessor-controlled code. Parser trees and source slices must refer to the same text.

`src/toolchain.rs` models the shared ignore subset of `toolchain.toml` and discovers it from the working directory. Substrate 0.12.0 exports the glob matcher but does not export a shared configuration loader, so discovery and TOML deserialization remain in Moldy. This module does not locate or invoke external formatters.

## Public library

The package exposes the `moldy` library with `config`, `error`, `formatter`, `presets`, and `toolchain` modules. `formatter::format_source(path, source, &config)` formats supplied text without reading the file. It does not load CLI configuration, merge shared ignores, or write files. The `Formatter` trait is public; built-in dispatch currently calls module functions.

See [development](development.md) for migration checks and adding a language.
