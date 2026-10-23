# ADR-0003: An incomplete or failed run never passes as clean

**Status:** Accepted, 2026-10-12 (Brandon Arrendondo). Proposed 2026-10-10.
Implemented 2026-10-23, with the exit-status contract below as ruled on
2026-10-23; Context describes the behavior before that change.

## Context

`moldy --check` is a CI gate: success is taken to mean the selected source
already has the requested formatting. An incomplete run cannot support that
conclusion, even if the files it managed to check were unchanged.

Today, `expand_paths` warns and skips a directory supplied without `-r`.
A run selecting no files returns success, including under `--check`. Read,
walk and formatting errors already propagate through the CLI, but those
existing error paths do not cover the skipped-directory or empty-selection
cases. Exit 1 currently covers both formatting differences (the explicit
`process::exit(1)`) and failures returned from `main` through
`anyhow::Result`; Clap usage errors already exit 2. The mapping below is
proposed, including moving operational failures from 1 to 2.
The mapping first proposed here (2 for every failure) was superseded on
2026-10-23 by the suite-wide contract below.

## Decision

Except for an explicit empty-selection opt-out, a run reports success only
after it completes the requested operation on all selected files. In check
mode, success additionally means none needs a formatting change. A failure to discover, read, parse for formatting, format
or write a selected file cannot produce a clean result.

- A directory explicitly supplied without recursive mode is an invocation
  error, not a warning followed by successful omission.
- A selection containing no files fails the gate by default. With the
  `--allow-empty` flag, it reports the empty selection on stderr and exits
  0. Without that flag, it reports the empty selection and exits 3.
  An empty selection is distinct from an empty source file, which can be
  valid input and can already be correctly formatted.
- Declared ignore rules and recursive extension filtering define the
  selection. A deliberately excluded file is not a failed file. An explicit
  supported file that is selected but cannot be processed is a failure.
- A failure on one selected file is contained to it: Moldy names the file
  and its reason on stderr and goes on to the others, then ends the run as
  incomplete.
- A file the formatter declines and leaves unchanged on purpose (a syntax
  error in a strict grammar, a Markdown rewrite it cannot verify, nesting too
  deep) makes the run incomplete. `--check` cannot vouch for a file it did
  not check.
- Report every file's outcome. `--check` names each file that would change
  and `--in-place` each file it rewrote; a declined or failed file is named
  with its reason in every mode. Under `--check` and `--in-place` a summary
  of the counts always ends stderr; a run to stdout prints it only when
  incomplete. `--completion-report FILE` writes every file's outcome as JSON,
  even after a failure.
- Use this exit-status contract, the same as aurora-lint ADR-0017 and knots
  ADR-0005:
  - **0:** completed and clean in `--check` mode, or completed formatting;
    also an explicitly allowed empty selection with `--allow-empty`.
  - **1:** completed in `--check` mode, with formatting differences found.
  - **2:** the run could not start: a usage error, a configuration error
    (unknown preset, malformed `moldy.toml`, `--config` file or
    `toolchain.toml`), a named path that does not exist, or a directory
    named without `-r`. No file is touched.
  - **3:** the run started but is incomplete: a selected file could not be
    reached, read, formatted or written, or was declined; or an empty
    selection without `--allow-empty`. It outranks 1. A completion report
    that cannot be written also exits 3.
  The usage guide documents these statuses, and CLI regressions cover each
  one.
- Already produced output does not turn a partial run into a completed one.
  A failure after earlier in-place writes must remain a failure and explain
  that the operation may have changed those files. This decision does not
  promise transactional rollback across a batch.

Tree-sitter recovery nodes do not by themselves mean formatting failed.
Moldy can preserve syntax it does not structurally handle; the test is whether
it completed its formatting operation while preserving the supplied text,
not whether the parser reported a perfectly error-free tree.

## Consequences

- CI can distinguish a clean completed check, formatting differences, and
  inability to complete. Skipped requested input cannot silently pass.
- Empty-selection workflows must make their intention explicit instead of
  relying on an accidental successful no-op.
- Shipping this contract is a user-visible change for `moldy-fmt` on
  crates.io. Record it under Changed in the changelog, as
  [ADR-0002](0002-changelog-is-for-users.md) proposes, including the new
  empty-selection default, the declined-file status and the exit statuses.
- Error reporting must not imply that earlier in-place changes were undone.

Origin: the incomplete-run principle in knots ADR-0005, adapted from analysis
coverage to formatting completion. Moldy has no analysis baseline to replace.

Source record: [knots ADR](https://github.com/brandon-arrendondo/knots/blob/7dc71958c024b08726de5a095fa4d94abddbce95/docs/adr/0005-incomplete-scans-never-pass-as-clean.md).
