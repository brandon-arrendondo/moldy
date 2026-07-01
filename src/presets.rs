//! Built-in `Config` presets for common project styles, embedded at compile
//! time from `presets/<language>/*.toml` so `moldy` works as a single binary
//! with no extra files to ship alongside it. Presets are organized on disk
//! by the language they target, mirroring `src/formatter/`.

use crate::config::Config;
use crate::error::MoldyError;

struct Preset {
    name: &'static str,
    /// Directory under `presets/` this preset lives in, for docs/listing —
    /// not part of the `--preset` lookup key, which stays flat.
    language: &'static str,
    toml: &'static str,
}

const PRESETS: &[Preset] = &[
    Preset {
        name: "linux-kernel",
        language: "c-cpp",
        toml: include_str!("../presets/c-cpp/linux-kernel.toml"),
    },
    Preset {
        name: "riot",
        language: "c-cpp",
        toml: include_str!("../presets/c-cpp/riot.toml"),
    },
    Preset {
        name: "rustfmt-compat",
        language: "rust",
        toml: include_str!("../presets/rust/rustfmt-compat.toml"),
    },
    Preset {
        name: "pep8",
        language: "python",
        toml: include_str!("../presets/python/pep8.toml"),
    },
    Preset {
        name: "black",
        language: "python",
        toml: include_str!("../presets/python/black.toml"),
    },
];

/// Names of all built-in presets, for error messages.
pub fn names() -> Vec<&'static str> {
    PRESETS.iter().map(|p| p.name).collect()
}

/// `(language, name)` for every built-in preset, for `--help` text grouped
/// by the language each preset targets.
pub fn describe() -> Vec<(&'static str, &'static str)> {
    PRESETS.iter().map(|p| (p.language, p.name)).collect()
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
    fn describe_groups_presets_by_language() {
        let described = describe();
        assert_eq!(described.len(), names().len());
        assert!(described.contains(&("c-cpp", "linux-kernel")));
        assert!(described.contains(&("c-cpp", "riot")));
        assert!(described.contains(&("rust", "rustfmt-compat")));
        assert!(described.contains(&("python", "pep8")));
        assert!(described.contains(&("python", "black")));
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

    #[test]
    fn python_presets_set_expected_widths() {
        assert_eq!(load("pep8").unwrap().python.max_width, 79);
        assert_eq!(load("black").unwrap().python.max_width, 88);
    }
}
