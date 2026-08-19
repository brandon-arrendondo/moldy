use serde::{Deserialize, Deserializer};
use std::path::Path;

use crate::error::MoldyError;

// ── Top-level config ─────────────────────────────────────────────────────────

/// The full `moldy.toml` schema, one section per formatting concern. Every
/// field has a `Default` matching moldy's from-scratch behavior; a preset
/// (`presets/*.toml`) overrides a subset to match an existing tool's output.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Indentation style and width, and whether `switch`/`case` bodies and
    /// goto labels get their own indent level.
    pub indent: IndentConfig,
    /// Brace placement (K&R/Allman/Stroustrup) and brace-insertion rules.
    pub braces: BraceConfig,
    /// Whitespace around parens, operators, casts, and comment alignment.
    pub spacing: SpacingConfig,
    /// Line-ending style and blank-line handling.
    pub newlines: NewlineConfig,
    /// Preprocessor directive indentation (`#if`/`#endif`).
    pub preprocessor: PreprocConfig,
    /// Comment normalization rules.
    pub comments: CommentConfig,
    /// Glob patterns for files/paths moldy should skip.
    pub ignore: IgnoreConfig,
    /// Rust-specific formatting knobs.
    pub rust: RustConfig,
    /// Python-specific formatting knobs.
    pub python: PythonConfig,
}

// ── Python ───────────────────────────────────────────────────────────────────

/// Knobs specific to `src/formatter/python.rs` (currently a stub — see the
/// module docs there). Default matches PEP8/flake8 (`max_width = 79`); the
/// `black` preset (`presets/python/black.toml`) moves it to 88 to match
/// `ruff format`'s default.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PythonConfig {
    /// Column budget for line-wrapping decisions.
    pub max_width: u32,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self { max_width: 79 }
    }
}

// ── Rust ─────────────────────────────────────────────────────────────────────

/// Knobs specific to `src/formatter/rust.rs`. Everything here defaults to
/// moldy's original from-scratch behavior; set `width_based_wrapping` and
/// `collapse_field_lists` to move toward rustfmt's actual output so moldy can
/// replace `cargo fmt` outright instead of just running alongside it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RustConfig {
    /// Column budget used by `width_based_wrapping` and `collapse_field_lists`.
    /// Matches rustfmt's default `max_width`.
    pub max_width: u32,
    /// Break bracketed comma lists (call args, tuples, arrays, generics, ...)
    /// onto one item per line based on whether the single-line rendering
    /// fits within `max_width`, instead of moldy's default of preserving
    /// whatever single/multi-line choice the source already made.
    pub width_based_wrapping: bool,
    /// Collapse struct/enum field lists onto a single line when they have no
    /// comments/attributes and fit within `max_width`, instead of moldy's
    /// default of always exploding them one field per line.
    pub collapse_field_lists: bool,
}

impl Default for RustConfig {
    fn default() -> Self {
        Self {
            max_width: 100,
            width_based_wrapping: false,
            collapse_field_lists: false,
        }
    }
}

// ── Preprocessor ─────────────────────────────────────────────────────────────

/// Preprocessor directive (`#if`/`#endif`) indentation.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PreprocConfig {
    /// Indent preprocessor directives at all (`#  if` vs. always column 0).
    pub pp_indent: bool,
    /// When `pp_indent` is set, indent to match the surrounding code's
    /// nesting level rather than the `#if`/`#endif` nesting alone.
    pub pp_indent_at_level: bool,
    /// Minimum spaces between `#endif` and its trailing `// FOO` comment.
    pub endif_comment_space: u32,
}

impl Default for PreprocConfig {
    fn default() -> Self {
        Self {
            pp_indent: false,
            pp_indent_at_level: true,
            endif_comment_space: 1,
        }
    }
}

// ── Comments ─────────────────────────────────────────────────────────────────

/// Comment normalization rules.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct CommentConfig {
    /// Normalize a block comment's closing `*/` line to a consistent style.
    pub normalize_block_comment_closing: bool,
}

// ── Ignore ───────────────────────────────────────────────────────────────────

/// Files/paths moldy should skip.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct IgnoreConfig {
    /// Glob patterns matched against a candidate file's path.
    pub patterns: Vec<String>,
}

impl Config {
    /// Read and parse `path` as a `moldy.toml` config file.
    pub fn load(path: &Path) -> Result<Self, MoldyError> {
        let text = std::fs::read_to_string(path).map_err(|e| MoldyError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::parse(&text, &path.display().to_string())
    }

    /// Parse `text` as TOML, tagging any error with `label` (a file path or
    /// other source description) for the caller's error message.
    pub fn parse(text: &str, label: &str) -> Result<Self, MoldyError> {
        toml::from_str(text).map_err(|e| MoldyError::Config {
            path: label.to_string(),
            source: e,
        })
    }

    /// The literal line-ending string for `newlines.style`, resolving
    /// `NewlineStyle::Native` to the current platform's convention.
    pub fn newline_str(&self) -> &'static str {
        match self.newlines.style {
            NewlineStyle::Lf => "\n",
            NewlineStyle::Crlf => "\r\n",
            NewlineStyle::Native => {
                if cfg!(windows) {
                    "\r\n"
                } else {
                    "\n"
                }
            }
        }
    }

    /// One indent level as a literal string, per `indent.style`/`indent.width`.
    pub fn indent_str(&self) -> String {
        match self.indent.style {
            IndentStyle::Spaces => " ".repeat(self.indent.width as usize),
            IndentStyle::Tabs => "\t".to_string(),
        }
    }
}

// ── Indent ───────────────────────────────────────────────────────────────────

/// Indentation style, width, and which extra constructs get their own level.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct IndentConfig {
    /// Spaces or tabs.
    pub style: IndentStyle,
    /// Columns per indent level (or, for `Tabs`, the display width used to
    /// size other alignment decisions).
    pub width: u8,
    /// Indent `case`/`default` labels one level inside their `switch`.
    pub indent_switch_case: bool,
    /// Indent `goto` target labels rather than pinning them to column 0.
    pub indent_goto_labels: bool,
}

impl Default for IndentConfig {
    fn default() -> Self {
        Self {
            style: IndentStyle::Spaces,
            width: 4,
            indent_switch_case: true,
            indent_goto_labels: false,
        }
    }
}

/// Whitespace character used for indentation.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IndentStyle {
    /// Indent with `IndentConfig::width` space characters per level.
    Spaces,
    /// Indent with one tab character per level.
    Tabs,
}

// ── Braces ───────────────────────────────────────────────────────────────────

/// Brace placement and brace-insertion rules.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BraceConfig {
    /// K&R, Allman, or Stroustrup brace placement.
    pub style: BraceStyle,
    /// Put `else`/`else if` on the same line as the preceding `}`.
    pub cuddle_else: bool,
    /// Put `catch` on the same line as the preceding `}`.
    pub cuddle_catch: bool,
    /// Collapse an empty `{ }` body onto one line.
    pub collapse_empty_body: bool,
    /// Force a large brace-initializer (array/struct literal) onto multiple
    /// lines even if it would otherwise fit on one.
    pub expand_large_initializers: bool,
    /// Put a function's opening brace on its own line (Allman-for-functions,
    /// independent of `style`).
    pub fn_brace_newline: bool,
    /// Brace placement for `extern "C" { ... }` blocks specifically.
    pub extern_c_brace: ExternCBrace,
    /// Add braces to a brace-less `if` body.
    pub add_braces_to_if: bool,
    /// Add braces to a brace-less `while` body.
    pub add_braces_to_while: bool,
    /// Add braces to a brace-less `for` body.
    pub add_braces_to_for: bool,
}

impl Default for BraceConfig {
    fn default() -> Self {
        Self {
            style: BraceStyle::Kr,
            cuddle_else: false,
            cuddle_catch: false,
            collapse_empty_body: true,
            expand_large_initializers: false,
            fn_brace_newline: true,
            extern_c_brace: ExternCBrace::ForceSameLine,
            add_braces_to_if: true,
            add_braces_to_while: true,
            add_braces_to_for: true,
        }
    }
}

/// Named brace-placement convention.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BraceStyle {
    /// Opening brace on the same line as its statement/declaration.
    Kr,
    /// Opening brace on its own line, indented to match the statement.
    Allman,
    /// Like Allman for functions; K&R for control-flow bodies.
    Stroustrup,
}

/// Brace placement for `extern "C" { ... }` blocks.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExternCBrace {
    /// Always force the opening brace onto the `extern "C"` line.
    #[default]
    ForceSameLine,
    /// Leave the source's existing brace placement alone.
    Preserve,
}

// ── SpaceOption ───────────────────────────────────────────────────────────────

/// A three-way spacing knob, for a whitespace rule that isn't simply
/// on/off: add it, remove it, or leave whatever the source already has.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SpaceOption {
    /// Always insert the space.
    Add,
    /// Always remove the space.
    Remove,
    /// Leave the source's existing spacing as-is.
    #[default]
    Preserve,
}

impl<'de> Deserialize<'de> for SpaceOption {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{self, Visitor};
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = SpaceOption;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, r#""add", "remove", "preserve", true, or false"#)
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<SpaceOption, E> {
                Ok(if v {
                    SpaceOption::Add
                } else {
                    SpaceOption::Remove
                })
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<SpaceOption, E> {
                match v {
                    "add" => Ok(SpaceOption::Add),
                    "remove" => Ok(SpaceOption::Remove),
                    "preserve" => Ok(SpaceOption::Preserve),
                    _ => Err(E::unknown_variant(v, &["add", "remove", "preserve"])),
                }
            }
        }
        d.deserialize_any(V)
    }
}

// ── Spacing ──────────────────────────────────────────────────────────────────

/// Whitespace around parens/operators/casts, pointer alignment, and
/// trailing-comment alignment.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SpacingConfig {
    /// Space between a function/macro name and its call `(`.
    pub space_before_call_paren: bool,
    /// Space between a control-flow keyword (`if`, `while`, ...) and its `(`.
    pub space_before_keyword_paren: bool,
    /// Space after a comma in a parameter/argument list.
    pub space_after_comma: bool,
    /// Space around binary operators (`+`, `==`, `&&`, ...).
    pub space_around_binary_ops: bool,
    /// Spacing just inside `( )`.
    pub space_inside_parens: SpaceOption,
    /// Spacing just inside `[ ]`.
    pub space_inside_brackets: SpaceOption,
    /// Spacing after a C-style cast, e.g. `(int) x` vs `(int)x`.
    pub space_after_cast: SpaceOption,
    /// Which side of a pointer declarator (`int *p`, `int* p`, `int * p`)
    /// the `*`/`&` binds to visually.
    pub pointer_align: PointerAlign,
    /// Spacing just inside `< >` in template/generic argument lists.
    pub space_inside_angle_brackets: bool,
    /// Minimum column span a trailing `//` comment is aligned to within a
    /// group of consecutive commented lines.
    pub align_right_cmt_span: usize,
    /// Minimum gap between code and an aligned trailing comment.
    pub align_right_cmt_gap: usize,
    /// Whether trailing-comment alignment resets per contiguous group or
    /// spans the whole file.
    pub align_right_cmt_style: AlignCmtStyle,
    /// Column span for aligning `=` in a run of enum-value assignments.
    pub align_enum_equ_span: usize,
    /// Column span for aligning consecutive doxygen `@param`/`@brief`-style
    /// comment tags.
    pub align_doxygen_cmt_span: usize,
    /// Round alignment columns to the nearest tab stop rather than an exact
    /// column.
    pub align_on_tabstop: bool,
}

impl Default for SpacingConfig {
    fn default() -> Self {
        Self {
            space_before_call_paren: false,
            space_before_keyword_paren: true,
            space_after_comma: true,
            space_around_binary_ops: true,
            space_inside_parens: SpaceOption::default(),
            space_inside_brackets: SpaceOption::default(),
            space_after_cast: SpaceOption::default(),
            pointer_align: PointerAlign::Name,
            space_inside_angle_brackets: false,
            align_right_cmt_span: 3,
            align_right_cmt_gap: 1,
            align_right_cmt_style: AlignCmtStyle::Groups,
            align_enum_equ_span: 1,
            align_doxygen_cmt_span: 1,
            align_on_tabstop: true,
        }
    }
}

/// Which side of `*`/`&` in a pointer/reference declarator gets the space.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PointerAlign {
    /// `int* p` — bind to the type.
    Type,
    /// `int *p` — bind to the name.
    #[default]
    Name,
    /// `int * p` — space on both sides.
    Middle,
}

/// Scope over which trailing-comment alignment is computed.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AlignCmtStyle {
    /// Align within each contiguous run of commented lines independently.
    #[default]
    Groups,
    /// Align every trailing comment in the file to one shared column.
    All,
}

// ── Newlines ─────────────────────────────────────────────────────────────────

/// Line-ending style and blank-line handling.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct NewlineConfig {
    /// LF, CRLF, or the current platform's native ending.
    pub style: NewlineStyle,
    /// Maximum consecutive blank lines to keep; extras are collapsed.
    pub max_blank_lines: u8,
    /// Ensure the file ends with exactly one newline.
    pub final_newline: bool,
    /// Force a blank line after a block of variable declarations.
    pub blank_line_after_var_decl_block: bool,
    /// Force a blank line immediately after an opening `{`.
    pub blank_line_after_open_brace: bool,
    /// Merge a run of consecutive `//` line comments into one.
    pub merge_line_comment: bool,
    /// Force a newline before a cuddled `} else`/`} catch` even when
    /// `braces.cuddle_else`/`cuddle_catch` would otherwise keep it cuddled.
    pub nl_brace_else: bool,
}

impl Default for NewlineConfig {
    fn default() -> Self {
        Self {
            style: NewlineStyle::Lf,
            max_blank_lines: 2,
            final_newline: true,
            blank_line_after_var_decl_block: true,
            blank_line_after_open_brace: false,
            merge_line_comment: false,
            nl_brace_else: true,
        }
    }
}

/// Line-ending convention.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NewlineStyle {
    /// Always `\n`.
    Lf,
    /// Always `\r\n`.
    Crlf,
    /// `\r\n` on Windows, `\n` elsewhere — see [`Config::newline_str`].
    Native,
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_valid() {
        let cfg = Config::default();
        assert_eq!(cfg.indent.style, IndentStyle::Spaces);
        assert_eq!(cfg.indent.width, 4);
        assert_eq!(cfg.newline_str(), "\n");
    }

    #[test]
    fn parse_full_config() {
        let toml = r#"
[indent]
style = "spaces"
width = 4
indent_switch_case = true
indent_goto_labels = false

[braces]
style = "kr"
cuddle_else = false
cuddle_catch = false
collapse_empty_body = true
fn_brace_newline = true
extern_c_brace = "force_same_line"
add_braces_to_if    = true
add_braces_to_while = true
add_braces_to_for   = true

[spacing]
space_before_call_paren    = false
space_before_keyword_paren = true
space_after_comma          = true
space_around_binary_ops    = true
pointer_align              = "name"
align_right_cmt_span       = 3
align_right_cmt_gap        = 1
align_right_cmt_style      = "groups"
align_enum_equ_span        = 1
align_doxygen_cmt_span     = 1
align_on_tabstop           = true

[newlines]
style           = "lf"
max_blank_lines = 2
final_newline   = true
blank_line_after_var_decl_block = true
blank_line_after_open_brace     = false
merge_line_comment              = false
nl_brace_else                   = true

[preprocessor]
pp_indent           = false
pp_indent_at_level  = true
endif_comment_space = 1

[comments]
normalize_block_comment_closing = false

[rust]
max_width = 100
width_based_wrapping = true
collapse_field_lists = true

[python]
max_width = 88
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.indent.width, 4);
        assert_eq!(cfg.braces.style, BraceStyle::Kr);
        assert_eq!(cfg.rust.max_width, 100);
        assert!(cfg.rust.width_based_wrapping);
        assert!(cfg.rust.collapse_field_lists);
        assert_eq!(cfg.python.max_width, 88);
        assert_eq!(cfg.newlines.max_blank_lines, 2);
    }
}
