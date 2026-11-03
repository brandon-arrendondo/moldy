//! The C, C++, Rust and Python formatters decline code nested past their
//! limit instead of exhausting the stack, and format code nested just
//! inside it. Test threads have small stacks (2 MiB) and this build is
//! not optimized, so this also checks the room the formatters run with,
//! on the caller's thread and on the thread deeper trees get.

use moldy::config::Config;
use moldy::error::MoldyError;
use moldy::formatter::format_source;
use std::path::Path;

/// Each language's deepest-recursing shapes, nested `n` levels.
fn shapes(n: usize) -> Vec<(&'static str, String)> {
    vec![
        (
            "deep.c",
            format!("void f(void)\n{}{}\n", "{".repeat(n), "}".repeat(n)),
        ),
        (
            "deep.c",
            format!("int x = {}1{};\n", "(".repeat(n), ")".repeat(n)),
        ),
        (
            "deep.cpp",
            format!("{}{}\n", "namespace a { ".repeat(n / 2), "}".repeat(n / 2)),
        ),
        (
            "deep.rs",
            format!("fn f() {}{}\n", "{ ".repeat(n / 2), "}".repeat(n / 2)),
        ),
        (
            "deep.rs",
            format!(
                "fn f() {{ let x = {}1{}; }}\n",
                "(".repeat(n),
                ")".repeat(n)
            ),
        ),
        (
            "deep.py",
            format!("x = {}1{}\n", "(".repeat(n), ")".repeat(n)),
        ),
    ]
}

#[test]
fn code_nested_past_the_limit_is_declined() {
    for (name, source) in shapes(3000) {
        match format_source(Path::new(name), &source, &Config::default()) {
            Err(MoldyError::TooDeep { line, .. }) => assert!(line >= 1),
            other => panic!("{name}: expected TooDeep, got {:?}", other.map(|_| ())),
        }
    }
}

#[test]
fn code_nested_as_deep_as_is_formatted_in_place_fits_a_test_thread() {
    // Trees up to 250 levels deep are formatted on the caller's thread.
    for (name, source) in shapes(240) {
        format_source(Path::new(name), &source, &Config::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn code_nested_just_inside_the_limit_is_formatted() {
    for (name, source) in shapes(1950) {
        format_source(Path::new(name), &source, &Config::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

/// `--dump-tree` of `source` read as the file `name`: its exit status, how
/// many lines it printed, and its standard error.
fn dump_tree(name: &str, source: &str) -> (Option<i32>, usize, String) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(env!("CARGO_BIN_EXE_moldy"))
        .args(["--dump-tree", "--stdin-filename", name, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("moldy starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(source.as_bytes())
        .expect("source is written");
    let output = child.wait_with_output().expect("moldy finishes");
    let lines = String::from_utf8_lossy(&output.stdout).lines().count();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status.code(), lines, stderr)
}

#[test]
fn the_tree_dump_declines_what_the_formatters_decline() {
    // Far past the limit, where a recursive walk ran out of stack.
    let source = format!("int x = {}1{};\n", "(".repeat(50_000), ")".repeat(50_000));
    let (status, lines, stderr) = dump_tree("deep.c", &source);
    assert_eq!((status, lines), (Some(3), 0));
    assert!(
        stderr.contains("deep.c: C is nested too deeply to dump (line 1); tree not dumped"),
        "{stderr}"
    );
    assert!(!stderr.contains("left unchanged"), "{stderr}");
    for (name, source) in shapes(1950) {
        let (status, lines, _) = dump_tree(name, &source);
        assert_eq!(status, Some(0), "{name}");
        assert!(lines > 1950, "{name}: {lines} lines");
    }
}

/// Output a second run would decline is not written: the file is declined
/// as not stable, exit 3, and standard input is printed as read.
#[test]
fn output_a_second_run_would_decline_is_declined_as_not_stable() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let source = format!("a: {}1{}\n", "[".repeat(100), "]".repeat(100));
    let mut child = Command::new(env!("CARGO_BIN_EXE_moldy"))
        .args(["--stdin-filename", "deep.yaml", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("moldy starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(source.as_bytes())
        .expect("source is written");
    let output = child.wait_with_output().expect("moldy finishes");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(3), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), source);
    assert!(
        stderr.contains("YAML output is not stable (output line"),
        "{stderr}"
    );
}
