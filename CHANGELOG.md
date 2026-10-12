# Changelog

What changed for users of Moldy, newest release first. See
[ADR-0002](docs/adr/0002-changelog-is-for-users.md) for what belongs here.

## [Unreleased]

### Added

- `--line-endings lf|crlf` explicitly converts output line endings. Library
  users can select `LineEndings` with `format_source_with_line_endings`.

### Fixed

- C/C++ files no longer abort with a memory allocation failure when a
  `{ ... }` block is the first statement inside another block, such as a
  function, `if` or loop body. These blocks now format and indent like
  other blocks.

### Changed

- A `{ ... }` block that is the first statement of a `switch` body or
  another block, and ends with a `case` label, now indents both braces
  like the same block elsewhere. Version 0.2.1 printed its opening brace
  at column 0 and its closing brace one level too deep.

### Changed

- Formatting preserves LF and CRLF endings by default, including mixed
  files. New or joined lines use the predominant source ending.
  Previously, formatting emitted LF and could leave carriage returns in
  comments and strings. Otherwise formatted CRLF files now pass `--check`.
- `[newlines].style` has no effect on output endings. This field was already
  unused; use `--line-endings` to request conversion.

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

[Unreleased]: https://github.com/brandon-arrendondo/moldy/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/brandon-arrendondo/moldy/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.1.0...v0.2.0
