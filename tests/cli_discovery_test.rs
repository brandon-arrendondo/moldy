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
        ("README.md", "#  Title\n"),
        ("data.json", "{\"a\":1}\n"),
        ("data.yaml", "a:   1\n"),
        ("script.sh", "if true;then echo hi;fi\n"),
        ("CMakeLists.txt", "project( x )\n"),
        ("module.cmake", "set( X 1 )\n"),
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
        fs::read_to_string(dir.path().join("data.json")).unwrap(),
        "{\n  \"a\": 1\n}\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("data.yaml")).unwrap(),
        "a: 1\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "# Title\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("CMakeLists.txt")).unwrap(),
        "project(x)\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("module.cmake")).unwrap(),
        "set(X 1)\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("script.sh")).unwrap(),
        "if true; then echo hi; fi\n"
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

/// A file the formatter declines is left unchanged and named, the run goes
/// on to the other files, and it ends incomplete (exit 3) in every mode.
#[test]
fn syntax_errors_leave_files_unchanged_and_make_the_run_incomplete() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("toolchain.toml"), "").unwrap();
    let broken = [
        ("broken.sh", "if true; then\necho  x\n"),
        ("broken.yaml", "a: [1, 2\n"),
        ("broken.json", "{\"a\": 1,}\n"),
    ];
    for (name, source) in broken {
        fs::write(dir.path().join(name), source).unwrap();
    }
    fs::write(dir.path().join("ok.json"), "{\"a\":1}\n").unwrap();

    for mode in ["--check", "--in-place"] {
        let output = Command::new(env!("CARGO_BIN_EXE_moldy"))
            .current_dir(dir.path())
            .args([mode, "broken.sh", "broken.yaml", "broken.json", "ok.json"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        for (name, source) in broken {
            assert!(
                stderr.contains(&format!("{name}: not formatted (")),
                "{mode}: {name} not named: {stderr}"
            );
            assert_eq!(fs::read_to_string(dir.path().join(name)).unwrap(), source);
        }
        // A declined file does not stop the run, but the run is incomplete.
        assert_eq!(output.status.code(), Some(3), "{output:?}");
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("ok.json")).unwrap(),
        "{\n  \"a\": 1\n}\n"
    );

    let stdout = Command::new(env!("CARGO_BIN_EXE_moldy"))
        .current_dir(dir.path())
        .arg("broken.yaml")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&stdout.stdout), "a: [1, 2\n");
}

/// Upper-case extensions, as Windows-built source trees carry them, are C:
/// `A.C` is formatted when named and found by `--recursive`, and `B.H`, like
/// any header, when named but not by the walk.
#[test]
fn upper_case_c_extensions_are_c() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("toolchain.toml"), "").unwrap();
    fs::create_dir(dir.path().join("up")).unwrap();
    let unformatted = "int foo(){return 0;}";
    fs::write(dir.path().join("up/A.C"), unformatted).unwrap();
    fs::write(dir.path().join("up/B.H"), unformatted).unwrap();
    let moldy = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_moldy"))
            .current_dir(dir.path())
            .args(args)
            .output()
            .unwrap()
    };

    // Unformatted, so a check that reads either file fails.
    for args in [["--check", "up/A.C"], ["--check", "up/B.H"]] {
        assert!(!moldy(&args).status.success(), "{args:?}");
    }
    let walk = moldy(&["--check", "--recursive", "up"]);
    assert!(!walk.status.success(), "{walk:?}");

    let output = moldy(&["--in-place", "--recursive", "up"]);
    assert!(output.status.success(), "{output:?}");
    assert!(fs::read_to_string(dir.path().join("up/A.C"))
        .unwrap()
        .contains("return 0;\n"));
    assert_eq!(
        fs::read_to_string(dir.path().join("up/B.H")).unwrap(),
        unformatted
    );
    for args in [
        ["--check", "up/A.C"].as_slice(),
        &["--check", "--recursive", "up"],
    ] {
        let check = moldy(args);
        assert!(check.status.success(), "{args:?}: {check:?}");
    }

    let output = moldy(&["--in-place", "up/B.H"]);
    assert!(output.status.success(), "{output:?}");
    assert!(moldy(&["--check", "up/B.H"]).status.success());
}

/// A `.h` or `.H` header whose content can only be C++ is formatted as C++,
/// like the same text in a `.hpp`; a header in C stays C, like a `.c`.
#[test]
fn headers_are_formatted_in_the_language_of_their_content() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("toolchain.toml"), "").unwrap();
    let cpp = "namespace ns {\nint twice(int a){return a*2;}\n}\n";
    let c = "int twice(int a){return a*2;}\nstruct s{int x;};\n";
    for (name, source) in [
        ("d.h", cpp),
        ("D.H", cpp),
        ("ref.hpp", cpp),
        ("b.h", c),
        ("B.H", c),
        ("ref.c", c),
    ] {
        fs::write(dir.path().join(name), source).unwrap();
    }
    let format = |name: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_moldy"))
            .current_dir(dir.path())
            .arg(name)
            .output()
            .unwrap();
        assert!(output.status.success(), "{name}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    };

    let as_cpp = format("ref.hpp");
    assert!(as_cpp.starts_with("namespace ns\n{\n"), "{as_cpp}");
    assert_eq!(format("d.h"), as_cpp);
    assert_eq!(format("D.H"), as_cpp);
    let as_c = format("ref.c");
    assert_eq!(format("b.h"), as_c);
    assert_eq!(format("B.H"), as_c);
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
        "selected.c: would reformat\n\
         moldy: 1 file: 1 would be reformatted, 0 unchanged, 0 not formatted\n"
    );
}
