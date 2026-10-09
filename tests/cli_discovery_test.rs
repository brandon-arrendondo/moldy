//! Regression checks for substrate-backed CLI discovery and header handling.

use std::fs;
use std::process::Command;

#[test]
fn recursive_formatting_selects_only_implemented_languages() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("toolchain.toml"), "[ignore]\npaths = []\n").unwrap();
    for (name, source) in [
        ("example.c", "int main(){return 0;}"),
        ("example.cpp", "int main(){return 0;}"),
        ("example.rs", "fn main(){let x=1;}"),
        ("example.py", "x=1\n"),
        ("explicit.h", "int foo(){return 0;}"),
        ("unsupported.js", "function foo(){return 0;}"),
    ] {
        fs::write(dir.path().join(name), source).unwrap();
    }

    let output = Command::new(env!("CARGO_BIN_EXE_moldy"))
        .current_dir(dir.path())
        .args(["--in-place", "--recursive", "."])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    for name in ["example.c", "example.cpp", "example.rs", "example.py"] {
        let formatted = fs::read_to_string(dir.path().join(name)).unwrap();
        assert!(formatted.contains(" = ") || formatted.contains("return 0;\n"));
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("explicit.h")).unwrap(),
        "int foo(){return 0;}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("unsupported.js")).unwrap(),
        "function foo(){return 0;}"
    );
    let check = Command::new(env!("CARGO_BIN_EXE_moldy"))
        .current_dir(dir.path())
        .args(["--check", "--recursive", "."])
        .output()
        .unwrap();
    assert!(check.status.success(), "{:?}", check);
}

#[test]
fn explicit_c_header_is_formatted() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("toolchain.toml"), "").unwrap();
    fs::write(dir.path().join("example.h"), "int foo(){return 0;}").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_moldy"))
        .current_dir(dir.path())
        .arg("example.h")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("return 0;\n"));
}

#[test]
fn shared_and_formatter_ignores_apply_to_explicit_inputs() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("toolchain.toml"),
        "[ignore]\npaths = [\"shared.c\"]\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("moldy.toml"),
        "[ignore]\npatterns = [\"local.c\"]\n",
    )
    .unwrap();
    for name in ["shared.c", "local.c", "selected.c"] {
        fs::write(dir.path().join(name), "int foo(){return 0;}").unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_moldy"))
        .current_dir(dir.path())
        .args(["--check", "shared.c", "local.c", "selected.c"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "selected.c: would reformat\n"
    );
}
