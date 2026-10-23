# ADR-0006: Moldy normalizes layout, and a project can keep its own conventions

**Status:** Proposed, 2026-10-23. Awaits the maintainer's review. The
reStructuredText `[rst]` options it describes are implemented.

## Context

A formatter and a prettifier are hard to tell apart when the input is
human prose. Code formatters such as rustfmt and Black change only layout:
indentation, spacing and line breaks, under rules any reader can predict.
Prose and markup tools such as Prettier and docstrfmt go further. They also
choose between equivalent spellings of the same markup, such as which
character underlines a title, which character marks a bullet, and how many
spaces follow a sentence. docstrfmt goes further still, and rewrites some
text in ways that change what the document means.

Moldy's markup formatters target a reference tool's default output:
Prettier for Markdown and YAML, docstrfmt for reStructuredText. Running the
reStructuredText formatter on a real Sphinx project showed what that costs.
The project writes underline-only titles, `*` bullets and two spaces
between sentences, consistently and on purpose. Moldy rewrote every file,
and most of the diff was those conventions rather than layout. A project
cannot adopt a formatter that overrides its documented style on every run.

## Decision

Moldy's guarantees, for every language:

1. **Meaning is kept.** The output means what the input means, by the
   language's own reading: compiler tokens, docutils' document tree,
   CommonMark rendering. Where Moldy cannot confirm that, it leaves the file
   unchanged and reports it ([ADR-0003](0003-incomplete-runs-never-pass-as-clean.md)).
   A reference tool's rewrites that change meaning are not copied.
2. **Words are the author's.** Moldy does not change wording, spelling,
   punctuation inside text, the order of fields or arguments, escapes, or
   code and literal content.
3. **Output is stable.** Formatting the output again changes nothing.

Within those guarantees Moldy changes two kinds of thing:

- **Layout:** indentation, blank lines, spacing and, where a language's
  reference tool does so, rewrapping prose to a width. Layout is
  configurable (width, indentation) but always normalized.
- **Conventions:** choices between equivalent spellings that an author may
  make on purpose, such as title adornments, bullet characters and sentence
  spacing. By default Moldy normalizes them as the reference tool does, so
  that a project with no stated style gets the reference tool's. Each
  convention that a project might hold deliberately has a `preserve`
  option, which keeps the source's own choice and normalizes only what
  surrounds it, such as an adornment's length.

A new convention normalized by default needs a `preserve` option before a
project is asked to adopt the formatter. A change to the default stays in
line with the reference tool.

## Consequences

- reStructuredText has `[rst] heading_style`, `bullet_style` and
  `sentence_spacing`, each `normalize` (the default, docstrfmt's choice) or
  `preserve`. They are documented in the
  [configuration guide](../configuration.md).
- Rewrapping prose is layout, not a convention. A line break inside a
  reStructuredText paragraph means nothing to docutils, so text an author
  broke by hand is rewrapped. Markdown follows Prettier's
  `proseWrap: preserve` and keeps its line breaks. A reStructuredText
  option to keep line breaks would be a separate decision, if a project
  needs one.
- A preserved convention is still checked: the output must keep the
  document's meaning, and inconsistent input (a list whose items use
  different bullets, a skipped title level) is refused as before.
