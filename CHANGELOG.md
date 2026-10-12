# Changelog

What changed for users of Moldy, newest release first. See
[ADR-0002](docs/adr/0002-changelog-is-for-users.md) for what belongs here.

## [Unreleased]

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

[Unreleased]: https://github.com/brandon-arrendondo/moldy/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/brandon-arrendondo/moldy/compare/v0.1.0...v0.2.0
