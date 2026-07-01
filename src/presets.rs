//! Built-in `Config` presets for common project styles, embedded at compile
//! time from `presets/*.toml` so `moldy` works as a single binary with no
//! extra files to ship alongside it.

use crate::config::Config;
use crate::error::MoldyError;

struct Preset {
    name: &'static str,
    toml: &'static str,
}

const PRESETS: &[Preset] = &[
    Preset {
        name: "linux-kernel",
        toml: include_str!("../presets/linux-kernel.toml"),
    },
    Preset {
        name: "riot",
        toml: include_str!("../presets/riot.toml"),
    },
    Preset {
        name: "rustfmt-compat",
        toml: include_str!("../presets/rustfmt-compat.toml"),
    },
];

/// Names of all built-in presets, for `--help` text and error messages.
pub fn names() -> Vec<&'static str> {
    PRESETS.iter().map(|p| p.name).collect()
}

/// Look up a built-in preset by name and parse it into a `Config`.
pub fn load(name: &str) -> Result<Config, MoldyError> {
    let preset = PRESETS.iter().find(|p| p.name == name).ok_or_else(|| {
        MoldyError::UnsupportedLanguage(format!(
            "unknown preset '{name}' (available: {})",
            names().join(", ")
        ))
    })?;
    Config::parse(preset.toml, &format!("preset:{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_parse() {
        for name in names() {
            load(name).unwrap_or_else(|e| panic!("preset '{name}' failed to parse: {e}"));
        }
    }

    #[test]
    fn unknown_preset_errors() {
        assert!(load("does-not-exist").is_err());
    }

    #[test]
    fn rustfmt_compat_enables_rust_knobs() {
        let cfg = load("rustfmt-compat").unwrap();
        assert!(cfg.rust.width_based_wrapping);
        assert!(cfg.rust.collapse_field_lists);
        assert_eq!(cfg.rust.max_width, 100);
        assert_eq!(cfg.indent.width, 4);
        assert_eq!(cfg.newlines.max_blank_lines, 1);
    }
}
