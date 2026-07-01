/// Acceptance test suite: broader formatter coverage than `corpus_test.rs`.
///
/// Layout:
///   tests/acceptance/<lang>/positive/*.{c,cpp}  — common real-world formatting
///     styles. `.expected` is generated from `funky` (the parity target), so a
///     failure here is a genuine parity gap, not a test bug.
///   tests/acceptance/<lang>/negative/*.{c,cpp}   — malformed/troublesome input
///     (unclosed braces/parens, unterminated strings/comments, truncated
///     files, etc). `.expected` is a snapshot of moldy's own current
///     best-effort output: moldy has no dedicated syntax-error path, so these
///     tests lock down "does not panic, produces this output" rather than any
///     particular recovery behavior.
use moldy::config::Config;
use moldy::formatter::format_source;
use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/acceptance")
}

fn fixtures(lang_dir: &str, kind: &str) -> Vec<PathBuf> {
    let dir = manifest_dir().join(lang_dir).join(kind);
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} missing: {e}", dir.display()))
        .filter_map(|e| {
            let e = e.ok()?;
            let p = e.path();
            let ext = p.extension()?.to_str()?;
            if matches!(ext, "c" | "cpp" | "h" | "hpp") {
                Some(p)
            } else {
                None
            }
        })
        .collect();
    files.sort();
    files
}

fn expected_path(path: &Path) -> PathBuf {
    let ext = path.extension().unwrap().to_str().unwrap();
    path.with_extension(format!("{ext}.expected"))
}

/// Format every fixture in `paths` and compare to its `.expected` file,
/// collecting all mismatches before panicking (so one run shows every gap).
fn assert_all_match_expected(paths: &[PathBuf]) {
    let config = Config::default();
    let mut failures = Vec::new();

    for path in paths {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let expected_file = expected_path(path);
        let expected = fs::read_to_string(&expected_file)
            .unwrap_or_else(|e| panic!("missing expected file {}: {e}", expected_file.display()));

        let formatted = format_source(path, &source, &config)
            .unwrap_or_else(|e| panic!("format_source failed for {}: {e}", path.display()));

        if formatted != expected {
            failures.push(path.file_name().unwrap().to_string_lossy().into_owned());
            eprintln!("FAIL: {}", path.display());
            for (i, (got, want)) in formatted.lines().zip(expected.lines()).enumerate() {
                if got != want {
                    eprintln!("  line {}: got  {:?}", i + 1, got);
                    eprintln!("  line {}: want {:?}", i + 1, want);
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
            "{}/{} fixtures failed: {}",
            failures.len(),
            paths.len(),
            failures.join(", ")
        );
    }
}

fn assert_all_idempotent(paths: &[PathBuf]) {
    let config = Config::default();
    for path in paths {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let pass1 = format_source(path, &source, &config)
            .unwrap_or_else(|e| panic!("pass1 failed for {}: {e}", path.display()));
        let pass2 = format_source(path, &pass1, &config)
            .unwrap_or_else(|e| panic!("pass2 failed for {}: {e}", path.display()));
        assert_eq!(pass1, pass2, "idempotency failure for {}", path.display());
    }
}

// ── C: common formatting styles ───────────────────────────────────────────

#[test]
fn c_positive_matches_expected() {
    assert_all_match_expected(&fixtures("c", "positive"));
}

#[test]
fn c_positive_idempotent() {
    assert_all_idempotent(&fixtures("c", "positive"));
}

// ── C: malformed / troublesome input ──────────────────────────────────────

#[test]
fn c_negative_matches_expected() {
    assert_all_match_expected(&fixtures("c", "negative"));
}

#[test]
fn c_negative_idempotent() {
    assert_all_idempotent(&fixtures("c", "negative"));
}

// ── C++: common formatting styles ─────────────────────────────────────────

#[test]
fn cpp_positive_matches_expected() {
    assert_all_match_expected(&fixtures("cpp", "positive"));
}

#[test]
fn cpp_positive_idempotent() {
    assert_all_idempotent(&fixtures("cpp", "positive"));
}

// ── C++: malformed / troublesome input ────────────────────────────────────

#[test]
fn cpp_negative_matches_expected() {
    assert_all_match_expected(&fixtures("cpp", "negative"));
}

#[test]
fn cpp_negative_idempotent() {
    assert_all_idempotent(&fixtures("cpp", "negative"));
}

/// Negative fixtures must never panic and must always produce *some* output
/// (moldy has no dedicated syntax-error path — see module docs above).
#[test]
fn negative_fixtures_never_panic() {
    let config = Config::default();
    for path in fixtures("c", "negative")
        .into_iter()
        .chain(fixtures("cpp", "negative"))
    {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        format_source(&path, &source, &config)
            .unwrap_or_else(|e| panic!("format_source errored for {}: {e}", path.display()));
    }
}
