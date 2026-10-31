//! Exit statuses and run reporting (docs/adr/0003): 0 completed, 1 `--check`
//! found differences, 2 the run could not start, 3 the run was incomplete.
//! Every case has its own CLI regression here.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const FORMATTED: &str = "int g(void)\n{\n    return 0;\n}\n";
const UNFORMATTED: &str = "int f(){return 0;}\n";

/// A directory with an empty toolchain.toml, so no ancestor's ignores apply.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("toolchain.toml"), "").unwrap();
    dir
}

fn moldy(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_moldy"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Runs `args` with `--completion-report` and returns the report.
fn with_report(dir: &Path, args: &[&str]) -> (Output, serde_json::Value) {
    let report = dir.join("report.json");
    let mut all = args.to_vec();
    all.extend(["--completion-report", report.to_str().unwrap()]);
    let output = moldy(dir, &all);
    let text = fs::read_to_string(&report).unwrap();
    (output, serde_json::from_str(&text).unwrap())
}

#[test]
fn a_clean_check_exits_0_and_says_so() {
    let dir = project();
    fs::write(dir.path().join("g.c"), FORMATTED).unwrap();
    let output = moldy(dir.path(), &["--check", "g.c"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        stderr(&output),
        "moldy: 1 file: 0 would be reformatted, 1 unchanged, 0 not formatted\n"
    );
}

#[test]
fn a_check_with_differences_exits_1() {
    let dir = project();
    fs::write(dir.path().join("f.c"), UNFORMATTED).unwrap();
    fs::write(dir.path().join("g.c"), FORMATTED).unwrap();
    let output = moldy(dir.path(), &["--check", "f.c", "g.c"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "f.c: would reformat\n\
         moldy: 2 files: 1 would be reformatted, 1 unchanged, 0 not formatted\n"
    );
}

#[test]
fn in_place_names_each_rewritten_file() {
    let dir = project();
    fs::write(dir.path().join("f.c"), UNFORMATTED).unwrap();
    fs::write(dir.path().join("g.c"), FORMATTED).unwrap();
    let output = moldy(dir.path(), &["-i", "f.c", "g.c"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stderr(&output),
        "f.c: reformatted\n\
         moldy: 2 files: 1 reformatted, 1 unchanged, 0 not formatted\n"
    );
}

#[test]
fn a_complete_stdout_run_prints_no_summary() {
    let dir = project();
    fs::write(dir.path().join("f.c"), UNFORMATTED).unwrap();
    let output = moldy(dir.path(), &["f.c"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "{}", stderr(&output));
}

#[test]
fn a_run_that_cannot_start_exits_2_and_writes_nothing() {
    let dir = project();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/f.c"), UNFORMATTED).unwrap();
    fs::write(dir.path().join("f.c"), UNFORMATTED).unwrap();
    fs::write(dir.path().join("bad.toml"), "[ignore\n").unwrap();
    for (args, message) in [
        (
            vec!["-i", "f.c", "src"],
            "src is a directory; pass -r to recurse",
        ),
        (vec!["-i", "f.c", "gone.c"], "gone.c does not exist"),
        (vec!["-i", "--config", "bad.toml", "f.c"], "bad.toml"),
        (vec!["-i", "--preset", "nosuch", "f.c"], "unknown preset"),
        (vec!["--check", "-i", "f.c"], "cannot be used with"),
    ] {
        let output = moldy(dir.path(), &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(
            stderr(&output).contains(message),
            "{args:?}: {}",
            stderr(&output)
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("f.c")).unwrap(),
            UNFORMATTED
        );
    }
}

#[test]
fn an_unknown_preset_is_named_as_a_preset() {
    let dir = project();
    fs::write(dir.path().join("f.c"), UNFORMATTED).unwrap();
    let output = moldy(dir.path(), &["--preset", "nosuch", "f.c"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = stderr(&output);
    assert!(
        stderr.contains("unknown preset 'nosuch' (available: "),
        "{stderr}"
    );
    assert!(!stderr.contains("unsupported language"), "{stderr}");
}

#[test]
fn an_empty_selection_exits_3_unless_allowed() {
    let dir = project();
    fs::create_dir(dir.path().join("empty")).unwrap();
    let (output, report) = with_report(dir.path(), &["--check", "-r", "empty"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(stderr(&output).contains("pass --allow-empty"));
    assert_eq!(report["complete"], false);
    let allowed = moldy(dir.path(), &["--check", "-r", "empty", "--allow-empty"]);
    assert_eq!(allowed.status.code(), Some(0));
    assert_eq!(stderr(&allowed), "moldy: no files selected\n");
}

#[test]
fn an_empty_source_file_is_valid_input() {
    let dir = project();
    fs::write(dir.path().join("empty.c"), "").unwrap();
    let output = moldy(dir.path(), &["--check", "empty.c"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn a_file_that_cannot_be_read_is_contained_and_the_run_exits_3() {
    let dir = project();
    fs::write(dir.path().join("f.c"), UNFORMATTED).unwrap();
    fs::write(dir.path().join("bin.c"), b"\xff\xfe\n").unwrap();
    let output = moldy(dir.path(), &["-i", "bin.c", "f.c"]);
    assert_eq!(output.status.code(), Some(3));
    let err = stderr(&output);
    assert!(
        err.contains("bin.c: not formatted (not valid UTF-8)"),
        "{err}"
    );
    assert!(err.contains("f.c: reformatted"), "{err}");
    assert!(
        err.contains("incomplete run: 1 of 2 selected file(s)"),
        "{err}"
    );
    assert!(
        err.contains("stay rewritten; nothing is rolled back"),
        "{err}"
    );
    assert_ne!(
        fs::read_to_string(dir.path().join("f.c")).unwrap(),
        UNFORMATTED
    );
}

#[test]
fn a_named_file_in_an_unsupported_language_exits_3() {
    let dir = project();
    fs::write(dir.path().join("notes.txt"), "x\n").unwrap();
    let output = moldy(dir.path(), &["--check", "notes.txt"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(stderr(&output).contains("notes.txt: not formatted (unsupported language)"));
}

#[test]
fn an_ignored_named_file_is_not_selected() {
    let dir = project();
    fs::write(
        dir.path().join("moldy.toml"),
        "[ignore]\npatterns = [\"*.txt\"]\n",
    )
    .unwrap();
    fs::write(dir.path().join("notes.txt"), "x\n").unwrap();
    fs::write(dir.path().join("g.c"), FORMATTED).unwrap();
    let output = moldy(dir.path(), &["--check", "notes.txt", "g.c"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn a_declined_file_exits_3_even_beside_a_difference() {
    let dir = project();
    fs::write(dir.path().join("f.c"), UNFORMATTED).unwrap();
    fs::write(dir.path().join("broken.yaml"), "a: [1, 2\n").unwrap();
    let output = moldy(dir.path(), &["--check", "f.c", "broken.yaml"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(stderr(&output).contains("broken.yaml: not formatted (YAML syntax error at line 1)"));
}

#[test]
fn parser_recovery_in_c_is_not_a_failure() {
    let dir = project();
    let source = "int  g(void){return 1;}\nint f(void)\n{\n  return 1\n}\n";
    fs::write(dir.path().join("rec.c"), source).unwrap();
    let output = moldy(dir.path(), &["--check", "rec.c"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    // The statement the parser could not read is kept as written: no `;`
    // is added to it.
    let output = moldy(dir.path(), &["rec.c"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("\n  return 1\n}"));
}

#[test]
fn output_that_would_change_the_code_exits_3_and_is_left_unchanged() {
    let dir = project();
    // The emitter would move the comment between the declarator and its
    // `{` into the body, so the check declines the file.
    let source = "int f(void)\n/* c */\n{\n    return  0;\n}\n";
    fs::write(dir.path().join("d.c"), source).unwrap();
    let output = moldy(dir.path(), &["d.c"]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), source);
    assert!(stderr(&output).contains(
        "d.c: not formatted (C could not be formatted without changing its meaning (line 2))"
    ));
    let output = moldy(dir.path(), &["--in-place", "d.c"]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert_eq!(fs::read_to_string(dir.path().join("d.c")).unwrap(), source);
}

#[test]
fn the_completion_report_records_every_outcome() {
    let dir = project();
    fs::write(dir.path().join("f.c"), UNFORMATTED).unwrap();
    fs::write(dir.path().join("g.c"), FORMATTED).unwrap();
    fs::write(dir.path().join("broken.yaml"), "a: [1, 2\n").unwrap();
    fs::write(dir.path().join("bin.c"), b"\xff\xfe\n").unwrap();
    let args = ["-i", "f.c", "g.c", "broken.yaml", "bin.c"];
    let (output, report) = with_report(dir.path(), &args);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(report["complete"], false);
    assert_eq!(report["mode"], "in-place");
    assert_eq!(
        report["files"],
        serde_json::json!({"selected": 4, "changed": 1, "unchanged": 1, "declined": 1, "failed": 1})
    );
    let statuses: Vec<(&str, &str)> = report["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["file"].as_str().unwrap(), r["status"].as_str().unwrap()))
        .collect();
    assert_eq!(
        statuses,
        [
            ("bin.c", "failed"),
            ("broken.yaml", "declined"),
            ("f.c", "changed"),
            ("g.c", "unchanged")
        ]
    );
}

#[test]
fn a_complete_run_reports_complete() {
    let dir = project();
    fs::write(dir.path().join("g.c"), FORMATTED).unwrap();
    let (output, report) = with_report(dir.path(), &["--check", "g.c"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(report["complete"], true);
    assert_eq!(report["mode"], "check");
}

#[cfg(unix)]
#[test]
fn what_the_walk_cannot_reach_is_failed() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = project();
    let w = dir.path().join("w");
    fs::create_dir_all(w.join("locked")).unwrap();
    fs::write(w.join("ok.c"), FORMATTED).unwrap();
    fs::write(w.join("locked/h.c"), FORMATTED).unwrap();
    symlink("nowhere.c", w.join("dangling.c")).unwrap();
    symlink("nowhere.txt", w.join("dangling.txt")).unwrap();
    let mode = |m| fs::set_permissions(w.join("locked"), fs::Permissions::from_mode(m)).unwrap();
    mode(0o000);
    // Root can still read the locked directory, so then it is not lost.
    let readable = fs::read_dir(w.join("locked")).is_ok();
    let (output, report) = with_report(dir.path(), &["--check", "-r", "w"]);
    mode(0o755);

    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let failed: Vec<&str> = report["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["status"] == "failed")
        .map(|r| r["file"].as_str().unwrap())
        .collect();
    let mut expected = vec!["w/dangling.c"];
    if !readable {
        expected.push("w/locked");
    }
    assert_eq!(failed, expected);
}

/// The published hook's own entry, as pre-commit runs it: on a batch of
/// staged files that moldy.toml ignores, it selects none and passes.
#[test]
fn the_published_hook_passes_an_all_ignored_batch() {
    let hooks = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.pre-commit-hooks.yaml"
    ))
    .unwrap();
    let entry = hooks
        .lines()
        .find_map(|l| l.trim().strip_prefix("entry: "))
        .unwrap();
    let mut words = entry.split_whitespace();
    assert_eq!(words.next(), Some("moldy"));
    let mut args: Vec<&str> = words.collect();
    let dir = project();
    fs::write(
        dir.path().join("moldy.toml"),
        "[ignore]\npatterns = [\"vendor/**\"]\n",
    )
    .unwrap();
    fs::create_dir(dir.path().join("vendor")).unwrap();
    fs::write(dir.path().join("vendor/v.c"), UNFORMATTED).unwrap();
    args.push("vendor/v.c");
    let output = moldy(dir.path(), &args);
    assert_eq!(output.status.code(), Some(0), "{entry}: {output:?}");
    assert_eq!(
        fs::read_to_string(dir.path().join("vendor/v.c")).unwrap(),
        UNFORMATTED
    );
}

#[cfg(unix)]
#[test]
fn a_named_file_behind_an_unsearchable_directory_is_failed_not_missing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("a.c"), FORMATTED).unwrap();
    let mode = |m| fs::set_permissions(&locked, fs::Permissions::from_mode(m)).unwrap();
    mode(0o000);
    // Root can still search the directory, so the case cannot arise there.
    let searchable = locked.join("a.c").try_exists().is_ok();
    let output = moldy(dir.path(), &["--check", "locked/a.c"]);
    mode(0o755);
    if searchable {
        return;
    }
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(
        stderr(&output).contains("locked/a.c: not formatted ("),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_run_that_cannot_start_leaves_no_stale_report() {
    let dir = project();
    let report = dir.path().join("report.json");
    fs::write(&report, "{\"complete\": true}\n").unwrap();
    let output = moldy(
        dir.path(),
        &[
            "--check",
            "gone.c",
            "--completion-report",
            report.to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(!report.exists());
}

#[test]
fn a_file_named_twice_is_counted_once() {
    let dir = project();
    fs::write(dir.path().join("g.c"), FORMATTED).unwrap();
    let output = moldy(dir.path(), &["--check", "g.c", "g.c"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stderr(&output).starts_with("moldy: 1 file:"),
        "{}",
        stderr(&output)
    );
}

/// Runs `args` with `input` on standard input.
fn moldy_stdin(dir: &Path, args: &[&str], input: &[u8]) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_moldy"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A usage error exits before reading stdin, so the write can meet a
    // closed pipe; the exit status is what the test checks.
    let _ = child.stdin.take().unwrap().write_all(input);
    child.wait_with_output().unwrap()
}

#[test]
fn standard_input_is_formatted_in_the_language_its_name_gives() {
    let dir = project();
    let args = ["-", "--stdin-filename", "src/f.c"];
    let output = moldy_stdin(dir.path(), &args, UNFORMATTED.as_bytes());
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "int f()\n{\n    return 0;\n}\n"
    );
    assert_eq!(stderr(&output), "");
}

#[test]
fn standard_input_that_cannot_be_formatted_exits_3() {
    let dir = project();
    let args = ["-", "--stdin-filename", "a.yaml"];
    let output = moldy_stdin(dir.path(), &args, b"a: [\n");
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "a: [\n");
    assert!(stderr(&output).starts_with("a.yaml: not formatted (YAML syntax error"));

    let output = moldy_stdin(dir.path(), &["-", "--stdin-filename", "a.c"], b"\xff\n");
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(stderr(&output).starts_with("a.c: not formatted (not valid UTF-8)"));

    let output = moldy_stdin(dir.path(), &["-", "--stdin-filename", "a.txt"], b"text\n");
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(stderr(&output).starts_with("a.txt: not formatted (unsupported language)"));
}

#[test]
fn standard_input_under_an_ignored_name_is_printed_as_read() {
    let dir = project();
    fs::write(
        dir.path().join("moldy.toml"),
        "[ignore]\npatterns = [\"gen/*\"]\n",
    )
    .unwrap();
    let args = ["-", "--stdin-filename", "gen/f.c", "--allow-empty"];
    let output = moldy_stdin(dir.path(), &args, UNFORMATTED.as_bytes());
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), UNFORMATTED);
    let output = moldy_stdin(dir.path(), &args[..3], UNFORMATTED.as_bytes());
    assert_eq!(output.status.code(), Some(3), "{output:?}");
}

#[test]
fn standard_input_used_wrongly_is_a_usage_error() {
    let dir = project();
    fs::write(dir.path().join("g.c"), FORMATTED).unwrap();
    let cases: [&[&str]; 5] = [
        &["-"],
        &["--stdin-filename", "f.c", "g.c"],
        &["-", "g.c", "--stdin-filename", "f.c"],
        &["-i", "-", "--stdin-filename", "f.c"],
        &["--check", "-", "--stdin-filename", "f.c"],
    ];
    for args in cases {
        let output = moldy_stdin(dir.path(), args, UNFORMATTED.as_bytes());
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
    }
}
