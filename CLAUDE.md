# moldy

@AGENTS.md

Moldy is a tree-sitter based formatter for C, C++, Rust, Python, JSON,
YAML, Markdown, CMake, Bash, and reStructuredText. The C/C++ formatter targets funky's
formatting policy; corpus tests compare checked-in expected output. The
other languages have separate fixtures and compatibility limits. See [documentation](README.md#documentation).

## Task tracking

Tasks are tracked in the fleet database under project `moldy`. Follow the
coordinator's assignment and report progress through the coordinator. There
is no repository-local task database or `.todo-sqlite-cli` marker.

## Module structure

| File                                 | Responsibility                                                                                                            |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| `src/main.rs`                        | clap CLI, config loading, shared ignore merge, recursive discovery, UTF-8 reads, output/check/write modes.                |
| `src/config.rs`                      | Formatting schema and defaults; C/C++ policies aim to match funky.                                                        |
| `src/error.rs`                       | Public error types.                                                                                                       |
| `src/formatter/mod.rs`               | Public `Formatter` trait, `format_source` dispatch, `dump_tree`.                                                          |
| `src/formatter/c_cpp.rs`             | Implemented C/C++ structural emitter, expression leaf walking, comment and enum alignment.                                |
| `src/formatter/rust.rs`              | Recursive emitter with pairwise spacing and structural handlers for blocks, lists, fields, match arms, and where clauses. |
| `src/formatter/python.rs`            | Recursive emitter with statement/block indentation and width-based bracketed-list wrapping.                               |
| `src/formatter/json.rs`              | Expand-all JSON/JSONC layout (`jq .`, `json.dumps(indent=2)`).                                                            |
| `src/formatter/yaml.rs`              | YAML layout targeting Prettier.                                                                                           |
| `src/formatter/markdown.rs`          | Markdown blocks and inline text targeting Prettier (`proseWrap: preserve`); checks its output's shape against the input.  |
| `src/formatter/markdown_emphasis.rs` | CommonMark emphasis pairing, used to trust and verify emphasis rewrites.                                                  |
| `src/formatter/markdown_check.rs`    | Independent CommonMark reading (pulldown-cmark) of input and output, for the Markdown check.                              |
| `src/formatter/cmake.rs`             | CMake layout targeting gersemi; `cmake_signatures.rs` holds Moldy's own command signature table.                          |
| `src/formatter/bash.rs`              | Bash layout targeting Prettier's shell plugin (shfmt printer).                                                            |
| `src/formatter/rst.rs`               | reStructuredText layout targeting docstrfmt; finds tables in the source and checks its output's shape against the input.  |
| `src/formatter/line_endings.rs`      | Maps source line endings onto formatted output.                                                                           |
| `src/formatter/output.rs`            | Shared output operations.                                                                                                 |
| `src/presets.rs`                     | Embedded preset lookup.                                                                                                   |
| `src/toolchain.rs`                   | Discover nearest shared `toolchain.toml` from the working directory; deserialize only ignore paths.                       |

## Formatter invariants

Tree-sitter stores tokens and source offsets, not formatting whitespace. Emit
whitespace under `Config` and use node byte offsets to slice the matching source.
Keep each parser tree paired with the original text it parsed. Preserve UTF-8
boundaries. Use ancestor/structural context for indentation and punctuation.
C preprocessor directives, Rust attributes/macros, and Python strings/f-strings
have opaque emission paths. Do not introduce analysis dead-code blanking into
formatting: text in inactive preprocessor branches must be retained.

The C, C++, Rust and Python formatters continue over syntax errors. Unknown
C/C++ nodes emit source text; the entire file is not guaranteed to pass
through unchanged. The JSON, YAML, Markdown, CMake, Bash and reStructuredText formatters
return `MoldyError::Syntax` for any syntax error (`root.has_error()`); the
CLI warns and leaves the file unchanged. They return `MoldyError::TooDeep`
for nesting past `MAX_TREE_DEPTH` (and Markdown and YAML past
`MAX_BLOCK_DEPTH`, estimated before parsing); keep both guards ahead of any
recursion. Markdown also returns
`MoldyError::Unverified` when its re-parsed output's shape differs from the
input's; keep that check passing on real documents, not only the corpus.
reStructuredText does the same, keeps what the grammar misreads as written
or refuses it, and returns `MoldyError::TooDeep` past its nested-document
limit. Where a grammar accepts input the
language rejects or reads it differently (Markdown flanking and lazy lists,
Bash `>#`), keep the affected text as written rather than reformat it.
Verify
malformed fixtures when changing emission.

## C/C++ parity

The related `../funky` checkout, when available, supplies the reference formatter
and config. Inspect its local contributor guidance before changing that repo.
Moldy's `tests/corpus_test.rs` compares its output to checked-in `.expected`
files and tests idempotency. `tests/acceptance_test.rs` covers positive and
malformed C/C++ fixtures. Do not claim general parity from the small local
corpus alone. Preserve output on pinned inputs when changing substrate use.

## Rust and Python

Rust and Python fixtures are in `tests/rust_corpus` and `tests/python_corpus`.
The `rustfmt-compat` preset enables width-based bracketed list wrapping and
collapsing short field lists (width 100); rustfmt's packed multiline list layout
is not implemented. Attributes and macros remain opaque.

Python defaults to width 79; `black` uses 88. Bracketed lists use a staged
single-line / hugged-body / one-item-per-line strategy. Strings are opaque,
comprehensions and subscripts lack width-based wrapping, and magic trailing
commas do not force multiline layout. Fixtures target Ruff formatting at
width 79 and flake8 checks; full tool parity is not established. When changing
Python fixtures or emission, run the external reference check with installed
`ruff` and `flake8`:

```sh
cargo test --test python_corpus_test -- --ignored
```

## JSON, YAML, Markdown, CMake, Bash and reStructuredText

Fixtures are in `tests/<language>_corpus` and run from
`tests/data_corpus_test.rs`. Reference formatters: JSON `json.dumps(indent=2)`,
YAML Prettier, Markdown Prettier `--prose-wrap preserve`, CMake gersemi, Bash
Prettier with prettier-plugin-sh, reStructuredText docstrfmt (layout only;
its rewrites that change meaning are deviations). What Moldy may change in prose is set by
[ADR-0006](docs/adr/0006-layout-not-authored-style.md): layout is normalized,
and an authored convention normalized by default (RST title adornments,
bullets, sentence spacing) needs a `preserve` option. Each reference test names its known
deviation fixtures. The default suite also re-parses CMake and Bash output and
compares arguments, tokens and comments; keep those checks passing. When
changing one of these formatters, run the ignored checks (see
[development](docs/development.md)) and compare against the reference on real
files, not only the corpus:

```sh
PRETTIER_PLUGIN_SH=/path/to/prettier-plugin-sh/lib/index.js \
  cargo test --test data_corpus_test -- --ignored
```

gersemi's signature table is MPL-2.0; do not copy it. Extend
`cmake_signatures.rs` from the CMake documentation.

## Substrate

`lang-parsing-substrate` 0.12.0 owns registry metadata, grammar access, and glob
matching. Disable default features and enable only the ten implemented
formatters. `language_info_for_file` drives dispatch; `language_for_key`
provides grammars, including `markdown_inline`; `language_for_file` serves tree
dumping; `PathIgnore` serves path matching; `markdown::ContentMap` strips
Markdown container prefixes. Recursive discovery uses `is_discoverable_file`,
which matches extensions and file names such as `CMakeLists.txt` and skips
`.h`. Explicit `.h` files use C, with no content sniffing. Do not duplicate
extension or file-name lists in production code. Logic another substrate
consumer could use (document structure, language facts) belongs in the
substrate; layout policy stays in Moldy.

The substrate provides no shared TOML loader in this version; keep the shared
ignore subset's discovery in `src/toolchain.rs`. Analysis APIs (metrics,
fingerprints, dead-code detection, call graphs) are not formatting operations.
See [architecture](docs/architecture.md) for consumer responsibilities.

## Adding a language

1. Enable its substrate feature in `Cargo.toml`, keeping defaults disabled.
2. Implement `src/formatter/<language>.rs` and add a dispatch arm.
3. Add corpus expectations, idempotency, malformed-input and discovery tests.
4. Document extensions, config, and compatibility limits.

## Running and verification

```sh
cargo build --locked
cargo test --locked
cargo run -- path/to/file.c
cargo run -- --in-place path/to/file.c
cargo run -- --check --recursive src/
cargo run -- --dump-tree path/to/file.c
pre-commit run --all-files
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked
```

Load `moldy.toml` only from the current directory, or use `--config`. Presets
bypass automatic config loading. Shared ignores come from the nearest
`toolchain.toml` found by walking up from the current directory. Stdin is not
implemented. See [development](docs/development.md) for required tool setup and
substrate migration validation.
