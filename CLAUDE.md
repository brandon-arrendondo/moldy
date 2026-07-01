# moldy

Multi-language code formatter built on tree-sitter. Uses
`lang-parsing-substrate` (`../lang_parsing_substrate`) for language detection
and tree-sitter grammar access.

**Immediate goal:** achieve full formatting parity with `funky` (`../funky`)
for C and C++, then expand to the other languages the substrate supports.
Once parity is confirmed, `funky` will be deprecated.

## Task tracking

This repo uses `todo-sqlite-cli` (DB resolved via `.todo-sqlite-cli` marker).

**Always check before coding:**
```
todo-sqlite-cli next        # the one task to work on now
todo-sqlite-cli list        # full active backlog
todo-sqlite-cli show <id>   # details for a specific task
```

**When working:**
```
todo-sqlite-cli start <id>  # before touching code
todo-sqlite-cli done <id>   # after committing
```

## Module structure

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point (clap 4). `--in-place`, `--check`, `--config`, `--recursive`, `--dump-tree`. Uses substrate's `is_source_extension` for directory walks. |
| `src/config.rs` | `Config` struct — **intentionally identical to funky's** so formatted output is comparable. Loaded from `moldy.toml`. |
| `src/error.rs` | `MoldyError` (thiserror). `Parse`, `Format`, `Config`, `Io`, `NotUtf8`, `UnsupportedLanguage` variants. |
| `src/formatter/mod.rs` | `Formatter` trait. `format_source(path, source, config)` dispatch. `dump_tree(path, source)` debug printer. |
| `src/formatter/c_cpp.rs` | C/C++ formatter. **Currently a stub** — parses with tree-sitter, returns source unchanged. Main implementation target. |
| `src/formatter/rust.rs` | Rust formatter. No funky reference exists (funky is C/C++ only), so this isn't a parity target — it's the proof that a new language costs one `format()` function plus a feature flag on the substrate. Single recursive `emit_node` with pairwise token spacing (`ws_before`) for the bulk of the grammar, plus dedicated handlers only for constructs that need real indentation/newline logic (blocks, item lists, struct/enum bodies, match arms, where-clauses, bracketed comma lists). Attributes and macro invocations are treated opaquely, mirroring this codebase's C-preprocessor invariant. Corpus tests in `tests/rust_corpus/` are self-referential (no funky to diff against) plus an idempotency check. |

## The C/C++ formatter — what needs to be built

`src/formatter/c_cpp.rs` contains a `format()` function that currently
returns the source unchanged. The goal is to make it produce output
**identical to funky** for every C/C++ file in `../funky/tests/corpus/`.

### Reference implementation

| File | What to look at |
|------|----------------|
| `../funky/src/formatter.rs` | Full 7 600-line token-stream formatter — the behaviour spec |
| `../funky/src/config.rs` | Config structs (identical to moldy's) |
| `../funky/tests/corpus/*.c` | Input files |
| `../funky/tests/corpus_test.rs` | How corpus tests work: run funky, compare to `.expected` |

### Recommended architecture (CST leaf-node walker)

tree-sitter does **not** store whitespace in the tree. The approach:

1. **Collect leaf nodes** — walk the tree recursively and collect every node
   where `child_count() == 0`. These correspond to tokens. Comments are leaf
   nodes with `kind() == "comment"`. Preprocessor lines (`#include`, `#define`,
   `#if`, etc.) appear as named non-leaf nodes — their children are the individual
   tokens; treat the entire subtree opaquely if you want to match funky's
   preprocessor-is-opaque invariant.

2. **Walk in order** — iterate leaf nodes left-to-right. Before emitting each
   leaf's text (`&source[node.start_byte()..node.end_byte()]`), emit
   formatter-controlled whitespace based on:
   - `prev_leaf.kind()` and `cur_leaf.kind()`
   - The ancestor chain of `cur_leaf` (for indentation depth and context)
   - `Config`

3. **Indentation depth** — count `compound_statement` ancestors (or maintain a
   depth counter as you descend). `switch_statement` and `case_statement` need
   special handling when `config.indent.indent_switch_case` is true.

4. **Context from ancestors** — `node.parent()` is cheap and replaces funky's
   `BraceCtx` stack for many decisions (e.g. "is this `{` opening a function
   body?" → check whether the grandparent is `function_definition`).

### Key tree-sitter invariants

- `node.is_named()` — false for punctuation/operators (anonymous nodes like `"{"`,
  `";"`, `","`) and true for syntactic constructs. Use `node.kind()` for both.
- `node.has_error()` on the root — presence of a syntax error; currently we pass
  the source through unchanged in that case.
- Whitespace between consecutive leaves: `next.start_byte() - prev.end_byte()` is
  the source gap. **Ignore it**; emit formatter whitespace instead.
- `node.start_position()` / `node.end_position()` return `tree_sitter::Point`
  with `{row, column}` if you need line/column for error messages.

### Parity test strategy

Copy (or symlink) `../funky/tests/corpus/` to `tests/corpus/`. Write
`tests/corpus_test.rs` that for each `.c`/`.cpp` file:

1. Runs `moldy::formatter::format_source(path, source, &Config::default())`.
2. Compares the result to the corresponding `.expected` file (same as funky's
   corpus tests).

Initially all tests fail (stub returns source unchanged). Drive them green one
construct at a time. Funky's corpus test infrastructure in
`../funky/tests/corpus_test.rs` is the template.

## Adding a new language formatter

1. Enable the language feature in `Cargo.toml`:
   `lang-parsing-substrate = { path = "...", features = ["lang-c", "lang-cpp", "lang-rust"] }`

2. Create `src/formatter/<lang>.rs` with a `pub fn format(source, config)` stub.

3. Add a match arm in `src/formatter/mod.rs`:
   ```rust
   "rust" => rust::format(source, config),
   ```

4. Add corpus tests in `tests/corpus/` and `tests/corpus_test.rs`.

## Running

```
cargo build
cargo test
cargo run -- path/to/file.c
cargo run -- --in-place path/to/file.c
cargo run -- --check path/to/file.c
cargo run -- --dump-tree path/to/file.c   # print tree-sitter CST
cargo run -- -r src/                      # recurse a directory
```

Config is loaded from `moldy.toml` in the current directory automatically.

## Relationship to funky

- Config structure is intentionally identical — same keys, same defaults.
- `--dump-tree` replaces funky's `--dump-tokens` (tree-sitter CST vs token stream).
- `funky.toml` and `moldy.toml` are interchangeable for C/C++ formatting.
- `funky` will be deprecated once the corpus tests pass.

## Substrate

`lang-parsing-substrate` (`../lang_parsing_substrate`) provides:
- `language_for_file(path)` → `Option<tree_sitter::Language>`
- `language_info_for_file(path)` → `Option<&'static LanguageInfo>` (has `.key` field)
- `is_source_extension(ext)` → bool
- `languages()` → `&[LanguageInfo]`
- Grammar re-exports: `lang_parsing_substrate::tree_sitter_c::LANGUAGE`, etc.
