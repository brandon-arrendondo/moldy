use moldy::config::Config;
use moldy::formatter::format_source;
use std::fs;
use std::path::{Path, PathBuf};

fn corpus_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("tests/corpus directory missing")
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

#[test]
fn corpus_matches_funky() {
    let files = corpus_files();
    assert!(!files.is_empty(), "tests/corpus/ contains no C/C++ files");

    let mut failures = Vec::new();

    for path in &files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

        let expected_path = path.with_extension(format!(
            "{}.expected",
            path.extension().unwrap().to_str().unwrap()
        ));
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("missing expected file {}: {e}", expected_path.display()));

        let config = Config::default();
        let formatted = format_source(path, &source, &config)
            .unwrap_or_else(|e| panic!("format_source failed for {}: {e}", path.display()));

        if formatted != expected {
            failures.push(path.file_name().unwrap().to_string_lossy().into_owned());
            eprintln!(
                "FAIL: {}\n  first diff at char {}",
                path.display(),
                formatted
                    .chars()
                    .zip(expected.chars())
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| formatted.len().min(expected.len()))
            );
            // Print first differing lines
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
            "{}/{} corpus files failed: {}",
            failures.len(),
            files.len(),
            failures.join(", ")
        );
    }
}

#[test]
fn corpus_idempotent() {
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
