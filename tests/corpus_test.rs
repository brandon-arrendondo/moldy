use moldy::config::Config;
use moldy::formatter::format_source;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

#[test]
fn corpus_expected_files_pass_cli_check() {
    let dir = tempfile::tempdir().expect("temporary corpus directory");
    for path in corpus_files() {
        let expected_path = path.with_extension(format!(
            "{}.expected",
            path.extension().unwrap().to_str().unwrap()
        ));
        let check_path = dir.path().join(path.file_name().unwrap());
        fs::copy(&expected_path, &check_path).expect("copy expected corpus file");
        let output = Command::new(env!("CARGO_BIN_EXE_moldy"))
            .current_dir(dir.path())
            .arg("--check")
            .arg(&check_path)
            .output()
            .expect("run moldy --check");
        assert!(
            output.status.success(),
            "expected file failed --check: {}\n{}",
            expected_path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn array_bound_directive_recovery_is_unchanged() {
    // The grammar cannot read directives inside an array bound, so the
    // declaration holding them is kept as written.
    let source = include_str!("regression/expression_directives.c");
    let expected = include_str!("regression/expression_directives.c.expected");
    let formatted = format_source(
        Path::new("expression_directives.c"),
        source,
        &Config::default(),
    )
    .expect("format recovered array bound");
    assert_eq!(formatted, expected);
}

#[test]
fn continued_initializer_directives_preserve_crlf() {
    let source = "int values[] = {\r\n    0,\r\n#if defined(X) && \\\r\n    defined(Y)\r\n    1,\r\n#endif\r\n    2\r\n};\r\n";
    let path = Path::new("continued.c");
    let config = Config::default();
    let formatted = format_source(path, source, &config).expect("format CRLF initializer");
    assert!(
        formatted.contains("\\\r\n"),
        "continued directive lost CRLF"
    );
    assert!(!formatted.replace("\r\n", "").contains(['\r', '\n']));
    assert_eq!(
        formatted,
        format_source(path, &formatted, &config).expect("format a second time")
    );
}
