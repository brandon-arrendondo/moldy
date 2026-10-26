# CLI usage and language support

```text
moldy [OPTIONS] <FILES>...
```

| Option                       | Behavior                                                                                      |
| ---------------------------- | --------------------------------------------------------------------------------------------- |
| `-i`, `--in-place`           | Write changed files back to their paths.                                                      |
| `--check`                    | Write no formatted output; report changed filenames on stderr and exit 1 if any would change. |
| `--line-endings <STYLE>`     | Explicitly normalize output endings; default preserves mapped source endings.                 |
| `-r`, `--recursive`          | Walk directories, following symbolic links.                                                   |
| `--stdin-filename <NAME>`    | With `-`, the file name the source read from stdin stands for; see [below](#standard-input). |
| `--allow-empty`              | Accept a run that selects no files: report it on stderr and exit 0 instead of 3.              |
| `--completion-report <FILE>` | Write each selected file's outcome and the counts to `FILE` as JSON, even when the run fails. |
| `-c`, `--config <FILE>`      | Load this TOML file instead of checking for `moldy.toml` in the current directory.            |
| `--preset <NAME>`            | Use a bundled preset instead of a config file; conflicts with `--config`.                     |
| `--dump-tree`                | Print each file's tree-sitter CST; this debug option is hidden from help.                     |
| `-h`, `--help`               | Print help.                                                                                   |
| `-V`, `--version`            | Print version.                                                                                |

`--check` and `--in-place` cannot be combined. Without either option, formatted files are concatenated on stdout without filename separators. Use one input per command when redirecting stdout to a file.

## Standard input

`moldy - --stdin-filename NAME` reads the source from stdin and writes the formatted result to stdout. Stdin has no extension, so `NAME` stands in for the file's path: its extension or file name, and for a `.h`, its content, chooses the language, as the table [below](#language-detection) gives it, and the config's ignore patterns match it. `NAME` need not exist. An editor passes the path of the buffer it pipes:

```sh
moldy - --stdin-filename src/parser.rs < src/parser.rs
```

The run ends like a run on one file, with the same exit statuses, and messages name `NAME`. A `NAME` whose extension or file name Moldy does not format exits 3 (`unsupported language`), as naming such a file does, and prints nothing. A source the formatter declines, or one that crashes it, is printed as read and exits 3. A `NAME` an ignore pattern covers selects nothing: the source is printed as read, and the run exits 3, or 0 with `--allow-empty`. Each of these is a usage error (exit 2): `-` without `--stdin-filename`, `--stdin-filename` without `-`, `-` beside another path, and `-` with `--in-place` or `--check`.

## Line endings

By default, CRLF stays CRLF and LF stays LF. In a mixed file, lines that
map through formatting keep their own endings, including blank lines and
opaque multiline strings. Matching ignores formatting whitespace and uses
unique neighboring lines to bound repeated-line matches. The final fragment
of a wrapped line can retain its source ending, including when a literal's
prefix moves to a new line. New, joined, or
otherwise unmatched lines use the file's predominant ending; a tie uses the
first source ending, and a source with no endings uses LF. This mapping is
best effort when malformed-input recovery drops or synthesizes text.

`--line-endings lf` or `--line-endings crlf` explicitly converts every output
ending, including endings in opaque multiline text. The existing
`newlines.final_newline` setting still controls whether the output ends in a
newline; normalization changes its convention, not that setting.

A well-formatted CRLF file passes `--check` without a normalization flag.
A normalization flag can make it fail `--check` because the requested endings
differ. `--in-place` uses the same policy. Presets and the legacy parsed
`[newlines].style` field do not request conversion; use the flag, or the
library's `format_source_with_line_endings` API. See
[ADR-0005](adr/0005-preserve-source-line-endings.md) for the mapping decision.

## Language detection

| Language | Recursive and explicit                | Explicit only |
| -------- | ------------------------------------- | ------------- |
| C        | `.c`                                  | `.h`          |
| C++      | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx` | —             |
| Rust     | `.rs`                                 | —             |
| Python   | `.py`                                 | —             |
| JSON     | `.json`, `.jsonc`                     | —             |
| YAML     | `.yaml`, `.yml`                       | —             |
| Markdown | `.md`, `.markdown`                    | —             |
| CMake    | `CMakeLists.txt`, `.cmake`            | —             |
| Bash     | `.sh`, `.bash`                        | —             |

Extensions and file names come from the compiled substrate registry. Extensions match in any case, so `A.C` and `B.H`, as Windows-built source trees often name them, are C and `.CPP` is C++; GCC's convention that `.C` is C++ is not followed. File names such as `CMakeLists.txt` match as written. A `.h` path selects C unless its content can only be C++ (a namespace, a class, a template), by the substrate's header detection, as knots reads it; then it is formatted as C++. A header in the subset of C++ that is also C stays C. Known limit: a C header that names a variable `new` or `delete`, or has a statement such as `a & b;`, is read as C++ and can be laid out wrongly (`new->next` becomes `new ->next`); exclude such a header with an ignore pattern. Shell scripts without an extension are not recognized, even with a shebang line; give them `.sh` or `.bash`. Enabling another grammar alone does not implement formatting for that language.

Directories require `--recursive`; a directory named without it stops the run before any file is touched, with exit 2. So does a named path that does not exist. Recursive discovery uses the substrate's `is_discoverable_file`, which selects the extensions and file names above and skips `.h`. A directory the walk cannot read, or a dangling symbolic link it would have selected, is reported as not formatted; a symbolic link loop is not, since its target is walked anyway. Explicit file paths are attempted even if their extension is unsupported, and are then reported as not formatted (`unsupported language`). Ignore patterns apply to both explicit paths and recursive files, before any file is read; an ignored file is not selected, even when named. See [configuration](configuration.md).

## Output and exits

Inputs must be UTF-8, whether files or [standard input](#standard-input).

Each selected file ends one of four ways: changed (it would be, or was, reformatted), unchanged, declined (the formatter left it unchanged on purpose; see [below](#json-yaml-markdown-cmake-and-bash)), or failed (it could not be read, is not UTF-8, is in an unsupported language, or could not be written). A file that declines or fails does not stop the run: Moldy names it on stderr, for example `a.yaml: not formatted (YAML syntax error at line 3)`, and goes on to the next file. Printing to stdout, a file the formatter declines, or one that crashes it (`internal error: ...`), is printed unchanged; a file that cannot be read, is not UTF-8 or is in an unsupported language prints nothing. `--check` names each file that would change (`a.c: would reformat`) and `--in-place` each file it rewrote (`a.c: reformatted`).

Under `--check` and `--in-place`, a summary always ends stderr, for example `moldy: 12 files: 3 would be reformatted, 8 unchanged, 1 not formatted`. A run to stdout prints it only when the run is incomplete, so a single file piped through Moldy adds nothing to stderr. An incomplete run then says how many selected files were not formatted. Under `--in-place` it also says that the files it already reformatted stay rewritten: nothing is rolled back.

| Status | Meaning |
| ------ | ------- |
| 0 | Completed: every selected file was formatted, or `--check` found nothing to change. Also an empty selection with `--allow-empty`. |
| 1 | `--check` completed and found files that would be reformatted. |
| 2 | The run could not start: invalid arguments, an unknown preset, a malformed `moldy.toml`, `--config` file or `toolchain.toml`, a named path that does not exist, or a directory named without `-r`. No file is touched. |
| 3 | Incomplete: a selected file declined or failed, or no file was selected without `--allow-empty`. It outranks 1, and also covers a `--completion-report` that cannot be written. |

A run that selects no files is incomplete, because a check of nothing does not establish that a project is formatted. Pass `--allow-empty` when an empty selection is expected.

`--completion-report <FILE>` writes the outcome of every selected file as JSON, whatever the mode, and writes it even when the run fails, so an `--in-place` run shows which files it did rewrite. Moldy removes an existing report file as soon as it has read its arguments, so a run that cannot start (exit 2) leaves no report, never a stale one; an argument error that clap reports stops moldy before that and leaves the file alone. A file named twice is handled, and counted, once:

```json
{
  "complete": false,
  "mode": "in-place",
  "files": { "selected": 3, "changed": 1, "unchanged": 1, "declined": 1, "failed": 0 },
  "results": [
    { "file": "a.c", "status": "changed" },
    { "file": "b.yaml", "status": "declined", "reason": "YAML syntax error at line 3" },
    { "file": "c.c", "status": "unchanged" }
  ]
}
```

`mode` is `check`, `in-place` or `stdout`, and `complete` is `false` exactly when the run exits 3. In stdout mode, `changed` means the formatted output differs from the file. `results` is sorted by file, and `reason` appears only for a declined or failed file.

`--dump-tree` takes precedence over formatting inside the file loop, including when paired with `--check` or `--in-place`.

## Formatting limits

C/C++ targets funky's rules, verified against checked-in fixtures. The C, C++, Rust and Python formatters continue over trees with syntax errors; C/C++ unknown nodes fall back to their source text. A successful formatting call is not a syntax-validation result or a guarantee that invalid input is unchanged. Review diffs before accepting in-place changes to malformed source.

Rust's `rustfmt-compat` preset enables width-based list wrapping and short field-list collapsing, but it is not complete rustfmt parity. Attributes and macro invocations remain opaque. Rustfmt's packed multiline list layout is not implemented.

Python defaults to width 79; the `black` preset selects 88. Strings and f-strings remain opaque, with no quote normalization. Comprehensions, subscripts/slices, and multi-target assignments lack width-based wrapping. Magic trailing commas do not force multiline output. The width is a wrapping budget for supported constructs, not a hard maximum line length. The presets do not guarantee complete Black/Ruff or PEP8 conformance for arbitrary source.

C/C++ string literals with backslash-newline continuations, including inline
assembly, are not reliably parsed with CRLF: formatting can change indentation
or damage strings. Explicit CRLF conversion can expose this on a later pass.
Two consecutive preprocessor directives inside an initializer list can lay out
differently with CRLF. The pair alone preserves tokens and is idempotent; with
further directives or initializers after it, recovery can damage tokens and
fail repeated-formatting checks. This predates ending preservation; the 0.2.1
initializer-directive fix did not cover those CRLF recovery cases.

UTF-8 BOMs are removed from BOM + CRLF files. See ADR-0005 for these limits.

### JSON, YAML, Markdown, CMake and Bash

Each of these formatters leaves a file with any syntax error unchanged and
declines it: Moldy names it and the line of the first error, for example
`a.yaml: not formatted (YAML syntax error at line 3)`. The run continues
with the other files, but it is incomplete and exits 3, in `--check` and
`--in-place` alike: a file Moldy did not format has not been checked. A
file nested deeper than Moldy handles is declined the same way, as nested
too deeply. The limit depends on the construct: about 60 levels of YAML
block indentation at two spaces, 100 levels of parentheses in a CMake
argument, about 200 levels of JSON objects or CMake blocks, and a few
hundred levels of brackets, quotes, lists or blocks elsewhere. Markdown
emphasis nested more than about 240 deep, and inline text with
thousands of backticks, is kept as written. A Markdown file is left
declined the same way when its paragraphs, headings and table
cells hold very long `*` and `_` runs (one run of about 1,000, or many
shorter ones) or very many backtick runs in all, or when its lines, in code blocks too, hold very long runs
of `` ` `` or `~`, or of `*` or `_` with blanks (a few thousand on a line,
or many shorter ones).
Without `--check` or `--in-place`, the file's text is printed unchanged.
Each targets an
external formatter's default output, checked against fixtures in
`tests/<language>_corpus`; matching those fixtures is not a guarantee of
identical output on every file. Long scalars, prose and comments are never
re-wrapped.

**JSON** expands every non-empty object and array to one member per line,
the layout of `jq .` and Python's `json.dumps(indent=2)`. Keys, strings and
numbers are kept as written. `.jsonc` comments are kept, and one blank line
between members is kept. A trailing comma is a syntax error to the grammar,
and so, wrongly, is a `+` in an exponent (`1e+10`); a file with either is
declined. A member with a comment between its
key and value is kept as written.

**YAML** targets Prettier's YAML output: block collections are re-indented,
flow collections are written `[a, b]` and `{ a: 1 }` and broken one item
per line when they do not fit, and quoted scalars prefer double quotes
where that needs no new escapes. Explicit `? key` entries, and flow
collections holding comments or multi-line scalars, keep their text and
are only re-indented. A `|` or `>` block scalar is only re-indented: its
trailing spaces and tabs, its lines of blanks, and the blank lines a `+`
scalar keeps are part of its value and stay, where Prettier drops a last
line of blanks. A block scalar that is a whole document keeps its
indentation, and one that ends the file without a final newline gets
none, since the newline would become part of its value. A file is
declined when moving an indented top-level scalar to
column 0 would turn a line starting with `---` or `...` into a document
marker. A `...` document end is dropped, as Prettier does, except before
directives, where it stays, and before a document without its own `---`,
where it becomes `---` so the two documents stay apart. Repeated `...`
markers count once and add no empty document, where Prettier prints one.
A comment on a dropped `...` moves to its own line, where Prettier keeps
the `...`.

**Markdown** targets Prettier with `proseWrap: preserve`: paragraph line
breaks stay where they are. Headings become `#` headings, lists are
renumbered and re-marked as Prettier does, tables are aligned by display
width, `*emphasis*` becomes `_emphasis_`, and front matter and `yaml`
fences are formatted as YAML. Other fenced code, and indented code
(including its tabs), is kept as written. Emphasis is paired by
CommonMark's rules, not the grammar's, so `_` between letters or digits
(`ota_0 or ota_1`) stays text. A delimiter is rewritten only where `_`
pairs the same way at both ends, next to ASCII text or space, and the
result keeps the same emphasis; otherwise it stays as written. Lines that
continue a paragraph under CommonMark but that the grammar reads as a new
block (a list starting at a number other than 1, or a line indented 4
columns) are kept as written; Prettier joins them into one line. Escapes
are never added or removed, and a multi-line setext heading stays setext.

Markdown output is checked twice before it is used. It is parsed again
with the same grammar and compared with the input, and both are also read
with an independent CommonMark parser (pulldown-cmark, with GFM tables,
task lists and strikethrough), which catches input the grammar misreads
the same way twice. Block structure, heading levels, list looseness and
start numbers, emphasis, links, hard breaks, text and code block contents
must all match. Moldy also refuses input its grammar reads differently
from CommonMark and GFM: a code fence closed on an indented line or after
text; a table with a `|` inside a cell, more cells in a row than its
header, or a row indented 4 columns; and a nested or adjacent block after
lines that continue a paragraph. In each case the file is left unchanged
and declined, for example `a.md: not formatted (Markdown could not be
formatted without changing its meaning (line 12))`. Front
matter and `yaml` code blocks are formatted as YAML and are not part of
the comparison; one holding a `{{ }}` or `{% %}` template is left as
written. A trailing tab, or spaces followed by a tab, is not turned into
a hard break. A leading byte
order mark is removed, as for every language.

**CMake** targets gersemi's default output: bodies are indented four
spaces, and a built-in command is lowercased and broken into one argument
group per line when it does not fit in 80 columns. Moldy carries its own
table of built-in command signatures, written from the CMake
documentation; it covers common commands rather than all of them, and
subcommand forms such as `string`, `file` and `list` are treated as
positional. A user-defined function or macro keeps its arguments as
written; only its indentation changes, and the closing parenthesis of a
multi-line call moves to its own line. Some keyword layouts differ from
gersemi, for example in `install(TARGETS ...)`. A parenthesized
condition group that holds a comment is expanded one word per line so
the comment keeps its place, and a parenthesized group holding a comment
in any other command is expanded one argument per line. Text touching a
group's `)`, as in `(a b)y`, is a separate argument to CMake and is
written as one, as gersemi does. A comment after
a trailing bracket comment takes its own line. A file CMake would reject although the
grammar accepts it, with a `\` before a line break outside quotes, an
argument starting where a bracket comment ends (`#[[c]]b`), or a bracket argument
touching a `)`, a quoted or bracket argument before it, or an argument
after it (`(a)[[b]]`, `[[a]]b`), is declined.

**Bash** targets Prettier's shell plugin (prettier-plugin-sh, which prints
with shfmt's printer). Statements go on their own lines, a compound command
stays on one line only when written that way with one statement per body,
a chain broken over lines puts `&&`, `||` or `|` at the start of the next
line, and a space follows redirection operators. Heredoc bodies are kept
byte for byte. Backticks become `$( )` unless they contain `\\`, `\$`,
`` \` `` or `\"`, whose meaning would change. Arguments written after a
redirection stay after it, where Prettier sometimes moves them. An
array written over several lines keeps its line breaks and comments, and
an `if`, `while` or `until` condition that continues past its keyword's
line gets its own lines, as shfmt prints them. A condition holding a
heredoc puts `then` or `do` on the line after the heredoc; Prettier writes
`<< EOF; then`, which the grammar misreads. A `{name}` or number touching
a redirection (`exec {fd}>file`, `exec 0<&5`) stays attached to it as its
file descriptor. A comment after a loop or
`if` header moves after its `do` or `then`, and a body written on that
line moves to the next. A comment between a function's name and its body
moves after the `{`.

Some statements are kept exactly as written because the grammar reads
them differently from bash: a heredoc whose body has a line starting
with `\`, a `#` right after a redirection, a command the grammar runs on
across lines (for example `'{'` and `'}'` strings on separate lines, read
as one brace expansion), an assignment touching the word after it
(``a="x"`cmd` ``), and a string whose backticks the grammar pairs wrongly
(`` "`a` `b`" ``), a comment inside `[[ ]]` or `(( ))`, and text the
grammar skips, such as a `-` argument before a redirection
(`cmd - <<EOF`) or an escaped blank between expansions (`$a\ $b`), a
line continuation inside a word (`echo a\` then `b`, which bash reads as
`echo ab`), and a redirection the grammar joins to a statement across a
line break (`> out` on a line of its own, or followed by commands on the
next lines). Only
that statement is kept, or the whole `case` command when it is in a
`case` item; the rest of the file is formatted. A file is declined
when the grammar skips text outside a statement
(`cmd a \ ` before a line break), reads a comment right after a line
continuation (`a\` then `#b`, which bash reads as `a#b`), ends a
heredoc at a marker line with more text on it (`EOF&&`), or reads a
`` ` `` substitution inside another, which bash would close at the inner
`` ` ``. The grammar can do this after a quoted substitution such as
``X="`a`"``, when a later string holds a `` ` `` followed by a blank
(``Y="x ` c`"``). A file is also left unchanged when the grammar reads
part of a compound command (`if` and `then`, or an `else`) into a
statement kept as written, and formatting the result again would change
it, or when it reads a reserved word such as `fi`, `then`, `{`, `for` or
`function` as the name of a command at the start of its line, which bash
rejects (as after a `[ ]` test it runs on past its `]`, or empty
assignments it runs on past a comment).
