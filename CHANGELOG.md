# Changelog

What changed for users of Moldy, newest release first. See
[ADR-0002](docs/adr/0002-changelog-is-for-users.md) for what belongs here.

## [Unreleased]

### Fixed

- C, C++, Rust and Python: a file nested more than 2,000 levels deep in
  its syntax tree (thousands of nested blocks or parentheses, or a very
  long `else if` chain) is declined as nested too deeply, as JSON, YAML
  and the other languages already were. Before, Moldy ran out of stack
  and aborted the whole run, from about 800 levels on a 1 MB stack (the
  default for a Windows program's main thread) to a few thousand on
  Linux. A file more than 250 levels deep is now formatted on a
  thread with a stack of its own, so the limit holds on every platform
  and on any library caller's thread.

## [0.7.1] - 2026-10-27

### Fixed

- Comments stay by the code they describe. Before, Moldy kept a
  comment's text but could move it so that it described other code:
  - C and C++: a comment after an enumerator moved onto the next line,
    where it read as the next enumerator's (`MODE_A = 0, /* first mode
    */` became `MODE_A = 0,` and a line `/* first mode */`); a comment
    after `#endif` moved below it; a comment after `if (...)`, after a
    statement in a body given braces, or after a `{` moved onto a line of
    its own; and an expression written over several lines was joined
    onto one, each comment running into the next operand (`x = a || /*
    why a */b || /* why b */c;`).
  - Python: a comment on the line after `if ...:`, `def ...:` or another
    clause moved up onto that line, and one after an opening bracket
    (`f(  # type: ignore`) moved down to the next.
  - Rust: a comment after a field, variant, attribute, match arm or
    statement moved onto the next line.
  - A blank line Moldy adds before a statement went between the
    statement and the comments that lead it: the one after a C block of
    declarations, and the ones around a Python `def` or `class`. The
    comment then read as a note on the code above. The blank line now
    goes above the comments.
  - A blank line above a comment on a line of its own was removed
    inside lists: C initializers and enums, Rust arrays and struct
    literals, and Python brackets. It is now kept, as funky keeps it in
    C; Ruff and Black remove it in Python.
  - Rust: a closing bracket was joined onto the line of a block comment
    before it (`g(a /* why a */` and `);` became one line). Such a list
    is now laid out one item per line, which also formats generic
    parameters, turbofish arguments and closure parameters that end
    with a comment.

  A comment on the line after a top-level `#define` or `#pragma`, which
  0.7.0 declined, is now formatted. The check every run makes now also
  compares where each comment is among the code, and the blank lines
  around it, so output that would
  move a comment is declined (exit 3) instead of written. Some files
  0.7.0 formatted by moving a comment may now be declined, where Moldy
  cannot yet lay out the code around the comment without moving it:
  - Python: a comment inside an expression in parentheses or square
    brackets that is not a list of items, such as a condition split
    over lines, a subscript, a comprehension, or strings joined by
    being written next to each other; and a comment on a line of its
    own in a parenthesized `from ... import (...)` list.
  - C and C++: a comment inside a declaration, between its `=` and its
    value or before its `;`; a comment on a line of its own between
    `else` and the `if` or `{` after it, or between a function's
    declarator and its `{`; a comment inside strings joined by being
    written next to each other; and a comment in a `case` body that
    also holds a preprocessor conditional.
  - Rust: a comment on a line of its own between an `if` condition or
    `else` and its `{`; and a block comment that ends its line before a
    closing bracket that does not close a list, such as that of a
    parenthesized expression, type or pattern, an `if` or `while`
    condition, an index or an array length, where no comma can follow
    it and the bracket would be joined onto its line.
  - Rust: a block comment that starts its line with code after it on
    that line (`/* c */ g();`), in a statement, item, field, variant,
    match arm or list element, where the code would be moved below it.

### Changed

- C and C++: an expression keeps the line breaks it is written with, as
  `if` conditions and calls already did, and so does a `for` header,
  whether or not a comment sits between the lines. A continuation line
  keeps its indentation past the statement's first line; one that starts
  with an operator is indented at least one level. Before, Moldy joined
  such an expression onto one line (`x = a +` and `b;` became `x = a +
  b;`, and a return type on its own line, `char *` and `name(void)`,
  became `char *name(void)`). funky keeps them too.

## [0.7.0] - 2026-10-27

### Fixed

- The C, C++, Rust and Python formatters no longer change code. A
  statement the parser cannot read is kept as written, and the rest of
  the file is formatted around it (code inside a preprocessor
  conditional, such as an include guard, is kept as written in any
  case). Before, Moldy added the token the parser assumed was missing
  (`return 1` gained a `;`, Python's `def f(:` a `)`), split a
  declaration after an unexpanded macro (`EXPORT struct widget
  *make(void);` became `EXPORT struct;` and `widget *make(void);`), or
  joined tokens (`#define REG 0` became `#defineREG0`). Every run now also parses the output again and compares
  its tokens, comments and line structure with the input's. A file whose
  code would change is left unchanged and reported, for example
  `a.c: not formatted (C could not be formatted without changing its
  meaning (line 12))`, and the run exits 3. See the usage guide for what
  is compared and the edits formatting may make. The check makes these
  formatters about three times slower, and they use about three and a
  half times the memory. Some files 0.6.0
  formatted are now declined because their output changed code, so
  `--check` reports them: a function that declares a `struct` (0.6.0 gave
  it a second `;`), a comment on the line after a top-level `#define`
  or `#pragma` (0.6.0 moved it onto the directive line), and a Python
  encoding line or shebang after blank lines at the top of the file
  (0.6.0 removed the blank lines, which put the line where Python reads
  it).
- C and C++: a comment between `if (...)`, `else`, `for (...)`, `while
  (...)` or `do` and its body no longer deletes the body:
  `if (a) /* why */ return 0;` lost its `return 0;`. The comment now
  opens the body.
- C and C++: text inside a block comment is no longer aligned as if it
  were code. A `//` in a URL was taken for a trailing comment (`http://`
  became `http:   //`), and the `=` of `name = value` lines in a comment
  was aligned as in an enum.
- C and C++: text inside a string literal is no longer spaced: `"true"`
  became `" true "`.
- Rust: a one-element tuple keeps its comma. `(5,)` and `(u8,)` became
  `(5)` and `(u8)`, which are not tuples.
- A C header that names a variable `new` or `delete`, as the Linux
  `list.h` helpers do, or has a statement such as `a & b;`, is formatted as
  C. Before, it was read as C++ and laid out wrongly (`new ->next`). A C++
  header whose only C++ is a reference return after a macro the parser
  cannot expand, such as `API_EXPORT int& counter();`, is now read as C,
  which cannot read that declaration, so it is kept as written. Give such
  a header a C++ extension such as `.hpp` to have it formatted.

### Changed

- `[ignore].patterns` and `toolchain.toml` ignore paths match a file
  extension in any case, as discovery has since 0.5.0: `*.c` also ignores
  `A.C`, and `gen/*.h` ignores `gen/B.H`, so a Windows-built tree no longer
  needs each pattern twice. Directory names and the rest of the file name
  still match as written: `vendor/**` does not ignore `Vendor/x.c`.

## [0.6.0] - 2026-10-26

### Added

- `moldy - --stdin-filename NAME` formats source read from stdin and writes
  it to stdout. `NAME` is the file the source stands for: its extension
  (and for a `.h`, its content) chooses the language, and ignore patterns
  match it. `-` with `--in-place`
  or `--check`, or without `--stdin-filename`, exits 2. Before, `-` was
  taken as a file name and the run exited 3.

### Fixed

- A crash inside the formatter now fails only the file it was formatting:
  the file is named as `a.rs: not formatted (internal error: ...)`, the
  other files are still handled, and the run exits 3. Printing to stdout,
  the file's text is printed unchanged, as for a file the formatter
  declines. Before, the crash stopped the whole run with exit 101.
- An unknown `--preset` is reported as `unknown preset 'NAME' (available:
  ...)` instead of as an unsupported language. It still exits 2.

### Changed

- Library: `MoldyError` is now `#[non_exhaustive]` and has a new variant,
  `UnknownPreset`. Both break a `match` on it that lists every variant;
  add a wildcard arm. Later variants will not break such a match again.
- A `.h` or `.H` header whose content can only be C++, such as one holding
  a namespace, class or template, is formatted as C++, as knots already
  reads it. Before, every `.h` was formatted as C, which
  laid out C++ headers by C rules. A header written in C stays C, with a
  known exception: one that names a variable `new` or `delete`, or has a
  statement such as `a & b;`, is read as C++ and can be laid out wrongly
  (`new ->next`). Exclude such a header with an ignore pattern.

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

[Unreleased]: https://github.com/brandon-arrendondo/moldy/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/brandon-arrendondo/moldy/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/brandon-arrendondo/moldy/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.1.0...v0.2.0
