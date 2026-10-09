# CLI usage and language support

```text
moldy [OPTIONS] <FILES>...
```

| Option | Behavior |
| --- | --- |
| `-i`, `--in-place` | Write changed files back to their paths. |
| `--check` | Write no formatted output; report changed filenames on stderr and exit 1 if any would change. |
| `-r`, `--recursive` | Walk directories, following symbolic links. |
| `-c`, `--config <FILE>` | Load this TOML file instead of checking for `moldy.toml` in the current directory. |
| `--preset <NAME>` | Use a bundled preset instead of a config file; conflicts with `--config`. |
| `--dump-tree` | Print each file's tree-sitter CST; this debug option is hidden from help. |
| `-h`, `--help` | Print help. |
| `-V`, `--version` | Print version. |

`--check` and `--in-place` cannot be combined. Without either option, formatted files are concatenated on stdout without filename separators. Use one input per command when redirecting stdout to a file.

## Language detection

| Language | Recursive and explicit extensions | Explicit only |
| --- | --- | --- |
| C | `.c` | `.h` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx` | — |
| Rust | `.rs` | — |
| Python | `.py` | — |

Extensions come from the compiled substrate registry. A `.h` path selects C; Moldy does not use the substrate's content-based C++ header detection. Use a C++ extension for C++ headers. Enabling another grammar alone does not implement formatting for that language.

Directories require `--recursive`; otherwise Moldy warns and skips them. Recursive discovery uses `is_source_extension`, so `.h` is skipped. Explicit file paths are attempted even if their extension is unsupported and then fail at dispatch. Ignore patterns apply to both explicit paths and recursive files; see [configuration](configuration.md).

## Output and exits

Inputs must be UTF-8 files. Stdin is not implemented; `-` is currently treated as a filesystem path. File errors, unsupported languages, malformed configuration, and incompatible flags terminate processing. An explicitly supplied unsupported file reports its path, for example `unsupported language for: a.js`. Files already written with `--in-place` are not rolled back if a later file fails.

Successful execution returns 0, including a scan where no files were selected. `--check` returns 1 if any file would change. Runtime errors return 1; clap argument errors return 2. A check that selects no files does not establish that the project is formatted.

`--dump-tree` takes precedence over formatting inside the file loop, including when paired with `--check` or `--in-place`.

## Formatting limits

C/C++ targets funky's rules, verified against checked-in fixtures. Formatters continue over trees with syntax errors; C/C++ unknown nodes fall back to their source text. A successful formatting call is not a syntax-validation result or a guarantee that invalid input is unchanged. Review diffs before accepting in-place changes to malformed source.

Rust's `rustfmt-compat` preset enables width-based list wrapping and short field-list collapsing, but it is not complete rustfmt parity. Attributes and macro invocations remain opaque. Rustfmt's packed multiline list layout is not implemented.

Python defaults to width 79; the `black` preset selects 88. Strings and f-strings remain opaque, with no quote normalization. Comprehensions, subscripts/slices, and multi-target assignments lack width-based wrapping. Magic trailing commas do not force multiline output. The width is a wrapping budget for supported constructs, not a hard maximum line length. The presets do not guarantee complete Black/Ruff or PEP8 conformance for arbitrary source.
