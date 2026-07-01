// Python has no in-repo formatter to diff against yet (src/formatter/python.rs
// is a stub — see its module docs), so these fixtures are PEP8/flake8-clean
// and `ruff format`-idempotent by construction: the stub's identity
// passthrough already matches `.expected`. As python.rs grows real emission
// logic, add fixtures that need actual rewriting and update `.expected` to
// what `ruff format` produces, same as tests/rust_corpus does for rustfmt.
//
// `python_corpus_matches_ruff_and_flake8` (ignored by default) shells out to
// the real `ruff` and `flake8` binaries to confirm the fixtures still hold
// that property — it's not run as part of `cargo test` because CI/dev
// environments aren't guaranteed to have either installed. Run explicitly
// with `cargo test --test python_corpus_test -- --ignored` when iterating on
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
    let files = corpus_files();
    assert!(!files.is_empty());

    for path in &files {
        let ruff = Command::new("ruff")
            .args(["format", "--check"])
            .arg(path)
            .status()
            .unwrap_or_else(|e| panic!("failed to run ruff on {}: {e}", path.display()));
        assert!(
            ruff.success(),
            "{} is not ruff-format-clean; regenerate its .expected from `ruff format`",
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
