use clap::Parser;
use lang_parsing_substrate::{is_discoverable_file, PathIgnore};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use walkdir::WalkDir;

use moldy::config::Config;
use moldy::error::{self, MoldyError};
use moldy::formatter;
use moldy::toolchain::ToolchainConfig;

mod outcome;
use outcome::{InvocationError, Mode, Run, Status, INCOMPLETE_EXIT, INVOCATION_EXIT};

const EXIT_STATUS_HELP: &str = "\
Exit status:
  0  Completed: every selected file was formatted, or --check found nothing
     to change. Also an empty selection with --allow-empty.
  1  --check found files that would be reformatted.
  2  The run could not start: invalid arguments or configuration, a named
     path that does not exist, or a directory named without -r.
  3  Incomplete: a selected file could not be read, formatted or written, or
     was left unformatted on purpose, or nothing was selected. Outranks 1.";

#[derive(Parser)]
#[command(
    name = "moldy",
    version,
    about = "Multi-language code formatter built on tree-sitter",
    after_help = EXIT_STATUS_HELP
)]
struct Cli {
    /// Source file(s) or directories to format.
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
    #[arg(long, conflicts_with = "in_place")]
    check: bool,

    /// Normalize line endings explicitly; default preserves source endings.
    #[arg(long, value_parser = ["lf", "crlf"], value_name = "STYLE")]
    line_endings: Option<String>,

    /// Recurse into directories and format all supported source files.
    #[arg(short = 'r', long)]
    recursive: bool,

    /// Accept a run that selects no files (exit 0) instead of failing it (exit 3).
    #[arg(long)]
    allow_empty: bool,

    /// Write each selected file's outcome (changed, unchanged, declined or
    /// failed) and the counts to FILE as JSON, even when the run fails.
    #[arg(long, value_name = "FILE")]
    completion_report: Option<PathBuf>,

    /// Print the tree-sitter CST and exit (for debugging).
    #[arg(long, hide = true)]
    dump_tree: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("Error: {err:?}");
            let code = if err.is::<InvocationError>() {
                INVOCATION_EXIT
            } else {
                INCOMPLETE_EXIT
            };
            ExitCode::from(code)
        }
    }
}

fn run() -> anyhow::Result<u8> {
    let cli = Cli::parse();
    let (config, ignore) = load_run_config(&cli).map_err(invocation)?;
    let (paths, walk_losses) = expand_paths(&cli.files, cli.recursive, &ignore)?;

    if cli.dump_tree {
        for path in &paths {
            formatter::dump_tree(path, &read_source(path)?)?;
        }
        return Ok(0);
    }

    let mut run = Run::new(mode(&cli));
    for (path, reason) in walk_losses {
        run.record(&path, Status::Failed, Some(reason));
    }
    if paths.is_empty() && run.is_empty() {
        run.select_nothing(cli.allow_empty);
    }
    for path in &paths {
        let (status, reason) = format_one(path, &cli, &config);
        run.record(path, status, reason);
    }
    run.print_summary();
    run.write_report(cli.completion_report.as_deref())?;
    Ok(run.exit_code())
}

/// An error that stops the run before any file is handled, as an
/// [`InvocationError`].
fn invocation(err: anyhow::Error) -> anyhow::Error {
    InvocationError(format!("{err:#}")).into()
}

fn mode(cli: &Cli) -> Mode {
    if cli.check {
        Mode::Check
    } else if cli.in_place {
        Mode::InPlace
    } else {
        Mode::Stdout
    }
}

/// The config, with `toolchain.toml`'s shared ignores folded in, and the
/// ignore matcher built from it. Any error here is a configuration error.
fn load_run_config(cli: &Cli) -> anyhow::Result<(Config, PathIgnore)> {
    let mut config = load_config(cli.config.as_deref(), cli.preset.as_deref())?;
    merge_toolchain_ignores(&mut config)?;
    let ignore = PathIgnore::new(&config.ignore.patterns)?;
    Ok((config, ignore))
}

/// Formats one file as the mode asks, and says how it ended. A file the
/// formatter declines is printed unchanged in stdout mode, as before.
fn format_one(path: &Path, cli: &Cli, config: &Config) -> (Status, Option<String>) {
    let source = match read_source(path) {
        Ok(source) => source,
        Err(err) => return (Status::Failed, Some(failure_reason(&err))),
    };
    let formatted =
        match formatter::format_source_with_line_endings(path, &source, config, ending_policy(cli))
        {
            Ok(formatted) => formatted,
            Err(
                err @ (MoldyError::Syntax { .. }
                | MoldyError::Unverified { .. }
                | MoldyError::TooDeep { .. }),
            ) => {
                if mode(cli) == Mode::Stdout {
                    print!("{source}");
                }
                return (Status::Declined, Some(declined_reason(&err)));
            }
            Err(err) => return (Status::Failed, Some(failure_reason(&err))),
        };
    let changed = source != formatted;
    match mode(cli) {
        Mode::InPlace if changed => {
            if let Err(err) = std::fs::write(path, formatted.as_bytes()) {
                return (Status::Failed, Some(format!("could not write: {err}")));
            }
        }
        Mode::Stdout => print!("{formatted}"),
        _ => {}
    }
    let status = if changed {
        Status::Changed
    } else {
        Status::Unchanged
    };
    (status, None)
}

fn ending_policy(cli: &Cli) -> formatter::LineEndings {
    match cli.line_endings.as_deref() {
        Some("lf") => formatter::LineEndings::Lf,
        Some("crlf") => formatter::LineEndings::Crlf,
        _ => formatter::LineEndings::Preserve,
    }
}

/// Why a file could not be formatted, without repeating its path.
fn failure_reason(err: &MoldyError) -> String {
    match err {
        MoldyError::Io { source, .. } => source.to_string(),
        MoldyError::NotUtf8 { .. } => "not valid UTF-8".to_string(),
        MoldyError::UnsupportedLanguage(_) => "unsupported language".to_string(),
        other => other.to_string(),
    }
}

/// Why the formatter left a file unchanged, e.g. "YAML syntax error at line 3".
fn declined_reason(err: &MoldyError) -> String {
    err.to_string()
        .trim_end_matches("; file left unchanged")
        .to_string()
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
/// (knots, moldy, aurora-lint) instead of just moldy's own `[ignore]` section.
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

/// A file the recursive walk could not reach, with the reason.
type WalkLoss = (PathBuf, String);

/// The files the named paths select, and the files a recursive walk could
/// not reach (each with its reason). A named path that does not exist, or a
/// directory named without `-r`, stops the run as an invocation error.
fn expand_paths(
    paths: &[PathBuf],
    recursive: bool,
    ignore: &PathIgnore,
) -> anyhow::Result<(Vec<PathBuf>, Vec<WalkLoss>)> {
    let mut out = Vec::new();
    let mut lost = Vec::new();

    for p in paths {
        if p.as_os_str() == "-" {
            out.push(p.clone());
            continue;
        }
        if p.is_dir() {
            if !recursive {
                let message = format!("{} is a directory; pass -r to recurse", p.display());
                return Err(InvocationError(message).into());
            }
            walk(p, ignore, &mut out, &mut lost);
        } else if !p.exists() {
            let message = format!("{} does not exist", p.display());
            return Err(InvocationError(message).into());
        } else if !should_ignore(p, ignore) {
            out.push(p.clone());
        }
    }

    Ok((out, lost))
}

/// Recursive discovery under `root`. What the walk cannot reach, a directory
/// it cannot read or a dangling symlink discovery would have taken, is lost.
fn walk(root: &Path, ignore: &PathIgnore, out: &mut Vec<PathBuf>, lost: &mut Vec<WalkLoss>) {
    for entry in WalkDir::new(root).follow_links(true) {
        match entry {
            Ok(entry) => {
                let ep = entry.path();
                if ep.is_file() && is_discoverable_file(ep) && !should_ignore(ep, ignore) {
                    out.push(ep.to_path_buf());
                }
            }
            Err(err) => lost.extend(walk_loss(root, &err, ignore)),
        }
    }
}

/// The input one walk error loses, with its reason. A symlink loop loses
/// nothing, since its target is walked anyway; nor does a path discovery would
/// have skipped, or one an ignore pattern covers.
fn walk_loss(root: &Path, err: &walkdir::Error, ignore: &PathIgnore) -> Option<WalkLoss> {
    if err.loop_ancestor().is_some() {
        return None;
    }
    let path = err.path().unwrap_or(root);
    let selected = (path.is_dir() || is_discoverable_file(path)) && !should_ignore(path, ignore);
    let cause = err
        .io_error()
        .map_or_else(|| err.to_string(), |e| e.to_string());
    selected.then(|| (path.to_path_buf(), format!("walk: {cause}")))
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
    fn discovery_matches_file_names_and_skips_explicit_only() {
        assert!(is_discoverable_file(Path::new("src/foo.c")));
        assert!(is_discoverable_file(Path::new("cmake/CMakeLists.txt")));
        assert!(!is_discoverable_file(Path::new("src/foo.h")));
        assert!(!is_discoverable_file(Path::new("notes.txt")));
        assert!(!is_discoverable_file(Path::new("Makefile")));
    }

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
