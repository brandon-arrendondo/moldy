# Troubleshooting

**A header was skipped.** Recursive selection uses `is_source_extension`, which excludes `.h`. Pass the header explicitly. `.h` selects C; C++ headers need a C++ extension such as `.hpp`.

**A directory produced no output or passed `--check`.** Supply `--recursive`. A successful empty selection does not verify any files; check extensions and ignore patterns.

**A config change had no effect.** Automatic loading checks only `moldy.toml` in the current directory. Use `--config path/to/file.toml` for another location or for `funky.toml`. A preset bypasses local config loading. Language-specific policies may not apply to the input language.

**TOML contains an unknown field.** Config sections reject unknown keys. In particular, put `nl_brace_else` under `[newlines]`. Compare with the complete [default configuration](configuration.md).

**Files are unexpectedly ignored.** Inspect both Moldy's `[ignore].patterns` and the nearest `toolchain.toml` found from the current directory. Patterns match the candidate path and bare filename. Relative patterns are not rebased to the shared config's directory.

**Piping to `moldy -` fails.** Stdin is not implemented. Save source to a file with a supported extension and pass its path.

**Formatting differs from rustfmt or Black.** The presets implement selected compatibility policies. Read the [known limits](usage.md); Python quote normalization and magic trailing commas, and rustfmt packed multiline lists, are examples of differences.

**Malformed source changed despite a successful exit.** Formatters continue over syntax-error trees. Review the diff and use `--dump-tree` to inspect parser recovery. Formatting success does not establish valid syntax.

**An in-place run stopped halfway through.** A fatal error terminates processing without rolling back earlier writes. Review the diff and fix the failing file/config before rerunning.
