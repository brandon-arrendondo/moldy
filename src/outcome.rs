//! What one run did with each selected file, so that a run that could not
//! format every selected file never passes as clean (docs/adr/0003).
//!
//! Each file ends as changed, unchanged, declined (the formatter left it
//! unchanged on purpose) or failed (it could not be read, formatted or
//! written). The exit status, the per-file stderr lines, the summary and the
//! `--completion-report` document all come from those outcomes.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;
use std::fmt;
use std::fs;
use std::path::Path;

/// Exit status of a run that cannot start as invoked: invalid arguments or
/// configuration, a named path that does not exist, or a directory named
/// without `-r`.
pub(crate) const INVOCATION_EXIT: u8 = 2;

/// Exit status of a run that started but did not format every selected file,
/// or selected none without `--allow-empty`. It outranks `--check`'s 1.
pub(crate) const INCOMPLETE_EXIT: u8 = 3;

/// A run that cannot start as invoked; `main` maps it to [`INVOCATION_EXIT`].
#[derive(Debug)]
pub(crate) struct InvocationError(pub(crate) String);

impl fmt::Display for InvocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvocationError {}

/// What the run does with a formatted file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Mode {
    /// `--check`: report, write nothing.
    Check,
    /// `--in-place`: rewrite changed files.
    InPlace,
    /// Neither: print every formatted file on stdout.
    Stdout,
}

/// How one selected file ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Status {
    /// Formatting changes it: it would be, or was, rewritten.
    Changed,
    /// It is already formatted.
    Unchanged,
    /// The formatter left it unchanged on purpose: a syntax error in a
    /// strict grammar, a Markdown rewrite it could not verify, or nesting too
    /// deep. The run is incomplete, since the file was not checked.
    Declined,
    /// It could not be reached, read, formatted or written.
    Failed,
}

#[derive(Debug, Serialize)]
struct FileResult {
    file: String,
    status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

/// The outcomes of one run, in the order the files were handled.
#[derive(Debug)]
pub(crate) struct Run {
    mode: Mode,
    results: Vec<FileResult>,
    /// Nothing was selected and `--allow-empty` was not given.
    empty_refused: bool,
}

impl Run {
    pub(crate) fn new(mode: Mode) -> Self {
        Run {
            mode,
            results: Vec::new(),
            empty_refused: false,
        }
    }

    /// Records one file's outcome and prints its stderr line: "would
    /// reformat" or "reformatted" for a changed file under `--check` or
    /// `--in-place`, and "not formatted" with the reason for a declined or
    /// failed one in every mode.
    pub(crate) fn record(&mut self, file: &Path, status: Status, reason: Option<String>) {
        let file = file.display().to_string();
        match (status, self.mode) {
            (Status::Changed, Mode::Check) => eprintln!("{file}: would reformat"),
            (Status::Changed, Mode::InPlace) => eprintln!("{file}: reformatted"),
            (Status::Declined | Status::Failed, _) => {
                eprintln!(
                    "{file}: not formatted ({})",
                    reason.as_deref().unwrap_or("")
                );
            }
            _ => {}
        }
        self.results.push(FileResult {
            file,
            status,
            reason,
        });
    }

    /// Ends a run that selected nothing. With `allow`, that is a success;
    /// without it, the run is incomplete.
    pub(crate) fn select_nothing(&mut self, allow: bool) {
        if allow {
            eprintln!("moldy: no files selected");
        } else {
            eprintln!("moldy: no files selected; pass --allow-empty to accept an empty selection");
            self.empty_refused = true;
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    fn count(&self, status: Status) -> usize {
        self.results.iter().filter(|r| r.status == status).count()
    }

    fn not_formatted(&self) -> usize {
        self.count(Status::Declined) + self.count(Status::Failed)
    }

    pub(crate) fn is_complete(&self) -> bool {
        !self.empty_refused && self.not_formatted() == 0
    }

    /// 3 when incomplete, else 1 when `--check` found a file to change, else 0.
    pub(crate) fn exit_code(&self) -> u8 {
        if !self.is_complete() {
            INCOMPLETE_EXIT
        } else if self.mode == Mode::Check && self.count(Status::Changed) > 0 {
            1
        } else {
            0
        }
    }

    /// The summary on stderr: always under `--check` and `--in-place`, and on
    /// stdout runs only when incomplete, so that an editor piping one file
    /// through moldy sees nothing new. An incomplete run says how many files
    /// were not formatted and, under `--in-place`, that the files already
    /// rewritten stay rewritten.
    pub(crate) fn print_summary(&self) {
        if self.results.is_empty() || (self.mode == Mode::Stdout && self.is_complete()) {
            return;
        }
        let changed = match self.mode {
            Mode::Check => "would be reformatted",
            Mode::InPlace => "reformatted",
            Mode::Stdout => "changed",
        };
        eprintln!(
            "moldy: {}: {} {changed}, {} unchanged, {} not formatted",
            files(self.results.len()),
            self.count(Status::Changed),
            self.count(Status::Unchanged),
            self.not_formatted()
        );
        if self.is_complete() {
            return;
        }
        eprintln!(
            "moldy: incomplete run: {} of {} selected file(s) were not formatted",
            self.not_formatted(),
            self.results.len()
        );
        let rewritten = self.count(Status::Changed);
        if self.mode == Mode::InPlace && rewritten > 0 {
            eprintln!(
                "moldy: the {} reformatted above stay rewritten; nothing is rolled back",
                files(rewritten)
            );
        }
    }

    /// The `--completion-report` document.
    pub(crate) fn report(&self) -> serde_json::Value {
        let mut results: Vec<&FileResult> = self.results.iter().collect();
        results.sort_by(|a, b| a.file.cmp(&b.file));
        json!({
            "complete": self.is_complete(),
            "mode": self.mode,
            "files": {
                "selected": self.results.len(),
                "changed": self.count(Status::Changed),
                "unchanged": self.count(Status::Unchanged),
                "declined": self.count(Status::Declined),
                "failed": self.count(Status::Failed),
            },
            "results": results,
        })
    }

    /// Writes the report to `path`, when one is given.
    pub(crate) fn write_report(&self, path: Option<&Path>) -> Result<()> {
        let Some(path) = path else {
            return Ok(());
        };
        let text = serde_json::to_string_pretty(&self.report())?;
        fs::write(path, text + "\n")
            .with_context(|| format!("Failed to write completion report: {}", path.display()))
    }
}

/// "1 file" or "N files".
fn files(n: usize) -> String {
    if n == 1 {
        "1 file".to_string()
    } else {
        format!("{n} files")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with(mode: Mode, statuses: &[Status]) -> Run {
        let mut run = Run::new(mode);
        for (i, status) in statuses.iter().enumerate() {
            let reason = matches!(status, Status::Declined | Status::Failed).then(|| "x".into());
            run.record(Path::new(&format!("f{i}.c")), *status, reason);
        }
        run
    }

    #[test]
    fn incomplete_outranks_differences() {
        let run = run_with(Mode::Check, &[Status::Changed, Status::Failed]);
        assert_eq!(run.exit_code(), INCOMPLETE_EXIT);
        let run = run_with(Mode::Check, &[Status::Changed, Status::Unchanged]);
        assert_eq!(run.exit_code(), 1);
        let run = run_with(Mode::InPlace, &[Status::Changed]);
        assert_eq!(run.exit_code(), 0);
    }

    #[test]
    fn a_declined_file_makes_the_run_incomplete() {
        let run = run_with(Mode::Stdout, &[Status::Unchanged, Status::Declined]);
        assert!(!run.is_complete());
        assert_eq!(run.exit_code(), INCOMPLETE_EXIT);
    }

    #[test]
    fn an_empty_selection_is_incomplete_unless_allowed() {
        let mut run = Run::new(Mode::Check);
        run.select_nothing(true);
        assert_eq!(run.exit_code(), 0);
        let mut run = Run::new(Mode::Check);
        run.select_nothing(false);
        assert_eq!(run.exit_code(), INCOMPLETE_EXIT);
        assert_eq!(run.report()["complete"], false);
    }

    #[test]
    fn the_report_counts_each_status_and_sorts_by_file() {
        let run = run_with(
            Mode::InPlace,
            &[Status::Failed, Status::Changed, Status::Unchanged],
        );
        let report = run.report();
        assert_eq!(report["mode"], "in-place");
        assert_eq!(report["files"]["selected"], 3);
        assert_eq!(report["files"]["failed"], 1);
        assert_eq!(report["results"][0]["file"], "f0.c");
        assert_eq!(report["results"][0]["reason"], "x");
        assert!(report["results"][1].get("reason").is_none());
    }

    #[test]
    fn file_counts_read_in_the_singular_and_plural() {
        assert_eq!(files(1), "1 file");
        assert_eq!(files(3), "3 files");
    }
}
