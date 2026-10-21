# Troubleshooting

**A header was skipped.** Recursive selection uses `is_discoverable_file`, which excludes `.h`. Pass the header explicitly. `.h` selects C; C++ headers need a C++ extension such as `.hpp`.

**A directory produced no output or passed `--check`.** Supply `--recursive`. A successful empty selection does not verify any files; check extensions and ignore patterns.

**A config change had no effect.** Automatic loading checks only `moldy.toml` in the current directory. Use `--config path/to/file.toml` for another location or for `funky.toml`. A preset bypasses local config loading. Language-specific policies may not apply to the input language.

**TOML contains an unknown field.** Config sections reject unknown keys. In particular, put `nl_brace_else` under `[newlines]`. Compare with the complete [default configuration](configuration.md).

**Files are unexpectedly ignored.** Inspect both Moldy's `[ignore].patterns` and the nearest `toolchain.toml` found from the current directory. Patterns match the candidate path and bare filename. Relative patterns are not rebased to the shared config's directory.

**Piping to `moldy -` fails.** Stdin is not implemented. Save source to a file with a supported extension and pass its path.

**Formatting differs from rustfmt or Black.** The presets implement selected compatibility policies. Read the [known limits](usage.md); Python quote normalization and magic trailing commas, and rustfmt packed multiline lists, are examples of differences.

**A warning says a file was left unchanged.** The JSON, YAML, Markdown, CMake and Bash formatters do not format a file with a syntax error. Use `--dump-tree` and look for `ERROR` or `MISSING` nodes. The JSON grammar rejects a trailing comma, and also a `+` in an exponent such as `1e+10`, which is valid JSON. A Markdown file is also left unchanged when Moldy cannot confirm its output renders the same; the [usage guide](usage.md#json-yaml-markdown-cmake-and-bash) lists the cases. The warning does not fail `--check`.

**A shell script without an extension was not formatted.** Language selection uses extensions and file names, not shebang lines. Rename the script to `.sh` or `.bash`.

**A recursive run now changes Markdown, YAML or JSON files.** Recursive discovery selects every formatted language. Add `[ignore].patterns` entries, such as `"docs/**"` or `"*.json"`, for files Moldy should leave alone.

**Malformed source changed despite a successful exit.** The C, C++, Rust and Python formatters continue over syntax-error trees. Review the diff and use `--dump-tree` to inspect parser recovery. Formatting success does not establish valid syntax.

**An in-place run stopped halfway through.** A fatal error terminates processing without rolling back earlier writes. Review the diff and fix the failing file/config before rerunning.
