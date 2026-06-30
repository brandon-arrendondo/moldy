# moldy

A multi-language code formatter built on tree-sitter. The spiritual successor to [funky](../funky), with identical C/C++ formatting output and a foundation for additional languages.

## Features

- **Full C/C++ parity with funky** — identical output for every corpus file
- K&R, Allman, and Stroustrup brace styles
- Pointer alignment (`type`, `name`, `middle`)
- Trailing comment column-alignment (`//`, `/**<` Doxygen)
- Enum `=` sign alignment
- Configurable blank-line rules
- `--check` mode for CI / pre-commit hooks
- `--recursive` directory walk with glob-based ignore patterns
- `--dump-tree` to inspect the tree-sitter CST (replaces funky's `--dump-tokens`)
- Unicode-safe — full-source pass, no silent truncation

## Installation

```sh
cargo install moldy-fmt
```

Or build a release binary directly:

```sh
cargo build --release
# binary at target/release/moldy
```

## Usage

```
moldy [OPTIONS] <FILES>...
```

| Argument / Option | Description |
|---|---|
| `<FILES>...` | Files or directories to format. Use `-` for stdin. |
| `-i`, `--in-place` | Edit files in place. |
| `--check` | Exit 1 if any file would change; no writes. |
| `-r`, `--recursive` | Recurse into directories. |
| `-c`, `--config <FILE>` | Explicit config file (default: `moldy.toml` in cwd). |
| `--dump-tree` | Print the tree-sitter CST and exit (debug). |
| `-h`, `--help` | Print help. |
| `-V`, `--version` | Print version. |

`--check` and `--in-place` are mutually exclusive.

### Examples

```sh
# Format a single file to stdout
moldy src/foo.c

# Edit in place
moldy -i src/foo.c src/bar.h

# Check a whole tree (CI)
moldy --check -r src/

# Pipe through stdin
cat ugly.c | moldy - > pretty.c

# Inspect the tree-sitter CST
moldy --dump-tree src/foo.c
```

## Configuration

Place a `moldy.toml` in your project root (or pass `--config`). `funky.toml` is also accepted — the two formats are identical for C/C++. All keys are optional; defaults are shown below.

```toml
[indent]
style               = "spaces"  # "spaces" | "tabs"
width               = 4         # spaces per level (ignored for tabs)
indent_switch_case  = true      # indent case/default labels inside switch
indent_goto_labels  = false     # false: goto labels at column 0

[braces]
style               = "kr"      # "kr" | "allman" | "stroustrup"
cuddle_else         = false     # false: } \n else {   true: } else {
cuddle_catch        = false     # false: } \n catch (  true: } catch (
collapse_empty_body = true      # while (x) { } → while (x) {}
fn_brace_newline    = true      # function-def { always on its own line
extern_c_brace      = "force_same_line"  # "force_same_line" | "preserve"
nl_brace_else       = true      # true: newline before else/else-if
add_braces_to_if    = true      # add { } around braceless if bodies
add_braces_to_while = true      # add { } around braceless while bodies
add_braces_to_for   = true      # add { } around braceless for bodies

[spacing]
space_before_call_paren     = false      # foo( vs foo (
space_before_keyword_paren  = true       # if ( vs if(
space_after_comma           = true
space_around_binary_ops     = true
space_inside_parens         = "preserve" # "preserve" | "add" | "remove"
space_inside_brackets       = "preserve" # "preserve" | "add" | "remove"
space_after_cast            = "preserve" # "preserve" | "add" | "remove"
pointer_align               = "name"     # "type" | "name" | "middle"
space_inside_angle_brackets = false      # vector<int> vs vector< int >
align_right_cmt_span        = 3          # 0=off; column-align trailing // comments
align_right_cmt_gap         = 1          # minimum spaces before aligned comment
align_right_cmt_style       = "groups"   # "groups" | "all"
align_enum_equ_span         = 1          # 0=off; align enum = signs
align_doxygen_cmt_span      = 1          # 0=off; column-align /**< comments
align_on_tabstop            = true       # snap alignment to indent-width multiples

[newlines]
style                           = "lf"   # "lf" | "crlf" | "native"
max_blank_lines                 = 2
final_newline                   = true
blank_line_after_var_decl_block = true
blank_line_after_open_brace     = false
merge_line_comment              = false
nl_brace_else                   = true

[preprocessor]
pp_indent           = false
endif_comment_space = 1

[comments]
normalize_block_comment_closing = false

[ignore]
patterns = ["vendor/**", "third_party/**", "*.pb.h"]
```

## Pre-commit hook

```yaml
repos:
  - repo: https://github.com/brandon-arrendondo/moldy
    rev: v0.1.0
    hooks:
      - id: moldy  # runs moldy --in-place
```

## Relationship to funky

moldy is funky's successor. The config format is intentionally identical — `funky.toml` and `moldy.toml` are interchangeable for C/C++. Once moldy reaches full parity across all funky corpus files, funky will be deprecated.

Key differences:
- moldy uses tree-sitter (CST-based) instead of a hand-rolled lexer
- `--dump-tree` replaces `--dump-tokens`
- Architecture supports additional languages beyond C/C++

## License

MIT — see [LICENSE](LICENSE).
