# ADR-0005: Preserve source line endings unless conversion is requested

**Status:** Proposed, 2026-10-12. The maintainer has ruled preservation by
default; this record's interface and mapping await review.

## Context

Before this decision, layout used LF, while opaque emission could retain
carriage returns. Even an otherwise formatted CRLF file can fail `--check`.
A mixed file cannot be preserved merely by detecting one file-wide style.
A formatter should not silently change ending conventions. Like explicit
Git ending conversion, conversion must be requested by an option.

## Decision

The default CLI and `format_source` API preserve LF and CRLF endings on
lines that map through formatting. A mixed input retains each mapped line's
own ending. `--line-endings lf|crlf` requests normalization across the entire
output, including multiline literals/comments. The library offers
`format_source_with_line_endings` and `LineEndings::{Preserve,Lf,Crlf}`.

The legacy `[newlines].style` field remains parsed for compatibility and
continues not to select emitted endings. It cannot silently convert a file
when a preset is selected. The CLI flag or explicit library policy is the
conversion request. `newlines.final_newline` independently controls final
newline presence as before.

### Mapping and newly created lines

For each source/output line, construct a signature by removing whitespace.
Unique signatures in both texts form monotone patience anchors (a longest
increasing subsequence of source positions in output order). Between these
anchors, repeated signatures are matched in order. Bounding those gaps keeps
an inserted brace from taking an unrelated later brace's ending. An unmatched
output line that is the ending fragment of the next nonempty source line
inherits that source ending. This preserves a literal's embedded ending when
wrapping moves its prefix; generic one-character braces do not take this path.
A monotone cursor bounds fragment checks to linear work. Existing
blank lines participate in matching; extra blank lines use the fallback.
This is a text-layout mapping, not analysis, preprocessing, or a complete
lexer. It neither authorizes recovery to drop tokens nor proves recovery
correct. Lines changed beyond whitespace or ambiguously recovered may not
map, and use the fallback.

New, joined, or unmatched lines use the file's predominant ending. This is
more predictable than arbitrarily choosing one of several contributing
lines when wrapping or joining. Count LF and CRLF source endings; a tie uses
the first ending. If there are no source endings, use LF. A mapped EOF line
without an ending also uses this fallback if formatting adds its final
newline. The algorithm needs linear space and O(n log n) anchor work, with
no quadratic edit-distance table or unbounded lookahead.

### Parser and opaque text

Parse the original source and use that exact string for every tree-sitter
slice, consistent with
[ADR-0004](0004-format-text-without-analysis-transforms.md).
After layout/alignment, prepare rendered endings for restoration. CRLF becomes
an internal LF boundary. If the source uses CRLF and contains no standalone
carriage return, a lone CR retained by opaque/error recovery is also a line
boundary; removing it would join code and comments. Any standalone CR in the
original source disables this lone-CR boundary rule for the entire file;
standalone CR remains text. No input preprocessing or analysis transform is
used.
The debug tree route likewise parses the original supplied source.

Restoration covers opaque multiline literals and comments as well as ordinary
layout, so their mapped embedded endings are retained by default. Explicit
normalization intentionally converts them. Standalone carriage returns are
not classified as LF/CRLF endings; this decision does not add support for
classic CR-only files. `--dump-tree` continues to parse the supplied original
source and ignores formatting/output options.

Directive and comment emission removes trailing CR/LF before emitting layout,
so a directive followed by a comment does not acquire a stray space or alternate
between outputs on repeated formatting.

## Compatibility limits

C/C++ backslash-newline splices inside string literals, including inline
assembly, are not reliably parsed with CRLF. Formatting may change continuation
indentation or damage the strings; requesting CRLF can expose the same
limitation on the next pass.

Two consecutive preprocessor directives inside an initializer list, such as
`#pragma message("i")` followed by `#ifdef X`, are enough for CRLF recovery to
lay out differently. The pair alone preserves tokens and is idempotent. With
further directives or initializers after it, recovery can damage tokens and
fail idempotency. This predates ending preservation: version 0.2.1 and the
later allocation fix produce the same text as this implementation after
removing carriage returns. The version 0.2.1 changelog claim that initializer
directives keep their text does not hold for those CRLF recovery cases. The
existing C and C++ initializer-directive fixtures demonstrate this limit.
Ending restoration does not repair parser recovery. Avoid formatting affected
inputs until the formatter supports them.

A UTF-8 BOM is removed from BOM + CRLF input, as it was previously.

## Consequences

- Well-formatted CRLF and mapped mixed files pass `--check` by default.
- `--in-place` does not silently convert their conventions; an explicit
  normalization request can still make a check report differences.
- Uniform LF fixtures retain existing layout. Emitted carriage returns
  originating from CRLF are restored as boundaries instead of joining code
  through existing error-recovery paths.
- Text matching is best effort on malformed recovery. Any token damage
  remains a separate formatter defect, not an ending-policy exception.
- This changes default behavior; record it under Changed per
  [ADR-0002](0002-changelog-is-for-users.md), without choosing a release
  version.
