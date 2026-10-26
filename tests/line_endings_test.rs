use moldy::config::Config;
use moldy::formatter::{format_source, format_source_with_line_endings, LineEndings};
use std::path::Path;
use std::process::Command;

#[test]
fn every_language_preserves_uniform_crlf_and_supports_explicit_normalization() {
    for (name, source) in [
        ("simple.c", "int value = 1;\n"),
        ("simple.cpp", "int value = 1;\n"),
        ("simple.rs", "fn main() {\n    work();\n}\n"),
        ("simple.py", "value = 1\n"),
        ("simple.jsonc", "{\n  \"a\": [\n    1\n  ] // note\n}\n"),
        ("simple.yaml", "a:\n  - b # note\nrun: |\n  x\n\n  y\n"),
        (
            "simple.sh",
            "if a; then\n  cat << EOF # note\nbody\nEOF\nfi\n",
        ),
        (
            "CMakeLists.txt",
            "if(A)\n    my_fn(\n      x\n    )\n    set(X 1) # c\nendif()\n",
        ),
        (
            "simple.md",
            "# Title\n\n- item\n  > quote\n\n```sh\nx\n```\n",
        ),
    ] {
        let path = Path::new(name);
        let config = Config::default();
        let lf = format_source(path, source, &config).unwrap();
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(format_source(path, &crlf, &config).unwrap(), crlf);
        assert_eq!(
            format_source_with_line_endings(path, &crlf, &config, LineEndings::Lf).unwrap(),
            lf
        );
        assert_eq!(
            format_source_with_line_endings(path, &lf, &config, LineEndings::Crlf).unwrap(),
            crlf
        );
    }
}

#[test]
fn mapped_mixed_lines_keep_their_own_endings_after_indentation_changes() {
    let source = "int first=1;\r\nint second=2;\nint third=3;\r\n";
    let path = Path::new("mixed.c");
    let formatted = format_source(path, source, &Config::default()).unwrap();
    assert_eq!(
        formatted,
        "int first = 1;\r\nint second = 2;\nint third = 3;\r\n"
    );
    assert_eq!(
        format_source(path, &formatted, &Config::default()).unwrap(),
        formatted
    );
}

#[test]
fn initializer_continuations_and_comment_carriage_returns_preserve_crlf() {
    let source = "int values[] = {\r\n    0, // first\r\n#if defined(X) && \\\r\n    defined(Y)\r\n    1,\r\n#endif\r\n    2 // last\r\n};\r\n";
    let path = Path::new("continued.c");
    let config = Config::default();
    let formatted = format_source(path, source, &config).unwrap();
    assert!(formatted.contains("#if defined(X) && \\\r\n    defined(Y)\r\n"));
    assert!(formatted.contains("// first\r\n"));
    assert!(formatted.contains("// last\r\n"));
    assert!(!formatted.replace("\r\n", "").contains(['\r', '\n']));
    assert_eq!(format_source(path, &formatted, &config).unwrap(), formatted);
}

#[test]
fn opaque_multiline_strings_keep_mixed_endings() {
    let path = Path::new("literal.py");
    let source = "value = \"\"\"first\r\nsecond\nthird\r\n\"\"\"\n";
    assert_eq!(
        format_source(path, source, &Config::default()).unwrap(),
        source
    );
}

#[test]
fn cli_check_and_in_place_preserve_endings_unless_the_flag_is_present() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("simple.c");
    let source = b"int first = 1;\r\nint second = 2;\n";
    std::fs::write(&path, source).unwrap();
    // Existing style and preset settings do not request a CLI conversion.
    std::fs::write(dir.path().join("moldy.toml"), "[newlines]\nstyle = 'lf'\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_moldy"))
            .current_dir(dir.path())
            .args(args)
            .arg(&path)
            .output()
            .unwrap()
    };
    assert!(run(&["--check"]).status.success());
    assert!(run(&["--check", "--preset", "riot"]).status.success());
    assert!(run(&["--in-place"]).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), source);
    assert_eq!(
        run(&["--check", "--line-endings", "lf"]).status.code(),
        Some(1)
    );
    assert!(run(&["--in-place", "--line-endings", "crlf"])
        .status
        .success());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"int first = 1;\r\nint second = 2;\r\n"
    );
    assert!(run(&["--check", "--line-endings", "crlf"]).status.success());
    assert_eq!(run(&["--line-endings", "native"]).status.code(), Some(2));
}

#[test]
fn raw_rust_literal_keeps_embedded_mixed_endings() {
    let source = "const TEXT: &str = r#\"first\r\nsecond\nthird\r\n\"#;\n";
    let formatted = format_source(Path::new("literal.rs"), source, &Config::default()).unwrap();
    assert!(formatted.contains("first\r\nsecond\nthird\r\n"));
    assert_eq!(
        format_source(Path::new("literal.rs"), &formatted, &Config::default()).unwrap(),
        formatted
    );
}

#[test]
fn wrapping_a_literal_prefix_keeps_its_first_embedded_ending() {
    let source = "first = 1\nsecond = 2\nvalue = {\"key\": \"\"\"first\r\nsecond\nthird\n\"\"\"}\n";
    let mut config = Config::default();
    config.python.max_width = 20;
    let path = Path::new("wrapped_literal.py");
    let formatted = format_source(path, source, &config).unwrap();
    assert!(formatted.contains("value = {\n"));
    assert!(formatted.contains("first\r\nsecond\nthird\n"));
    assert_eq!(format_source(path, &formatted, &config).unwrap(), formatted);
}

#[test]
fn directive_comments_are_equivalent_and_idempotent_across_endings() {
    let config = Config::default();
    let path = Path::new("directive.c");
    for directive in ["#include <a.h>", "#define A 1", "#pragma once", "#endif"] {
        let source = format!("{directive}\n// c\nint x;\n");
        let crlf_source = source.replace('\n', "\r\n");
        let lf = format_source(path, &source, &config);
        let formatted = format_source(path, &crlf_source, &config);
        let (lf, formatted) = match (lf, formatted) {
            (Ok(lf), Ok(formatted)) => (lf, formatted),
            // Declined either way, for the same reason.
            (Err(lf), Err(crlf)) => {
                assert_eq!(lf.to_string(), crlf.to_string(), "{directive}");
                continue;
            }
            (lf, crlf) => panic!("{directive}: {lf:?} but with CRLF {crlf:?}"),
        };
        assert_eq!(formatted.replace("\r\n", "\n"), lf, "{directive}");
        assert_eq!(format_source(path, &formatted, &config).unwrap(), formatted);
        assert_eq!(
            format_source_with_line_endings(path, &crlf_source, &config, LineEndings::Lf).unwrap(),
            lf
        );
    }
}

#[test]
fn c_corpus_endings_preserve_layout_and_idempotency() {
    let config = Config::default();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("c" | "cpp" | "h" | "hpp")
        ) {
            continue;
        }
        // These recovered directive fixtures have different CRLF
        // parse trees; ADR-0005 records the limitation separately.
        if matches!(
            path.file_name().and_then(|s| s.to_str()),
            Some("initializer_directives.c" | "initializer_directives.cpp")
        ) {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let lf = format_source(&path, &source, &config).unwrap();
        let crlf_source = source.replace('\n', "\r\n");
        let output = format_source(&path, &crlf_source, &config).unwrap();
        assert!(output.contains("\r\n"), "missing CRLF: {}", path.display());
        assert!(
            !output.replace("\r\n", "").contains(['\r', '\n']),
            "non-CRLF ending: {}",
            path.display()
        );
        assert_eq!(
            output.replace("\r\n", "\n"),
            lf,
            "layout: {}",
            path.display()
        );
        assert_eq!(
            format_source(&path, &output, &config).unwrap(),
            output,
            "idempotency: {}",
            path.display()
        );
    }
}
