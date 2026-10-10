# ADR-0003: An incomplete or failed run never passes as clean

**Status:** Proposed, 2026-10-10. Awaiting review and acceptance. This is a
behavioral target, not an implemented guarantee.

## Context

`moldy --check` is a CI gate: success is taken to mean the selected source
already has the requested formatting. An incomplete run cannot support that
conclusion, even if the files it managed to check were unchanged.

Today, `expand_paths` warns and skips a directory supplied without `-r`.
A run selecting no files returns success, including under `--check`. Read,
walk and formatting errors already propagate through the CLI, but those
existing error paths do not cover the skipped-directory or empty-selection
cases. [The usage guide](../usage.md) describes current behavior; this ADR
proposes changing it rather than claiming the change has shipped.

## Decision

A run reports success only after it completes the requested operation on all
selected files. In check mode, success additionally means none needs a
formatting change. A failure to discover, read, parse for formatting, format
or write a selected file cannot produce a clean result.

- A directory explicitly supplied without recursive mode is an invocation
  error, not a warning followed by successful omission.
- A selection containing no files is reported as an empty selection and
  fails the gate. It is distinct from an empty source file, which can be
  valid input and can already be correctly formatted.
- Declared ignore rules and recursive extension filtering define the
  selection. A deliberately excluded file is not a failed file. An explicit
  supported file that is selected but cannot be processed is a failure.
- Report incompleteness on stderr and use a nonzero exit status distinct
  from the status for completed checks that found formatting differences.
  The implementation must document those statuses and cover them with CLI
  regressions before this guarantee is advertised.
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
- Discovery, output modes and exit-status handling need implementation and
  tests. This draft changes none of them.
- Error reporting must not imply that earlier in-place changes were undone.

Origin: the incomplete-run principle in knots ADR-0005, adapted from analysis
coverage to formatting completion. Moldy has no analysis baseline to replace.

Source record: [knots ADR](https://github.com/brandon-arrendondo/knots/blob/7dc71958c024b08726de5a095fa4d94abddbce95/docs/adr/0005-incomplete-scans-never-pass-as-clean.md).
