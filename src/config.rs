use serde::{Deserialize, Deserializer};
use std::path::Path;

use crate::error::MoldyError;

// ── Top-level config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub indent: IndentConfig,
    pub braces: BraceConfig,
    pub spacing: SpacingConfig,
    pub newlines: NewlineConfig,
    pub preprocessor: PreprocConfig,
    pub comments: CommentConfig,
    pub ignore: IgnoreConfig,
}

// ── Preprocessor ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PreprocConfig {
    pub pp_indent: bool,
    pub pp_indent_at_level: bool,
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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct CommentConfig {
    pub normalize_block_comment_closing: bool,
}

// ── Ignore ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct IgnoreConfig {
    pub patterns: Vec<String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, MoldyError> {
        let text = std::fs::read_to_string(path).map_err(|e| MoldyError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::parse(&text, &path.display().to_string())
    }

    pub fn parse(text: &str, label: &str) -> Result<Self, MoldyError> {
        toml::from_str(text).map_err(|e| MoldyError::Config {
            path: label.to_string(),
            source: e,
        })
    }

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

    pub fn indent_str(&self) -> String {
        match self.indent.style {
            IndentStyle::Spaces => " ".repeat(self.indent.width as usize),
            IndentStyle::Tabs => "\t".to_string(),
        }
    }
}

// ── Indent ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct IndentConfig {
    pub style: IndentStyle,
    pub width: u8,
    pub indent_switch_case: bool,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IndentStyle {
    Spaces,
    Tabs,
}

// ── Braces ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BraceConfig {
    pub style: BraceStyle,
    pub cuddle_else: bool,
    pub cuddle_catch: bool,
    pub collapse_empty_body: bool,
    pub expand_large_initializers: bool,
    pub fn_brace_newline: bool,
    pub extern_c_brace: ExternCBrace,
    pub add_braces_to_if: bool,
    pub add_braces_to_while: bool,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BraceStyle {
    Kr,
    Allman,
    Stroustrup,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExternCBrace {
    #[default]
    ForceSameLine,
    Preserve,
}

// ── SpaceOption ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SpaceOption {
    Add,
    Remove,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SpacingConfig {
    pub space_before_call_paren: bool,
    pub space_before_keyword_paren: bool,
    pub space_after_comma: bool,
    pub space_around_binary_ops: bool,
    pub space_inside_parens: SpaceOption,
    pub space_inside_brackets: SpaceOption,
    pub space_after_cast: SpaceOption,
    pub pointer_align: PointerAlign,
    pub space_inside_angle_brackets: bool,
    pub align_right_cmt_span: usize,
    pub align_right_cmt_gap: usize,
    pub align_right_cmt_style: AlignCmtStyle,
    pub align_enum_equ_span: usize,
    pub align_doxygen_cmt_span: usize,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PointerAlign {
    Type,
    #[default]
    Name,
    Middle,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AlignCmtStyle {
    #[default]
    Groups,
    All,
}

// ── Newlines ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct NewlineConfig {
    pub style: NewlineStyle,
    pub max_blank_lines: u8,
    pub final_newline: bool,
    pub blank_line_after_var_decl_block: bool,
    pub blank_line_after_open_brace: bool,
    pub merge_line_comment: bool,
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NewlineStyle {
    Lf,
    Crlf,
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
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.indent.width, 4);
        assert_eq!(cfg.braces.style, BraceStyle::Kr);
        assert_eq!(cfg.newlines.max_blank_lines, 2);
    }
}
