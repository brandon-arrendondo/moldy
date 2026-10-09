# Architecture and substrate use

Moldy owns formatting policy. `lang-parsing-substrate` owns the language registry, tree-sitter grammar access, extension predicates, and the `PathIgnore` glob matcher used for file selection.

The dependency is version 0.11.2, matching the Knots refresh. `default-features = false` keeps the compiled registry limited to `lang-c`, `lang-cpp`, `lang-rust`, and `lang-python`. Substrate defaults otherwise enable languages Moldy cannot format, causing recursive discovery to select unsupported files. Adding a grammar feature must accompany a formatter and dispatch arm.

## Processing pipeline

1. The CLI loads a preset or Moldy config and appends shared ignore paths.
2. Directory walks select extensions with `is_source_extension`. Explicit paths bypass that predicate. Both routes apply `PathIgnore`.
3. Files are read as UTF-8. `format_source` obtains `language_info_for_file` and dispatches on its canonical key.
4. Each formatter obtains its grammar through `language_for_key` and creates a tree-sitter parser. The debug `dump_tree` route uses `language_for_file`.
5. Formatters emit text under `Config`. The CLI prints, compares, or writes it.

Moldy currently uses extension-based `.h` detection and preserves its explicit-only discovery policy. Knots uses `is_parseable_extension` to include headers in recursive analysis and uses content sniffing for C++ `.h` files; these are separate consumer decisions.

## Formatter responsibilities

`src/formatter/c_cpp.rs` implements recursive structural emission with expression leaf walking and output alignment passes. It targets funky's C/C++ policy. `rust.rs` and `python.rs` use structural handlers with generic child emission and pairwise token-spacing rules. `output.rs` provides shared output operations.

Tree-sitter nodes retain byte offsets into the original source. Text is sliced using those offsets; whitespace is reconstructed by the formatter. Unknown constructs may emit source text verbatim. Formatting does not reject all tree-sitter syntax errors.

Substrate dead-code blanking, call extraction, metrics, control-flow graphs, and fingerprints are analysis facilities and are not part of Moldy's formatting path. Blanking source before emission would discard text a formatter must preserve, including preprocessor-controlled code. Parser trees and source slices must refer to the same text.

`src/toolchain.rs` models the shared ignore subset of `toolchain.toml` and discovers it from the working directory. Substrate 0.11.2 exports the glob matcher but does not export a shared configuration loader, so discovery and TOML deserialization remain in Moldy. This module does not locate or invoke external formatters.

## Public library

The package exposes the `moldy` library with `config`, `error`, `formatter`, `presets`, and `toolchain` modules. `formatter::format_source(path, source, &config)` formats supplied text without reading the file. It does not load CLI configuration, merge shared ignores, or write files. The `Formatter` trait is public; built-in dispatch currently calls module functions.

See [development](development.md) for migration checks and adding a language.
