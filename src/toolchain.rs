//! Substrate-level shared config (`toolchain.toml`) — currently just the
//! `[ignore].paths` glob list every tool (knots, moldy, tools_sqc) respects,
//! so a project expresses file/directory ignores once instead of per-tool.
//! See `lang_parsing_substrate/docs/unified-config-spec.md`.

use serde::Deserialize;
use std::path::Path;

use crate::error::MoldyError;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ToolchainConfig {
    pub ignore: ToolchainIgnore,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ToolchainIgnore {
    pub paths: Vec<String>,
}

impl ToolchainConfig {
    /// Walks up from `start_dir` looking for `toolchain.toml`, returning
    /// `Ok(None)` if none is found before the filesystem root. Unrecognized
    /// top-level sections (e.g. `[language.*]`, meant for other tools) are
    /// ignored rather than rejected — `ToolchainConfig` only models the
    /// slice moldy cares about.
    pub fn discover(start_dir: &Path) -> Result<Option<Self>, MoldyError> {
        let mut dir = Some(start_dir);
        while let Some(d) = dir {
            let candidate = d.join("toolchain.toml");
            if candidate.is_file() {
                let text = std::fs::read_to_string(&candidate).map_err(|e| MoldyError::Io {
                    path: candidate.display().to_string(),
                    source: e,
                })?;
                let cfg: ToolchainConfig =
                    toml::from_str(&text).map_err(|e| MoldyError::Config {
                        path: candidate.display().to_string(),
                        source: e,
                    })?;
                return Ok(Some(cfg));
            }
            dir = d.parent();
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discover_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let result = ToolchainConfig::discover(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn discover_finds_toolchain_toml_in_start_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("toolchain.toml"),
            "[ignore]\npaths = [\"vendor/**\"]\n",
        )
        .unwrap();
        let cfg = ToolchainConfig::discover(dir.path()).unwrap().unwrap();
        assert_eq!(cfg.ignore.paths, vec!["vendor/**".to_string()]);
    }

    #[test]
    fn discover_walks_up_from_a_nested_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("toolchain.toml"),
            "[ignore]\npaths = [\"third_party/**\"]\n",
        )
        .unwrap();
        let nested = dir.path().join("src").join("nested");
        fs::create_dir_all(&nested).unwrap();
        let cfg = ToolchainConfig::discover(&nested).unwrap().unwrap();
        assert_eq!(cfg.ignore.paths, vec!["third_party/**".to_string()]);
    }

    #[test]
    fn discover_ignores_unrelated_sections() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("toolchain.toml"),
            "[ignore]\npaths = [\"generated/**\"]\n\n[language.python]\nline_length = 88\n",
        )
        .unwrap();
        let cfg = ToolchainConfig::discover(dir.path()).unwrap().unwrap();
        assert_eq!(cfg.ignore.paths, vec!["generated/**".to_string()]);
    }
}
