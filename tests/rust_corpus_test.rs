// Rust has no funky reference implementation to match — funky is C/C++ only.
// These fixtures instead lock in this formatter's own output: each `.rs` is
// paired with an `.rs.expected` that was generated (and hand-reviewed) from
// the current formatter, so future changes get caught as regressions.

use moldy::config::Config;
use moldy::formatter::format_source;
use std::fs;
use std::path::{Path, PathBuf};

fn corpus_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rust_corpus");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("tests/rust_corpus directory missing")
        .filter_map(|e| {
            let e = e.ok()?;
            let p = e.path();
            if p.extension().and_then(|e| e.to_str()) == Some("rs") {
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
fn rust_corpus_matches_expected() {
    let files = corpus_files();
    assert!(
        !files.is_empty(),
        "tests/rust_corpus/ contains no .rs files"
    );

    let mut failures = Vec::new();

    for path in &files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

        let expected_path = path.with_extension("rs.expected");
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("missing expected file {}: {e}", expected_path.display()));

        let config = Config::default();
        let formatted = format_source(path, &source, &config)
            .unwrap_or_else(|e| panic!("format_source failed for {}: {e}", path.display()));

        if formatted != expected {
            failures.push(path.file_name().unwrap().to_string_lossy().into_owned());
            eprintln!("FAIL: {}", path.display());
            for (i, (fl, el)) in formatted.lines().zip(expected.lines()).enumerate() {
                if fl != el {
                    eprintln!("  line {}: got  {:?}", i + 1, fl);
                    eprintln!("  line {}: want {:?}", i + 1, el);
                    break;
                }
            }
            if formatted.lines().count() != expected.lines().count() {
                eprintln!(
                    "  got {} lines, expected {} lines",
                    formatted.lines().count(),
                    expected.lines().count()
                );
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{}/{} rust corpus files failed: {}",
            failures.len(),
            files.len(),
            failures.join(", ")
        );
    }
}

#[test]
fn rust_corpus_idempotent() {
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
