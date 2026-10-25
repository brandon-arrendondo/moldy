# Changelog

What changed for users of Moldy, newest release first. See
[ADR-0002](docs/adr/0002-changelog-is-for-users.md) for what belongs here.

## [Unreleased]

### Added

- `moldy - --stdin-filename NAME` formats source read from stdin and writes
  it to stdout. `NAME` is the file the source stands for: its extension
  chooses the language, and ignore patterns match it. `-` with `--in-place`
  or `--check`, or without `--stdin-filename`, exits 2. Before, `-` was
  taken as a file name and the run exited 3.

### Changed

- A crash inside the formatter now fails only the file it was formatting:
  the file is named as `a.rs: not formatted (internal error: ...)`, the
  other files are still handled, and the run exits 3. Before, the crash
  stopped the whole run with exit 101.
- An unknown `--preset` is reported as `unknown preset 'NAME' (available:
  ...)` instead of as an unsupported language. It still exits 2. Library
  users see it as the new `MoldyError::UnknownPreset`.

## [0.5.1] - 2026-10-25

### Fixed

- Rust: code after a `//` comment inside an expression, such as a method
  chain or an array, stays on its own line instead of being joined onto the
  comment's line, where it was commented out. A comment in a bracketed list
  no longer takes a comma, and a list holding one is laid out one item per
  line.
- Rust: `..=x` and code tree-sitter-rust cannot read, such as the character
  literal `'\u{2_FFFF}'`, are kept as written instead of being spaced apart.
  A list that renders over several lines, such as parameters with
  attributes, is split on the first run, so a second run no longer changes
  it. A closure's parameters split one per line take no trailing comma,
  which a second run read wrongly and broke. A statement whose closure
  parameters end in a comma (`|a, b,| a + b`), which tree-sitter-rust cannot
  read, is kept as written instead of being spread over several lines, and
  so is the code after it that the misreading takes in.
- Python: the last clause of an `if`/`elif`/`else` or a `try` with several
  `except` clauses keeps its indentation instead of moving to column 0,
  which broke the parse. A one-element tuple keeps its comma (`(x,)`), a
  comment in a bracketed list no longer takes a comma or swallows the
  closing bracket, and an inline comment gets two spaces before it.

## [0.5.0] - 2026-10-23

### Added

- Each run reports what it did with every selected file. `--check` names
  each file that would change and `--in-place` each file it rewrote; a file
  that could not be formatted is named with its reason, such as
  `a.yaml: not formatted (YAML syntax error at line 3)`. Under `--check`
  and `--in-place` a summary ends stderr, for example
  `moldy: 12 files: 3 reformatted, 8 unchanged, 1 not formatted`.
- `--completion-report FILE` writes each selected file's outcome (changed,
  unchanged, declined or failed) and the counts to `FILE` as JSON, even
  when the run fails.
- `--allow-empty` lets a run that selects no files succeed.

### Fixed

- Files with upper- or mixed-case extensions, such as `A.C` and `B.H` from
  Windows-built source trees, are formatted and checked: `A.C` is C, and
  `--recursive` finds it, while `B.H`, like any `.h` header, is formatted
  when named. `.C` is C, not C++. `[ignore].patterns` and `toolchain.toml`
  ignore paths stay case-sensitive, so `*.c` does not ignore `A.C`; list
  both cases or use a directory pattern.

### Changed

- Moldy now exits 3 when a run cannot finish: a selected file it cannot
  read (or that is not UTF-8), format, write or reach while walking a
  directory, a named file in an unsupported language, a file it leaves
  unchanged because of a syntax error, a Markdown rewrite it cannot verify
  or nesting too deep, or a run that selects no files. Before, a file left
  unchanged was only a warning and the run could exit 0, an empty
  selection exited 0, and the other failures stopped the run with exit 1.
  A failing file no longer stops the run: the other files are still
  handled, and under `--in-place` the files already rewritten stay
  rewritten. Tree-sitter error recovery in C, C++, Rust and Python is not
  a failure.
- Moldy exits 2 when a run cannot start: a directory named without `-r`
  (before, a warning and exit 0), a named path that does not exist, an
  unknown preset, or a malformed `moldy.toml`, `--config` file or
  `toolchain.toml` (before, exit 1). No file is touched. `--check` with
  `--in-place` is now reported by the argument parser, also with exit 2.
- Exit 1 now means only that `--check` found files that would be
  reformatted.

## [0.4.0] - 2026-10-20

### Added

- JSON (`.json`, `.jsonc`), YAML (`.yaml`, `.yml`), Markdown (`.md`,
  `.markdown`), CMake (`CMakeLists.txt`, `.cmake`) and Bash (`.sh`, `.bash`)
  formatting. JSON expands every object and array like `jq .`; YAML and
  Markdown follow Prettier, with Markdown line breaks kept where they are;
  CMake follows gersemi; Bash follows Prettier's shell plugin. A file in
  one of these languages with a syntax error is left unchanged, with a
  warning on stderr that does not change the exit status. A Markdown file
  is also left unchanged, with a warning, when Moldy cannot confirm the
  formatted document renders the same, and a file in any of these
  languages that nests too deeply is left unchanged the same way. Library
  users see these cases as `MoldyError::Syntax`, `MoldyError::Unverified`
  and `MoldyError::TooDeep`. Warnings name the line of the
  problem, and the pre-commit hook shows them. See the usage
  guide for each language's known differences from its reference.
- `[json]`, `[yaml]`, `[cmake]` and `[bash]` configuration sections for
  indentation width and, for YAML and CMake, line width.

### Changed

- Recursive formatting (`-r`) now also selects files in these languages,
  including every `CMakeLists.txt`. Add ignore patterns for files Moldy
  should leave alone, such as generated JSON or prose you format another
  way.

## [0.3.0] - 2026-10-13

### Added

- `--line-endings lf|crlf` explicitly converts output line endings. Library
  users can select `LineEndings` with `format_source_with_line_endings`.

### Fixed

- Documentation and sample configuration no longer describe `[newlines].style`
  as controlling output endings. This field has never affected formatting;
  use `--line-endings` to request conversion.
- C/C++ files no longer abort with a memory allocation failure when a
  `{ ... }` block is the first statement inside another block, such as a
  function, `if` or loop body. These blocks now format and indent like
  other blocks.

### Changed

- A `{ ... }` block that is the first statement of a `switch` body or
  another block, and ends with a `case` label, now indents both braces
  like the same block elsewhere. Version 0.2.1 printed its opening brace
  at column 0 and its closing brace one level too deep.

- Formatting preserves LF and CRLF endings by default, including mixed
  files. New or joined lines use the predominant source ending.
  Previously, formatting emitted LF and could leave carriage returns in
  comments and strings. Otherwise formatted CRLF files now pass `--check`.

## [0.2.1] - 2026-10-12

### Fixed

- C/C++ multiline initializer lists preserve commas before or after leading
  and trailing comments, including nested struct initializers and whether
  the last element has a comma. Versions 0.1.0 and 0.2.0 inserted extra
  commas around comments and could add a comma to the last element.
- C/C++ preprocessor directives inside initializer lists keep their text,
  each on its own line starting at column 0, with no added commas. Versions
  0.1.0 and 0.2.0 could join directive tokens (`#if X` became `#ifX`) and
  append commas.

Accountable reviewer: Brandon Arrendondo. Moldy is developed with AI
assistance; see the README's AI Assistance section.

## [0.2.0] - 2026-10-12

### Fixed

- Recursive formatting (`-r`) no longer stops with "unsupported language"
  when a directory contains source files in a language Moldy does not format.
  Discovery now selects only C, C++, Rust and Python files and skips the rest.
- The sample `moldy.toml`, `--help` and the documentation no longer describe
  features Moldy does not have: `/* moldy:off */ ... /* moldy:on */`
  format-off regions, and reading source from stdin with `-`. Neither has ever
  worked; comments of that form are formatted like any other comment.

Accountable reviewer: Brandon Arrendondo. Moldy is developed with AI
assistance; see the README's AI Assistance section.

[Unreleased]: https://github.com/brandon-arrendondo/moldy/compare/v0.5.1...HEAD
[0.5.1]: https://github.com/brandon-arrendondo/moldy/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/brandon-arrendondo/moldy/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.1.0...v0.2.0
