# ADR-0002: The changelog tells users what changed; it is not a git log or a task list

**Status:** Proposed, 2026-10-10. Awaiting review and acceptance.

## Context

A Moldy user needs to know what a release adds, removes or changes,
especially when formatting output or check behavior changes. A list of
commits or completed work mixes those changes with implementation history.
The formatter validation record already holds comparison evidence; it is
not a release note.

Moldy has no `CHANGELOG.md` today. The `moldy-fmt` package is published
on crates.io; its release workflow requests generated GitHub release notes
rather than drawing from a curated changelog.

## Decision

Create `CHANGELOG.md` starting with the next release as a curated record for
users of Moldy. Each release has one dated section, newest first, with
Unreleased on top. Draw the GitHub release body from that release's section;
a compare link may supplement the notes but does not replace them. Write the
entry in the same change that ships the user-visible effect, under Unreleased.
Use these headings only when they have entries:

1. **Added:** a new language, CLI option, preset or formatting
   capability. Describe what the user can now do.
2. **Fixed:** a crash, incorrect output, nondeterminism or other tool defect.
   Describe the failing construct and resulting behavior in user terms.
3. **Removed:** an option, output field, platform or supported behavior
   a user could have relied on. Even small removals must be recorded.
4. **Changed:** a change to existing output's meaning or an option's behavior,
   including changed defaults. It is not a category for internal work.

**A formatting-policy change is listed under Changed, even when it
corrects a bug.** Explain which constructs and presets change, so users
know to review formatting diffs. Describe the correction in that entry
rather than duplicating it under Fixed.

Entries are short publication-ready explanations, not copied commit subjects
or work-item titles. Exclude validation and corpus work, paper and docs-only
edits, CI and packaging chores, refactors, tests or fixtures added for their
own sake, and dependency changes with no user-visible effect. If any of those
ships a user-visible capability or fix, describe that effect instead.

Do not publish internal tracking references or locate defects in another
project that have not been fixed upstream. The changelog is part of the public
record, and the intention is to ship it in release packages; today's packages
do not include it.

## Consequences

- Release notes require editorial review; automation can collect candidates
  but cannot decide that every completed change deserves an entry.
- Group related effects so a release remains readable. Absence of internal
  work from the changelog is correct, not missing attribution.
- Backfilling older releases is a separate maintainer decision. Git history
  remains the implementation record; this ADR authorizes no history rewrite
  or replacement of already-published archives.
- Deprecated and Security headings are not used. A security fix is a Fixed
  entry, published only once fixed; if it involves another project, wait
  until the fix has landed upstream. Nothing is deprecated yet; if something
  is, add the Deprecated heading then.
- Validation results stay in the validation record. A formatting-change
  entry points readers to that evidence without turning the changelog into
  a benchmark report.

Origin: adaptation of knots ADR-0004, itself ported from aurora-lint
ADR-0009, restated for Moldy's user-visible behavior.

Source record: [knots ADR](https://github.com/brandon-arrendondo/knots/blob/7dc71958c024b08726de5a095fa4d94abddbce95/docs/adr/0004-changelog-is-for-users.md).
