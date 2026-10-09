# moldy

@AGENTS.md

Moldy is a tree-sitter based formatter for C, C++, Rust, and Python. The
C/C++ formatter targets funky's formatting policy; corpus tests compare
checked-in expected output. Rust and Python have separate fixtures and
compatibility limits. See [documentation](README.md#documentation).

## Task tracking

Tasks are tracked in the fleet database under project `moldy`. Follow the
coordinator's assignment and report progress through the coordinator. There
is no repository-local task database or `.todo-sqlite-cli` marker.

## Module structure

| File | Responsibility |
| --- | --- |
| `src/main.rs` | clap CLI, config loading, shared ignore merge, recursive discovery, UTF-8 reads, output/check/write modes. |
| `src/config.rs` | Formatting schema and defaults; C/C++ policies aim to match funky. |
| `src/error.rs` | Public error types. |
| `src/formatter/mod.rs` | Public `Formatter` trait, `format_source` dispatch, `dump_tree`. |
| `src/formatter/c_cpp.rs` | Implemented C/C++ structural emitter, expression leaf walking, comment and enum alignment. |
| `src/formatter/rust.rs` | Recursive emitter with pairwise spacing and structural handlers for blocks, lists, fields, match arms, and where clauses. |
| `src/formatter/python.rs` | Recursive emitter with statement/block indentation and width-based bracketed-list wrapping. |
| `src/formatter/output.rs` | Shared output operations. |
| `src/presets.rs` | Embedded preset lookup. |
| `src/toolchain.rs` | Discover nearest shared `toolchain.toml` from the working directory; deserialize only ignore paths. |

## Formatter invariants

Tree-sitter stores tokens and source offsets, not formatting whitespace. Emit
whitespace under `Config` and use node byte offsets to slice the matching source.
Keep each parser tree paired with the original text it parsed. Preserve UTF-8
boundaries. Use ancestor/structural context for indentation and punctuation.
C preprocessor directives, Rust attributes/macros, and Python strings/f-strings
have opaque emission paths. Do not introduce analysis dead-code blanking into
formatting: text in inactive preprocessor branches must be retained.

Formatters currently continue over syntax errors. Unknown C/C++ nodes emit
source text; the entire file is not guaranteed to pass through unchanged.
Verify malformed fixtures when changing emission.

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

## Substrate

`lang-parsing-substrate` 0.11.2 owns registry metadata, grammar access, and glob
matching. Disable default features and enable only the four implemented
formatters. `language_info_for_file` drives dispatch; `language_for_key`
provides grammars; `language_for_file` serves tree dumping; `PathIgnore` serves
path matching. Recursive discovery uses `is_source_extension` and skips `.h`.
Explicit `.h` files use C, with no content sniffing. Do not duplicate extension
lists in production code.

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
