# Troubleshooting

**A header was skipped.** Recursive selection uses `is_discoverable_file`, which excludes `.h`. Pass the header explicitly. A `.h` with C++-only syntax is formatted as C++, and any other `.h` as C.

**A directory produced no output or passed `--check`.** Supply `--recursive`. A successful empty selection does not verify any files; check extensions and ignore patterns.

**A config change had no effect.** Automatic loading checks only `moldy.toml` in the current directory. Use `--config path/to/file.toml` for another location or for `funky.toml`. A preset bypasses local config loading. Language-specific policies may not apply to the input language.

**TOML contains an unknown field.** Config sections reject unknown keys. In particular, put `nl_brace_else` under `[newlines]`. Compare with the complete [default configuration](configuration.md).

**Files are unexpectedly ignored.** Inspect both Moldy's `[ignore].patterns` and the nearest `toolchain.toml` found from the current directory. Patterns match the candidate path and bare filename. Relative patterns are not rebased to the shared config's directory.

**Piping to `moldy -` fails with exit 2.** Stdin has no extension to choose the language from. Name the file the source stands for: `moldy - --stdin-filename main.c`.

**Formatting differs from rustfmt or Black.** The presets implement selected compatibility policies. Read the [known limits](usage.md); Python quote normalization and magic trailing commas, and rustfmt packed multiline lists, are examples of differences.

**A file is reported as not formatted, and the run exits 3.** The JSON, YAML, Markdown, CMake and Bash formatters do not format a file with a syntax error. The C, C++, Rust and Python formatters decline a file when formatting it would change its code (`could not be formatted without changing its meaning`); the [usage guide](usage.md#c-c-rust-and-python-code-is-never-changed) lists what they compare. Use `--dump-tree` and look for `ERROR` or `MISSING` nodes. The JSON grammar rejects a trailing comma, and also a `+` in an exponent such as `1e+10`, which is valid JSON. A Markdown file is also left unchanged when Moldy cannot confirm its output renders the same; the [usage guide](usage.md#json-yaml-markdown-cmake-and-bash) lists the cases. Such a file makes the run incomplete, so `--check` and `--in-place` exit 3 until it is fixed or ignored with an `[ignore]` pattern. A file that cannot be read, is not UTF-8 or is in an unsupported language is reported the same way.

**A shell script without an extension was not formatted.** Language selection uses extensions and file names, not shebang lines. Rename the script to `.sh` or `.bash`.

**A recursive run now changes Markdown, YAML or JSON files.** Recursive discovery selects every formatted language. Add `[ignore].patterns` entries, such as `"docs/**"` or `"*.json"`, for files Moldy should leave alone.

**Part of a file was not laid out.** The C, C++, Rust and Python formatters keep a statement that holds a syntax error as written, and format the rest of the file around it. C and C++ code inside a preprocessor conditional (`#if`, `#ifdef`, `#ifndef`, including an include guard) is always kept as written, apart from trailing-comment alignment. Use `--dump-tree` and look for `ERROR` or `MISSING` nodes. Formatting success does not establish valid syntax.

**An in-place run stopped halfway through.** A fatal error terminates processing without rolling back earlier writes. Review the diff and fix the failing file/config before rerunning.
