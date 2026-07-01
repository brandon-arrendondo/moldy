use clap::Parser;
use lang_parsing_substrate::PathIgnore;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use moldy::config::Config;
use moldy::error;
use moldy::formatter;
use moldy::toolchain::ToolchainConfig;

#[derive(Parser)]
#[command(
    name = "moldy",
    version,
    about = "Multi-language code formatter built on tree-sitter"
)]
struct Cli {
    /// Source file(s) or director(ies) to format. Use `-` to read from stdin.
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Path to TOML config file (default: look for moldy.toml in cwd).
    #[arg(short, long, value_name = "FILE", conflicts_with = "preset")]
    config: Option<PathBuf>,

    /// Use a built-in style preset instead of a config file. C/C++: "linux-kernel", "riot".
    /// Rust: "rustfmt-compat". Python: "pep8", "black".
    #[arg(long, value_name = "NAME")]
    preset: Option<String>,

    /// Edit file(s) in place instead of writing to stdout.
    #[arg(short = 'i', long)]
    in_place: bool,

    /// Check mode: exit 1 if any file would change; do not write.
    #[arg(long)]
    check: bool,

    /// Recurse into directories and format all supported source files.
    #[arg(short = 'r', long)]
    recursive: bool,

    /// Print the tree-sitter CST and exit (for debugging).
    #[arg(long, hide = true)]
    dump_tree: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.check && cli.in_place {
        anyhow::bail!("--check and --in-place are mutually exclusive");
    }

    let mut config = load_config(cli.config.as_deref(), cli.preset.as_deref())?;
    merge_toolchain_ignores(&mut config)?;
    let expanded = expand_paths(&cli.files, cli.recursive, &config)?;

    let mut any_changed = false;

    for path in &expanded {
        let source = read_source(path)?;

        if cli.dump_tree {
            formatter::dump_tree(path, &source)?;
            continue;
        }

        let formatted = formatter::format_source(path, &source, &config)?;

        if cli.check {
            if source != formatted {
                eprintln!("{}: would reformat", path.display());
                any_changed = true;
            }
            continue;
        }

        if cli.in_place {
            if source != formatted {
                std::fs::write(path, formatted.as_bytes()).map_err(|e| error::MoldyError::Io {
                    path: path.display().to_string(),
                    source: e,
                })?;
            }
        } else {
            print!("{formatted}");
        }
    }

    if cli.check && any_changed {
        std::process::exit(1);
    }

    Ok(())
}

fn load_config(explicit: Option<&Path>, preset: Option<&str>) -> anyhow::Result<Config> {
    if let Some(name) = preset {
        return Ok(moldy::presets::load(name)?);
    }
    if let Some(p) = explicit {
        return Ok(Config::load(p)?);
    }
    let default = Path::new("moldy.toml");
    if default.exists() {
        return Ok(Config::load(default)?);
    }
    Ok(Config::default())
}

/// Folds `toolchain.toml`'s shared `[ignore].paths` (discovered by walking up
/// from the current directory) into `config.ignore.patterns`, so a project's
/// file/directory ignores are expressed once and respected by every tool
/// (knots, moldy, tools_sqc) instead of just moldy's own `[ignore]` section.
fn merge_toolchain_ignores(config: &mut Config) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    merge_toolchain_ignores_from(config, &cwd)
}

fn merge_toolchain_ignores_from(config: &mut Config, start_dir: &Path) -> anyhow::Result<()> {
    if let Some(toolchain) = ToolchainConfig::discover(start_dir)? {
        config.ignore.patterns.extend(toolchain.ignore.paths);
    }
    Ok(())
}

fn read_source(path: &Path) -> Result<String, error::MoldyError> {
    let bytes = std::fs::read(path).map_err(|e| error::MoldyError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    String::from_utf8(bytes).map_err(|_| error::MoldyError::NotUtf8 {
        path: path.display().to_string(),
    })
}

fn expand_paths(
    paths: &[PathBuf],
    recursive: bool,
    config: &Config,
) -> anyhow::Result<Vec<PathBuf>> {
    let ignore = PathIgnore::new(&config.ignore.patterns)?;
    let mut out = Vec::new();

    for p in paths {
        if p.as_os_str() == "-" {
            out.push(p.clone());
            continue;
        }
        if p.is_dir() {
            if !recursive {
                eprintln!(
                    "warning: {} is a directory; pass -r to recurse",
                    p.display()
                );
                continue;
            }
            for entry in WalkDir::new(p).follow_links(true) {
                let entry = entry?;
                let ep = entry.path();
                if !ep.is_file() {
                    continue;
                }
                let Some(ext) = ep.extension() else { continue };
                if !lang_parsing_substrate::is_source_extension(ext) {
                    continue;
                }
                if should_ignore(ep, &ignore) {
                    continue;
                }
                out.push(ep.to_path_buf());
            }
        } else {
            if should_ignore(p, &ignore) {
                continue;
            }
            out.push(p.clone());
        }
    }

    Ok(out)
}

/// Checks the full path first, then falls back to matching the bare filename
/// alone — so a pattern like `"*.min.js"` ignores matches anywhere in the
/// tree without needing a `"**/*.min.js"` prefix.
fn should_ignore(path: &Path, ignore: &PathIgnore) -> bool {
    if ignore.is_ignored(path) {
        return true;
    }
    if let Some(name) = path.file_name() {
        if ignore.is_ignored(Path::new(name)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_ignore_matches_full_path_glob() {
        let ignore = PathIgnore::new(["vendor/**"]).unwrap();
        assert!(should_ignore(Path::new("vendor/lib/foo.c"), &ignore));
        assert!(!should_ignore(Path::new("src/foo.c"), &ignore));
    }

    #[test]
    fn should_ignore_falls_back_to_bare_filename() {
        let ignore = PathIgnore::new(["*.min.js"]).unwrap();
        assert!(should_ignore(
            Path::new("src/vendor/jquery.min.js"),
            &ignore
        ));
    }

    #[test]
    fn merge_toolchain_ignores_extends_config_patterns() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("toolchain.toml"),
            "[ignore]\npaths = [\"generated/**\"]\n",
        )
        .unwrap();
        let mut config = Config {
            ignore: moldy::config::IgnoreConfig {
                patterns: vec!["vendor/**".to_string()],
            },
            ..Config::default()
        };
        merge_toolchain_ignores_from(&mut config, dir.path()).unwrap();
        assert_eq!(
            config.ignore.patterns,
            vec!["vendor/**".to_string(), "generated/**".to_string()]
        );
    }

    #[test]
    fn merge_toolchain_ignores_is_noop_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        merge_toolchain_ignores_from(&mut config, dir.path()).unwrap();
        assert!(config.ignore.patterns.is_empty());
    }
}
