# ADR-0004: Moldy formats text and never runs analysis transforms

**Status:** Proposed, 2026-10-10. Records the implemented formatting invariant;
awaiting review and acceptance.

## Context

The language substrate offers grammar and registry facilities as well as
analysis helpers. Some consumers blank provably inactive preprocessor lines
before analysis. That transform is useful for measuring active code, but a
formatter must retain source that belongs to other build configurations.

Moldy's formatters slice text using tree-sitter byte offsets. A tree parsed
from one string and offsets applied to another string are not a valid pair.
[The architecture guide](../architecture.md) already describes this boundary.

## Decision

Parse the supplied source text for formatting and use that same text for
all node slices and source-preserving emission. Do not run substrate dead-code
blanking or other analysis transforms on the formatting path. This also
applies to the debug tree route: it describes the source being formatted.

Inactive `#if` branches remain source to preserve and format, not material
to erase. Moldy reconstructs whitespace according to its configuration;
this decision does not require every inactive branch to remain byte-for-byte
unchanged. Unknown constructs and opaque regions retain their documented
passthrough behavior. It does not promise unsupported format-off markers.

Grammar lookup and language facts may come from the substrate. Metrics,
call extraction, control-flow graphs and fingerprints are not formatting
preconditions. Adding a substrate capability does not authorize changing
this boundary.

## Consequences

- Formatting retains code needed by alternate build configurations.
- A parser tree and its source slices always refer to the same text. A new
  emission path must preserve that invariant rather than borrowing an
  analysis consumer's transformed source.
- Future formatting changes that touch preprocessor handling must verify
  preservation of inactive branches as well as the emitted whitespace.
- This decision records existing behavior; it adds no analysis pipeline or
  new formatting feature.
