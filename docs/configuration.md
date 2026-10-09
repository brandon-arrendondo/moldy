# Configuration

All keys are optional. Config sections reject unknown fields. Defaults come from `src/config.rs`; the example below includes every field.

Resolution order is `--preset`, an explicit `--config`, `moldy.toml` in the current working directory, then built-in defaults. Presets and explicit config cannot be combined. Moldy does not search parent directories for `moldy.toml`, automatically load `funky.toml`, or merge a preset with a local config file.

## Built-in presets

| Name | Target | Purpose |
| --- | --- | --- |
| `linux-kernel` | C/C++ | Kernel-style configuration. |
| `riot` | C/C++ | RIOT-style configuration. |
| `rustfmt-compat` | Rust | Width 100, width-based wrapping and short field lists. |
| `pep8` | Python | Width 79. |
| `black` | Python | Width 88. |

Presets select configuration, not a language: extensions still control dispatch. See [usage](usage.md) for compatibility limits.

## Complete default configuration

```toml
[indent]
style = "spaces"                 # spaces | tabs
width = 4
indent_switch_case = true
indent_goto_labels = false

[braces]
style = "kr"                     # kr | allman | stroustrup
cuddle_else = false
cuddle_catch = false
collapse_empty_body = true
expand_large_initializers = false
fn_brace_newline = true
extern_c_brace = "force_same_line" # force_same_line | preserve
add_braces_to_if = true
add_braces_to_while = true
add_braces_to_for = true

[spacing]
space_before_call_paren = false
space_before_keyword_paren = true
space_after_comma = true
space_around_binary_ops = true
space_inside_parens = "preserve"   # preserve | add | remove
space_inside_brackets = "preserve"
space_after_cast = "preserve"
pointer_align = "name"            # type | name | middle
space_inside_angle_brackets = false
align_right_cmt_span = 3
align_right_cmt_gap = 1
align_right_cmt_style = "groups"  # groups | all
align_enum_equ_span = 1
align_doxygen_cmt_span = 1
align_on_tabstop = true

[newlines]
style = "lf"                     # lf | crlf | native
max_blank_lines = 2
final_newline = true
blank_line_after_var_decl_block = true
blank_line_after_open_brace = false
merge_line_comment = false
nl_brace_else = true

[preprocessor]
pp_indent = false
pp_indent_at_level = true
endif_comment_space = 1

[comments]
normalize_block_comment_closing = false

[ignore]
patterns = []

[rust]
max_width = 100
width_based_wrapping = false
collapse_field_lists = false

[python]
max_width = 79
```

`nl_brace_else` belongs to `[newlines]`, not `[braces]`. Sections express formatter-specific policies; not every field has an effect in every language. For example, brace and preprocessor policies are primarily C/C++ concerns.

## Shared ignore configuration

Moldy walks up from the current working directory to the filesystem root and loads the first `toolchain.toml` it finds:

```toml
[ignore]
paths = ["vendor/**", "generated/**"]
```

Those patterns are appended to `[ignore].patterns` from Moldy's config or preset. Other shared sections, including language-specific settings, are ignored. Invalid TOML or invalid ignore patterns fail the command.

Matching uses the substrate's `PathIgnore` against the candidate path, then against its bare filename. Thus `*.pb.h` works at any depth. Paths are not rebased to the directory containing `toolchain.toml`; run from the project root and use relative input paths for patterns such as `vendor/**`. Ignored directories are not pruned during traversal: their files are filtered after discovery.
