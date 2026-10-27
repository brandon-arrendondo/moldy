# ADR-0007: Moldy checks that code reads the same; layout follows the reference tool

**Status:** Accepted, 2026-10-25 (Brandon Arrendondo). Proposed
2026-10-24. Implemented for C, C++, Rust and Python. [Amendment
1](#amendment-1-a-comment-stays-by-the-code-it-describes), comment
placement: Proposed 2026-10-27, implemented for C, C++, Rust and Python.

## Context

A formatter's output differs from its input in two ways. Some differences
are layout: where lines break, how far they indent, whether a short list
stays on one line. Others change what the code is: a token moved into a
comment, an indentation change that ends a Python block early, a comma
dropped from a one-element tuple. The first kind is a matter of style, and
projects disagree about it. The second kind is a bug in every project.

Where that line falls depends on the language. In Rust almost no whitespace
carries meaning, but a `//` comment ends only at the end of its line, so
joining the next line onto it comments that code out. In C, preprocessor
lines end at the line break, and `#define F (x)` defines a different macro
from `#define F(x)`. In Python, line breaks and indentation are syntax
outside brackets and layout inside them.

Moldy's Markdown formatter already parses its output again and compares it
with the input, and leaves a file unchanged when the two differ. The C,
C++, Rust and Python formatters do not; they continue over whatever they
produce. A trial adoption on a real Rust and Python workspace
found output that no longer compiled or parsed: code after a line comment
folded into the comment, and the last `else` or `except` of a compound
statement moved to column 0. Run on 400 files of the Python standard
library, version 0.5.0 produced output that no longer parsed for 247 of
them; on 600 Rust files from crates.io, it broke 21 and changed the code
of 12 more. Every one of those differences would have been caught by
comparing the output's tokens with the input's.

The same trial also showed differences from rustfmt that are not bugs:
method chains joined onto one line, and attributes on parameters given a
line of their own. rustfmt follows the Rust Style Guide, which sets rules
of its own for Rust constructs: chains split one call per line once they
pass a width, struct literals split past a much smaller width than calls,
a closure as the last argument stays on the call's line, and continuation
lines take block indentation rather than visual alignment. Matching those
rules is layout work, and it can be ranked and done a piece at a time.

## Decision

Moldy separates the two kinds of difference and treats them differently.

**Correctness is checked, and is never an option.** The output must read as
the same program as the input. For each language that means:

- **Rust, C and C++:** the same tokens in the same order, and the same
  comments with the same text. Moldy may add or remove only what its layout
  rules permit, such as a trailing comma in a list it splits one item per
  line.
- **Python:** the same tokens and comments, and the same block structure:
  every statement belongs to the same block as before.
- **Markdown:** as now, the same document shape, read by the formatter's
  grammar and by an independent CommonMark reader
  ([ADR-0006](0006-layout-not-authored-style.md), guarantee 1).

Moldy checks this on every run. It parses its output with the same grammar
and compares the result with the input's. If they differ, or the output has
a syntax error where the input had none, Moldy leaves the file unchanged
and reports it as declined, which makes the run incomplete
([ADR-0003](0003-incomplete-runs-never-pass-as-clean.md)). A file whose
input has syntax errors is still formatted, as now, when the check passes:
text the grammar could not read is kept as written and compared like any
other.

**Moldy is a formatter, not a code corrector.** Where the input has a
syntax error, Moldy never adds the token the parser assumed was missing,
and never drops or joins tokens to get past it, in any language. It either
keeps the damaged text as written, where that is safe, or declines the
file. Keeping text as written is the main path; the check above proves the
result, and declining is the backstop.

The check uses the grammar Moldy formats with, so it cannot see a mistake
the grammar makes in reading both input and output. Two further measures
cover that:

- Where the grammar misreads input the language accepts, Moldy keeps the
  affected text as written rather than lay it out by the misreading, as the
  markup formatters already do.
- Development runs an external check on real code: the output of each code
  formatter is compiled or parsed by the language's own tools (`rustc` or
  `rustfmt`, Python's `ast`, a C compiler) over large public corpora, and
  compared with the input's reading. A change to a code formatter runs it.

Output is also stable: formatting it again changes nothing (ADR-0006,
guarantee 3). Stability is checked in the test suite and the external
check, not on every run.

**Layout follows the reference tool, and a judgment call gets an option.**
Each language's layout targets its reference tool's default output:
funky for C and C++, rustfmt for Rust, Ruff for Python, and the tools
ADR-0006 names for the markup languages. A rule the reference tool applies
is layout even when it is specific to the language, such as rustfmt's
widths per construct or its treatment of a closure as the last argument.
Where Moldy differs from its reference tool, the difference is known layout
work, measured as changed lines on real code and done in that order. Where
projects reasonably disagree about a layout rule, it gets a configuration
option whose default is the reference tool's choice (ADR-0006).

Where a layout rule would produce code the grammar cannot read back,
correctness wins and the layout gives way. A closure's last parameter takes
no trailing comma when its parameters are split, for that reason.

## Consequences

- A Rust, Python, C or C++ file that Moldy cannot format without changing
  its code is left unchanged and reported, instead of being rewritten with
  a broken result. A run that declines a file exits 3 under ADR-0003, so a
  formatter bug fails CI instead of passing as reformatted code.
- The check costs one more parse per formatted file.
- `CLAUDE.md` and the troubleshooting guide say that the code formatters
  continue over syntax errors and that success does not establish valid
  syntax. When the check is implemented, they say instead that Moldy
  confirms the output reads the same as the input, or declines the file.
- ADR-0003 says that tree-sitter recovery nodes do not by themselves mean
  formatting failed. That stays true; this record adds the test for when a
  file with recovery nodes may be rewritten: the comparison above passes.
- ADR-0006's first guarantee, that meaning is kept, becomes a checked
  property for the code languages as well as the markup languages.
- Differences from rustfmt, Ruff and funky are tracked as layout work,
  ranked by how much of a real project's diff each accounts for. Bringing
  the `rustfmt-compat` preset closer to rustfmt starts with method chains,
  then rustfmt's widths per construct, then a closure or block as the last
  argument.

## Amendment 1: a comment stays by the code it describes

**Status:** Proposed, 2026-10-27. Implemented for C, C++, Rust and
Python.

### Context

Where a comment sits is part of what it says. In

```c
typedef enum {
    MODE_A = 0, /* first mode */
    MODE_B,     /* second mode */
} mode_e;
```

each comment describes the enumerator on its line. Moved onto the next
line, the same text describes the next enumerator. `#endif /* FEATURE */`
names the conditional it closes; on a line of its own after the `#endif`,
it reads as a note on the statement that follows. Joining
`x = a || /* why a */` with the next line puts the comment in front of
`b`, and the reader can no longer tell which operand it explains.

The check above compares comments by their text and order. A formatter
that made these moves passed it, since the code and the comment text were
unchanged, while each comment came to describe different code. Run on a
real embedded C project, one release made 31 such moves.

### Decision

Correctness includes where each comment is. For C, C++, Rust and Python,
the check compares, for every comment:

- the code tokens on either side of it, so that no code moves past a
  comment;
- whether code comes before it on its line, a trailing comment, and if
  so, that the code token before it is still on its line;
- whether code follows it on its line, and if so, that the code token
  after it is still on its line.

A comment's column, its indentation and the blank lines around it are
layout and may change. A token a formatting option adds or removes, such
as a brace around a control statement's body, is not code a comment moved
past, but a trailing comment must still share its line with the code it
followed. Output that moves a comment is declined, as output that changes
code is.

Where a layout rule would move a comment, the comment's place wins: an
expression is not joined onto one line across a comment, and a trailing
comment does not get a line of its own.

### Consequences

- A comment never moves past code, and a trailing comment stays trailing.
  Where a formatter cannot yet lay out the code around a comment without
  moving it, the file is declined rather than formatted.
- Where the reference tool moves a comment, Moldy differs from it. funky,
  for example, puts a comment that follows a `{` on a line of its own;
  Moldy keeps it after the `{`.
