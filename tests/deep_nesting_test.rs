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
