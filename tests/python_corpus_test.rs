// `.py` inputs here range from already-clean to deliberately messy; `.expected`
// files are what `moldy` (default Config, i.e. the PEP8/79-col target) should
// produce, generated with `ruff format --line-length 79` and hand-verified —
// same static-fixture pattern tests/rust_corpus uses for rustfmt.
//
// `python_corpus_matches_ruff_and_flake8` (ignored by default) shells out to
// the real `ruff` and `flake8` binaries to confirm every `.expected` is
// itself ruff-format-idempotent and flake8-clean — i.e. that the target
// we're diffing against is actually correct, not just self-consistent. It's
// not run as part of `cargo test` because CI/dev environments aren't
// guaranteed to have either tool installed. Run explicitly with
// `cargo test --test python_corpus_test -- --ignored` when iterating on
// python.rs or the corpus.

use moldy::config::Config;
use moldy::formatter::format_source;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn corpus_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/python_corpus");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("tests/python_corpus directory missing")
        .filter_map(|e| {
            let e = e.ok()?;
            let p = e.path();
            if p.extension().and_then(|e| e.to_str()) == Some("py") {
                Some(p)
            } else {
                None
            }
        })
        .collect();
    files.sort();
    files
}

fn expected_files() -> Vec<PathBuf> {
    corpus_files()
        .iter()
        .map(|p| p.with_extension("py.expected"))
        .collect()
}

#[test]
fn python_corpus_matches_expected() {
    let files = corpus_files();
    assert!(
        !files.is_empty(),
        "tests/python_corpus/ contains no .py files"
    );

    let mut failures = Vec::new();

    for path in &files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

        let expected_path = path.with_extension("py.expected");
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("missing expected file {}: {e}", expected_path.display()));

        let config = Config::default();
        let formatted = format_source(path, &source, &config)
            .unwrap_or_else(|e| panic!("format_source failed for {}: {e}", path.display()));

        if formatted != expected {
            failures.push(path.file_name().unwrap().to_string_lossy().into_owned());
            eprintln!("FAIL: {}", path.display());
        }
    }

    if !failures.is_empty() {
        panic!(
            "{}/{} python corpus files failed: {}",
            failures.len(),
            files.len(),
            failures.join(", ")
        );
    }
}

#[test]
fn python_corpus_idempotent() {
    let files = corpus_files();
    let config = Config::default();

    for path in &files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

        let pass1 = format_source(path, &source, &config)
            .unwrap_or_else(|e| panic!("pass1 failed for {}: {e}", path.display()));

        let pass2 = format_source(path, &pass1, &config)
            .unwrap_or_else(|e| panic!("pass2 failed for {}: {e}", path.display()));

        assert_eq!(pass1, pass2, "idempotency failure for {}", path.display());
    }
}

#[test]
#[ignore = "requires ruff and flake8 on PATH; run explicitly when iterating on python.rs"]
fn python_corpus_matches_ruff_and_flake8() {
    let files = expected_files();
    assert!(!files.is_empty());

    for path in &files {
        let ruff = Command::new("ruff")
            .args(["format", "--check", "--line-length", "79"])
            .arg(path)
            .status()
            .unwrap_or_else(|e| panic!("failed to run ruff on {}: {e}", path.display()));
        assert!(
            ruff.success(),
            "{} is not ruff-format-clean at 79 cols; regenerate it with \
             `ruff format --line-length 79`",
            path.display()
        );

        let flake8 = Command::new("flake8")
            .arg("--max-line-length=79")
            .arg(path)
            .status()
            .unwrap_or_else(|e| panic!("failed to run flake8 on {}: {e}", path.display()));
        assert!(
            flake8.success(),
            "{} has flake8 violations under the PEP8 default (max-line-length=79)",
            path.display()
        );
    }
}
