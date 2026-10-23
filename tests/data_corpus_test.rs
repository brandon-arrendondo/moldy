// Corpus checks for the data, markup and build-file formatters. Each
// `tests/<lang>_corpus/` directory holds inputs next to `<input>.expected`
// files; every input must format to its expected file, and every expected
// file must format to itself (idempotency).
//
// `json_corpus_matches_json_dumps_layout` (ignored by default) shells out to
// `python3` to confirm each comment-free `.json` fixture's expected file has
// the same values as its input and the same line-by-line indentation as
// `json.dumps(indent=2)`, and that a malformed input is passed through.
//
// `yaml_corpus_matches_prettier` and `markdown_corpus_matches_prettier`
// (ignored) run `prettier` from PATH over every fixture except the known
// deviations. `yaml_corpus_preserves_parse_events` (ignored) uses python3's
// PyYAML to confirm each expected file parses to the same anchors, tags,
// scalar values and structure as its input, and
// `markdown_corpus_preserves_rendering` (ignored) uses python3's
// markdown-it-py to confirm each expected file renders to the same HTML as
// its input. `cmake_corpus_matches_gersemi` (ignored) runs `gersemi` from
// PATH, and `cmake_corpus_preserves_arguments` re-parses each CMake input
// and expected file to confirm every command keeps its arguments and
// comments. `bash_corpus_preserves_tokens` does the same for shell scripts,
// `bash_corpus_keeps_meaning` compares bash's own printout of each script
// before and after (when bash 5.2 or later is installed), and
// `bash_corpus_matches_prettier` (ignored) runs `prettier` with the
// shell plugin named by `PRETTIER_PLUGIN_SH` (a path to prettier-plugin-sh,
// which Prettier resolves only relative to where it is installed).
// `rst_corpus_keeps_meaning` (ignored) reads each reStructuredText input and
// expected file with python3's docutils and compares the document trees, and
// `rst_corpus_matches_docstrfmt` (ignored) runs `docstrfmt` from PATH over
// every fixture except the known deviations. Run the ignored checks with
// `cargo test --test data_corpus_test -- --ignored`.

use moldy::config::{Config, RstStyle};
use moldy::error::MoldyError;
use moldy::formatter::format_source;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn corpus_inputs(dir: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(dir);
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} missing: {e}", dir.display()))
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) != Some("expected"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} has no inputs", dir.display());
    files
}

fn expected_path(input: &Path) -> PathBuf {
    let mut name = input.file_name().unwrap().to_os_string();
    name.push(".expected");
    input.with_file_name(name)
}

/// Formats `text` as the CLI does: a file with a syntax error, or one the
/// grammar misreads or the formatter cannot keep, is left unchanged. Only
/// fixtures named `malformed*`, `grammar_error*` or `refused*` may have
/// one.
fn format_or_keep(path: &Path, text: &str) -> String {
    match format_source(path, text, &Config::default()) {
        Ok(formatted) => formatted,
        Err(MoldyError::Syntax { .. } | MoldyError::Unverified { .. })
            if is_unparsed_fixture(path) =>
        {
            text.to_string()
        }
        Err(e) => panic!("format_source failed for {}: {e}", path.display()),
    }
}

fn is_unparsed_fixture(path: &Path) -> bool {
    let name = file_name(path);
    ["malformed", "grammar_error", "refused"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn check_corpus(dir: &str) {
    let mut failures = Vec::new();
    for input in corpus_inputs(dir) {
        let source = fs::read_to_string(&input).unwrap();
        let expected = fs::read_to_string(expected_path(&input))
            .unwrap_or_else(|e| panic!("missing expected for {}: {e}", input.display()));
        let formatted = format_or_keep(&input, &source);
        if formatted != expected {
            failures.push(format!("{} (expected mismatch)", input.display()));
        }
        // Format the expected text under the input's own name so file-name
        // dispatch (e.g. `CMakeLists.txt`) applies.
        let again = format_or_keep(&input, &expected);
        if again != expected {
            failures.push(format!("{} (not idempotent)", input.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn json_corpus_matches_expected() {
    check_corpus("json_corpus");
}

const JSON_LAYOUT_CHECK: &str = r#"
import json, sys
src, exp = (open(p, encoding="utf-8").read() for p in sys.argv[1:3])
try:
    value = json.loads(src)
except ValueError:
    assert src == exp, "malformed input was not passed through unchanged"
    sys.exit(0)
assert value == json.loads(exp), "values changed"
ref = json.dumps(json.loads(exp), indent=2, ensure_ascii=False).split("\n")
got = exp.rstrip("\n").split("\n")
indent = lambda lines: [len(l) - len(l.lstrip(" ")) for l in lines]
assert indent(ref) == indent(got), "layout differs from json.dumps(indent=2)"
"#;

#[test]
#[ignore = "requires python3"]
fn json_corpus_matches_json_dumps_layout() {
    for input in corpus_inputs("json_corpus") {
        if input.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let output = Command::new("python3")
            .arg("-c")
            .arg(JSON_LAYOUT_CHECK)
            .arg(&input)
            .arg(expected_path(&input))
            .output()
            .expect("python3 not found");
        assert!(
            output.status.success(),
            "{}: {}",
            input.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn yaml_corpus_matches_expected() {
    check_corpus("yaml_corpus");
}

fn file_name(input: &Path) -> &str {
    input.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

/// Every fixture in `dir` except `deviations` and unparsed input, which is
/// returned unchanged, must match `prettier` run with `args`.
fn check_prettier(dir: &str, deviations: &[&str], args: &[&str]) {
    for input in corpus_inputs(dir) {
        if deviations.contains(&file_name(&input)) || is_unparsed_fixture(&input) {
            continue;
        }
        let output = Command::new("prettier")
            .args(args)
            .arg(&input)
            .output()
            .expect("prettier not found");
        assert!(output.status.success(), "{:?}", output);
        let expected = fs::read_to_string(expected_path(&input)).unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            expected,
            "{} differs from prettier",
            input.display()
        );
    }
}

/// YAML fixtures whose expected output knowingly differs from Prettier's:
/// the verbatim fallbacks for explicit keys and commented flow collections,
/// a block scalar's last lines of spaces or tabs, which are content that
/// Prettier drops, a scalar that is a whole document, which stays on its
/// `---` line, repeated `...` markers, which Prettier prints as an empty
/// document, a `...` with a comment, which Prettier keeps, and malformed
/// input, which is returned unchanged.
const YAML_PRETTIER_DEVIATIONS: &[&str] = &[
    "verbatim_fallbacks.yaml",
    "block_scalar_whitespace.yaml",
    "block_scalar_documents.yaml",
    "document_end_repeated.yaml",
    "document_end_comment.yaml",
    "malformed.yaml",
];

#[test]
#[ignore = "requires prettier on PATH"]
fn yaml_corpus_matches_prettier() {
    check_prettier("yaml_corpus", YAML_PRETTIER_DEVIATIONS, &[]);
}

const YAML_EVENTS_CHECK: &str = r#"
import sys, yaml
def events(path):
    out = []
    for e in yaml.parse(open(path, encoding="utf-8")):
        if isinstance(e, (yaml.DocumentStartEvent, yaml.DocumentEndEvent)):
            continue  # explicit `---` / `...` markers are layout
        out.append((type(e).__name__, getattr(e, "anchor", None),
                    getattr(e, "tag", None), getattr(e, "value", None)))
    return out
assert events(sys.argv[1]) == events(sys.argv[2]), "parse events differ"
"#;

#[test]
#[ignore = "requires python3 with PyYAML"]
fn yaml_corpus_preserves_parse_events() {
    for input in corpus_inputs("yaml_corpus") {
        // PyYAML (YAML 1.1) rejects a document after `...` without `---`.
        if ["malformed.yaml", "document_end_before_bare_document.yaml"].contains(&file_name(&input))
        {
            continue;
        }
        let output = Command::new("python3")
            .arg("-c")
            .arg(YAML_EVENTS_CHECK)
            .arg(&input)
            .arg(expected_path(&input))
            .output()
            .expect("python3 not found");
        assert!(
            output.status.success(),
            "{}: {}",
            input.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn markdown_corpus_matches_expected() {
    check_corpus("markdown_corpus");
}

/// Input the grammar reads differently from CommonMark or GFM, where
/// formatting could change the rendering, is left unchanged with an error.
#[test]
fn markdown_misreads_are_left_unchanged() {
    for source in [
        // A fence the grammar closes on a line indented 4 columns.
        "~~~\n    ~~~c\n    code\n    ~~~\n~~~\n",
        // A fence the grammar closes on text ending in backticks.
        "- item:\n    ```\n    @code```\n- next\n",
        // A table cell with a `|` inside math, which GFM splits on.
        "| a | b |\n|---|---|\n| x | \\( \\left|y\\right| \\) |\n",
        // A sub-list under a lazily continued line, before another list.
        "Para\n3. **Push.** done\n   - a\n- b\n",
        "Para\n3. **Push.** done\n   - a\n* b\n",
        // A row indented 4 columns, which ends the table in GFM.
        "- | a | b |\n  |---|---|\n  | x | y |\n        z\n",
    ] {
        let result = format_source(Path::new("doc.md"), source, &Config::default());
        assert!(
            matches!(result, Err(MoldyError::Unverified { .. })),
            "{source:?}: {result:?}"
        );
    }
}

#[test]
#[ignore = "requires prettier on PATH"]
fn markdown_corpus_matches_prettier() {
    // Escapes, JSON fences, code spans that wrap inside list items, and a
    // quote after a closed fence (CommonMark, which Prettier's parser
    // predates) knowingly differ.
    check_prettier(
        "markdown_corpus",
        &[
            "prettier_deviations.md",
            "emphasis_pairing.md",
            "lazy_ordered_list.md",
            "rendering_quirks.md",
        ],
        &["--prose-wrap", "preserve"],
    );
}

const MARKDOWN_HTML_CHECK: &str = r#"
import re, sys
from markdown_it import MarkdownIt
md = MarkdownIt("commonmark").enable(["table", "strikethrough"])
def html(path):
    text = open(path, encoding="utf-8").read()
    # Front matter is formatted as YAML, which is not Markdown rendering.
    text = re.sub(r"\A---\n.*?\n---\n", "", text, flags=re.S)
    out = re.sub(r"\s+", " ", md.render(text))
    out = re.sub(r" ?(<[^>]+>) ?", r"\1", out)
    # `***x***` is printed as `**_x_**`: the same text, nested the other way.
    return re.sub(r"<em><strong>(.*?)</strong></em>", r"<strong><em>\1</em></strong>", out)
assert html(sys.argv[1]) == html(sys.argv[2]), "rendered HTML differs"
"#;

#[test]
#[ignore = "requires python3 with markdown-it-py"]
fn markdown_corpus_preserves_rendering() {
    for input in corpus_inputs("markdown_corpus") {
        let output = Command::new("python3")
            .arg("-c")
            .arg(MARKDOWN_HTML_CHECK)
            .arg(&input)
            .arg(expected_path(&input))
            .output()
            .expect("python3 not found");
        assert!(
            output.status.success(),
            "{}: {}",
            input.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn cmake_corpus_matches_expected() {
    check_corpus("cmake_corpus");
}

/// Each command as `name(arg|arg|...)` and each comment, in order. Command
/// names are compared without case, and arguments CMake reads as one
/// (`A="${B}"`, with nothing between the parts) are joined.
fn cmake_commands(source: &str) -> Vec<String> {
    fn walk(node: tree_sitter::Node, src: &str, out: &mut Vec<String>) {
        let text = |n: tree_sitter::Node| src[n.start_byte()..n.end_byte()].trim_end().to_string();
        if matches!(node.kind(), "line_comment" | "bracket_comment") {
            out.push(text(node));
            return;
        }
        if node.kind().ends_with("_command") && node.child_count() > 0 {
            let mut leaves = Vec::new();
            let mut stack = vec![node];
            while let Some(n) = stack.pop() {
                let paren =
                    !n.is_named() && matches!(n.kind(), "(" | ")") && n.parent() != Some(node);
                if paren || matches!(n.kind(), "argument" | "line_comment" | "bracket_comment") {
                    leaves.push(n);
                    continue;
                }
                let mut cursor = n.walk();
                let kids: Vec<_> = n.children(&mut cursor).collect();
                stack.extend(kids.into_iter().rev());
            }
            let mut args: Vec<String> = Vec::new();
            let mut prev_end = None;
            for leaf in leaves {
                match args.last_mut() {
                    Some(last)
                        if leaf.kind() == "argument" && prev_end == Some(leaf.start_byte()) =>
                    {
                        last.push_str(&text(leaf));
                    }
                    _ => args.push(text(leaf)),
                }
                prev_end = (leaf.kind() == "argument").then(|| leaf.end_byte());
            }
            let name = text(node.child(0).unwrap()).to_ascii_lowercase();
            out.push(format!("{name}({})", args.join("|")));
            return;
        }
        let mut cursor = node.walk();
        for kid in node.children(&mut cursor) {
            walk(kid, src, out);
        }
    }
    let source = source.replace("\r\n", "\n");
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang_parsing_substrate::language_for_key("cmake").unwrap())
        .unwrap();
    let tree = parser.parse(&source, None).unwrap();
    let mut out = Vec::new();
    walk(tree.root_node(), &source, &mut out);
    out
}

#[test]
fn cmake_corpus_preserves_arguments() {
    for input in corpus_inputs("cmake_corpus") {
        let source = fs::read_to_string(&input).unwrap();
        let expected = fs::read_to_string(expected_path(&input)).unwrap();
        assert_eq!(
            cmake_commands(&source),
            cmake_commands(&expected),
            "{} changes a command's arguments",
            input.display()
        );
    }
}

#[test]
#[ignore = "requires gersemi on PATH"]
fn cmake_corpus_matches_gersemi() {
    // `install(TARGETS ... DESTINATION)` and a flag after a path list
    // knowingly differ; unparsed input is returned unchanged.
    for input in corpus_inputs("cmake_corpus") {
        if file_name(&input) == "gersemi_deviations.cmake" || is_unparsed_fixture(&input) {
            continue;
        }
        let output = Command::new("gersemi")
            .arg(&input)
            .output()
            .expect("gersemi not found");
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            fs::read_to_string(expected_path(&input)).unwrap(),
            "{} differs from gersemi",
            input.display()
        );
    }
}

#[test]
fn bash_corpus_matches_expected() {
    check_corpus("bash_corpus");
}

/// A script's tokens in order, without statement separators or the
/// delimiters of command substitutions (backticks become `$( )`), followed
/// by its comments sorted (a comment may move past a keyword). The optional
/// `(` before a case pattern, and a pattern's unescaped trailing blank, are
/// left out too.
fn bash_tokens(source: &str) -> Vec<String> {
    fn walk(node: tree_sitter::Node, src: &str, out: &mut Vec<String>) {
        let leaf = node.child_count() == 0
            || (matches!(
                node.kind(),
                "heredoc_body" | "string_content" | "raw_string" | "word"
            ) && node.named_child_count() == 0);
        if leaf {
            let text = &src[node.start_byte()..node.end_byte()];
            let parent = node.parent().map(|p| p.kind());
            let skip = matches!(node.kind(), ";" | ";;" | "\n")
                || (parent == Some("command_substitution") && matches!(text, "`" | "$(" | ")"))
                || (parent == Some("case_item") && text == "(");
            if !skip && !text.is_empty() {
                let text = match node.kind() {
                    "comment" => text.trim_end(),
                    // The grammar can take the blank after `\\` into a
                    // pattern; a blank escaped by `\` is part of it.
                    "extglob_pattern" => {
                        let mut text = text;
                        while let Some(rest) = text.strip_suffix([' ', '\t']) {
                            if (rest.len() - rest.trim_end_matches('\\').len()) % 2 == 1 {
                                break;
                            }
                            text = rest;
                        }
                        text
                    }
                    _ => text,
                };
                out.push(text.to_string());
            }
            return;
        }
        let mut cursor = node.walk();
        for kid in node.children(&mut cursor) {
            walk(kid, src, out);
        }
    }
    let source = source.replace("\r\n", "\n");
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang_parsing_substrate::language_for_key("bash").unwrap())
        .unwrap();
    let tree = parser.parse(&source, None).unwrap();
    let mut out = Vec::new();
    walk(tree.root_node(), &source, &mut out);
    let (mut comments, mut code): (Vec<String>, Vec<String>) =
        out.into_iter().partition(|t| t.starts_with('#'));
    comments.sort();
    code.extend(comments);
    code
}

#[test]
fn bash_corpus_preserves_tokens() {
    for input in corpus_inputs("bash_corpus") {
        let source = fs::read_to_string(&input).unwrap();
        let expected = fs::read_to_string(expected_path(&input)).unwrap();
        assert_eq!(
            bash_tokens(&source),
            bash_tokens(&expected),
            "{} changes a token",
            input.display()
        );
    }
}

/// Bash's own printout of a script, read as the body of a function
/// (`declare -f`), which normalises layout but keeps meaning. Backticks
/// become `$( )` and blanks inside `(( ))` are dropped, as the formatter
/// does. `None` when bash is not installed or rejects the script.
fn bash_printout(source: &str) -> Option<String> {
    let script = format!("__moldy() {{\n{source}\n}}\ndeclare -f __moldy\n");
    let output = Command::new("bash")
        .args(["--norc", "--noprofile", "-c", &script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let printed = String::from_utf8_lossy(&output.stdout);
    let mut out = String::new();
    let mut backtick = false;
    let mut arith = 0;
    let mut chars = printed.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                out.push_str(if backtick { ")" } else { "$(" });
                backtick = !backtick;
            }
            // Bash prints subshells as `( (`, so `((` is arithmetic.
            '(' | ')' if chars.peek() == Some(&c) => {
                chars.next();
                out.push(c);
                out.push(c);
                arith += if c == '(' { 1 } else { -1 };
            }
            ' ' | '\t' if arith > 0 => {}
            _ => out.push(c),
        }
    }
    Some(out)
}

#[test]
fn bash_corpus_keeps_meaning() {
    let Some(probe) = bash_printout("x=$(a  2>/dev/null)") else {
        eprintln!("bash not installed; skipping");
        return;
    };
    // Bash before 5.2 prints a `$( )` body as written, so its printout
    // would count layout inside one as a change of meaning.
    if !probe.contains("$(a 2> /dev/null)") {
        eprintln!("bash prints command substitutions as written (before 5.2); skipping");
        return;
    }
    for input in corpus_inputs("bash_corpus") {
        let source = fs::read_to_string(&input).unwrap();
        let Some(before) = bash_printout(&source) else {
            continue;
        };
        let expected = fs::read_to_string(expected_path(&input)).unwrap();
        assert_eq!(
            Some(before),
            bash_printout(&expected),
            "{} changes what bash reads",
            input.display()
        );
    }
}

#[test]
#[ignore = "requires prettier on PATH and PRETTIER_PLUGIN_SH"]
fn bash_corpus_matches_prettier() {
    let plugin = std::env::var("PRETTIER_PLUGIN_SH").expect("PRETTIER_PLUGIN_SH not set");
    let arg = format!("--plugin={plugin}");
    check_prettier(
        "bash_corpus",
        &[
            "prettier_deviations.sh",
            "kept_as_written.sh",
            "malformed.sh",
        ],
        &[arg.as_str()],
    );
}

/// Nesting past each formatter's limit is refused with a warning, not a
/// crash, and nesting just inside it formats. Test threads have small
/// stacks, so this also checks the limits leave room for recursion.
#[test]
fn deep_nesting_is_left_unchanged() {
    let deep = [
        (
            "deep.json",
            format!("{}1{}", "[".repeat(2000), "]".repeat(2000)),
        ),
        (
            "deep.yaml",
            format!("a: {}1{}\n", "[".repeat(1000), "]".repeat(1000)),
        ),
        (
            "deep.yaml",
            (0..300)
                .map(|i| format!("{}a:\n", " ".repeat(i)))
                .collect::<String>()
                + &" ".repeat(300)
                + "b: 1\n",
        ),
        ("deep.md", format!("{} x\n", ">".repeat(300))),
        (
            "deep.md",
            format!("{}x{}\n", "*".repeat(20000), "*".repeat(20000)),
        ),
        (
            "deep.md",
            format!("{}a{}\n\n", "*".repeat(31), "*".repeat(31)).repeat(600),
        ),
        ("deep.md", format!("{}x\n\n", "`a ".repeat(3999)).repeat(2)),
        (
            "deep.md",
            (0..300)
                .map(|i| format!("{}- a\n", "  ".repeat(i)))
                .collect(),
        ),
        (
            "CMakeLists.txt",
            format!("set(A {}a{})\n", "(".repeat(200), ")".repeat(200)),
        ),
        (
            "deep.sh",
            format!("{}x{}\n", "{ ".repeat(1000), "; }".repeat(1000)),
        ),
        (
            "deep.sh",
            format!("echo {}x{}\n", "$(".repeat(1000), ")".repeat(1000)),
        ),
        (
            "deep.rst",
            (0..300)
                .map(|i| format!("{}- a\n\n", "  ".repeat(i)))
                .collect(),
        ),
        (
            "deep.rst",
            (0..40)
                .map(|i| format!("{}.. note::\n\n", "    ".repeat(i)))
                .collect::<String>()
                + "x\n",
        ),
    ];
    for (name, source) in &deep {
        match format_source(Path::new(name), source, &Config::default()) {
            Err(MoldyError::TooDeep { .. }) => {}
            other => panic!("{name}: expected TooDeep, got {:?}", other.map(|_| ())),
        }
    }
    let within = [
        (
            "deep.json",
            format!("{}1{}", "[".repeat(150), "]".repeat(150)),
        ),
        (
            "deep.yaml",
            format!("a: {}1{}\n", "[".repeat(100), "]".repeat(100)),
        ),
        ("deep.md", format!("{} x\n", "> ".repeat(100))),
        (
            "CMakeLists.txt",
            format!("set(A {}a # c\n{})\n", "(".repeat(99), ")".repeat(99)),
        ),
        (
            "deep.sh",
            format!("{}x{}\n", "{ ".repeat(100), "; }".repeat(100)),
        ),
        (
            "deep.rst",
            (0..45)
                .map(|i| format!("{}- a\n\n", "  ".repeat(i)))
                .collect(),
        ),
        (
            "deep.rst",
            (0..20)
                .map(|i| format!("{}.. note::\n\n", "    ".repeat(i)))
                .collect::<String>()
                + &"    ".repeat(20)
                + "x\n",
        ),
    ];
    for (name, source) in &within {
        format_source(Path::new(name), source, &Config::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

/// Emphasis nested thousands deep, and thousands of unmatched backticks,
/// are kept as written, before the inline grammar's parse time grows
/// past the text's; shallower emphasis is still formatted.
#[test]
fn deep_emphasis_is_kept_as_written() {
    let deep = [
        format!("{}x{}\n", "_*".repeat(3000), "*_".repeat(3000)),
        format!("{}x{}\n", "_*w".repeat(3000), "w*_".repeat(3000)),
        format!("a {}x{} b\n", "_a ".repeat(3000), " a_".repeat(3000)),
        format!("{}x\n", "`a ".repeat(5000)),
    ];
    for source in &deep {
        let out = format_source(Path::new("deep.md"), source, &Config::default()).unwrap();
        assert_eq!(&out, source);
    }
    let within = "*a  *a  *a  x a* a* a*\n";
    let out = format_source(Path::new("deep.md"), within, &Config::default()).unwrap();
    assert_eq!(out, "_a *a *a x a* a* a_\n");
}

/// Delimiter runs in code blocks and thematic breaks are not inline text,
/// so many of them do not count toward the inline limit.
#[test]
fn long_runs_outside_inline_text_are_formatted() {
    let code = format!(
        "a\n\n```\n{}```\n",
        format!("{}\n", "_".repeat(100)).repeat(200)
    );
    let out = format_source(Path::new("runs.md"), &code, &Config::default()).unwrap();
    assert_eq!(out, code);
    let rules = format!("a\n\n{}", format!("{}\n\n", "*".repeat(80)).repeat(200));
    let out = format_source(Path::new("runs.md"), &rules, &Config::default()).unwrap();
    assert_eq!(out, format!("a\n{}", "\n---\n".repeat(200)));
}

#[test]
fn rst_corpus_matches_expected() {
    check_corpus("rst_corpus");
}

/// The `[rst]` style options keep each title's adornment, each list's
/// bullet and two spaces between sentences, fixing only the adornment
/// length; a sentence end at a line break gets two spaces when rewrapping
/// joins the lines, and an abbreviation before a lowercase word does not end
/// a sentence.
#[test]
fn rst_preserve_options_keep_source_style() {
    let source = "\
=======
 Guide
=======

Setup
-----

Read this first.  It is short, compared with the
other guides, vs.
others.  Then
continue.

* one
* two

Details
~~~~~~~~~

+ three
";
    let expected = "\
=======
 Guide
=======

Setup
-----

Read this first.  It is short, compared
with the other guides, vs. others.  Then
continue.

* one
* two

Details
~~~~~~~

+ three
";
    let mut config = Config::default();
    config.rst.max_width = 40;
    config.rst.heading_style = RstStyle::Preserve;
    config.rst.bullet_style = RstStyle::Preserve;
    config.rst.sentence_spacing = RstStyle::Preserve;
    let path = Path::new("guide.rst");
    let out = format_source(path, source, &config).unwrap();
    assert_eq!(out, expected);
    assert_eq!(format_source(path, &out, &config).unwrap(), out);
    let normalized = format_source(path, source, &Config::default()).unwrap();
    assert!(normalized.starts_with("#######\n Guide\n#######\n\n*******\n Setup\n"));
    assert!(normalized.contains("first. It is short"));
    assert!(normalized.contains("\n- one\n- two\n"));
}

/// Compares the docutils document trees of two reStructuredText files.
/// Unknown directives and roles get generic handlers that keep their text,
/// and the Sphinx directives whose body moldy formats as reStructuredText
/// are parsed as such. Bullet characters, enumerator punctuation, ids and
/// docutils messages are not compared, and runs of whitespace in ordinary
/// text count as one space.
const RST_DOCTREE_CHECK: &str = r##"
import sys
from collections import defaultdict
import docutils.core, docutils.statemachine
from docutils import nodes
from docutils.parsers.rst import Directive, directives, roles
RST_BODY = {"seealso"}
TITLED_BODY = {"versionadded", "versionchanged", "versionremoved", "deprecated"}
class Generic(Directive):
    has_content = True
    optional_arguments = 1
    final_argument_whitespace = True
    option_spec = defaultdict(lambda: directives.unchanged)
    def run(self):
        node = nodes.container(classes=["generic", self.name])
        node["options"] = repr(sorted((k, " ".join(str(v or "").split())) for k, v in self.options.items()))
        if self.name in TITLED_BODY:
            node["arguments"] = " ".join(" ".join(self.arguments).split())
            self.state.nested_parse(self.content, self.content_offset, node)
        elif self.name in RST_BODY:
            text = "\n".join(([" ".join(self.arguments)] if self.arguments else []) + list(self.content))
            self.state.nested_parse(docutils.statemachine.StringList(text.splitlines()), self.content_offset, node)
        else:
            node["arguments"] = " ".join(" ".join(self.arguments).split())
            text = "\n".join(self.content)
            node += nodes.literal_block(text, text)
        return [node]
found_directive = directives.directive
directives.directive = lambda name, lang, doc: (found_directive(name, lang, doc)[0] or Generic, [])
found_role = roles.role
def generic_role(name, rawtext, text, lineno, inliner, options=None, content=None):
    return [nodes.literal(rawtext, " ".join(text.split()), classes=["role", name])], []
roles.role = lambda name, lang, lineno, rep: (found_role(name, lang, lineno, rep)[0] or generic_role, [])
DROP = {"ids", "backrefs", "dupnames", "source", "line", "bullet", "prefix", "suffix", "xml:space"}
VERBATIM = (nodes.literal_block, nodes.doctest_block, nodes.raw, nodes.comment)
def flatten(node, out, verbatim=False):
    if isinstance(node, (nodes.system_message, nodes.problematic)):
        return
    if isinstance(node, nodes.Text):
        t = str(node) if verbatim else " ".join(str(node).split())
        if out and out[-1][0] == "#text" and not verbatim:
            out[-1] = ("#text", " ".join((out[-1][1] + " " + t).split()))
        elif t:
            out.append(("#text", t))
        return
    attrs = sorted((k, repr(" ".join(v.split()) if k == "title" else v)) for k, v in node.attributes.items() if k not in DROP and v not in ([], "", None))
    out.append((node.tagname, attrs))
    for child in node.children:
        flatten(child, out, verbatim or isinstance(node, VERBATIM))
    out.append(("/" + node.tagname,))
def tree(path):
    settings = {"report_level": 5, "halt_level": 5, "warning_stream": False, "file_insertion_enabled": False}
    out = []
    flatten(docutils.core.publish_doctree(open(path, encoding="utf-8").read(), settings_overrides=settings), out)
    return out
a, b = tree(sys.argv[1]), tree(sys.argv[2])
i = next((k for k, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
assert a == b, f"document trees differ: {a[i:i + 3]} != {b[i:i + 3]}"
"##;

#[test]
#[ignore = "requires python3 with docutils"]
fn rst_corpus_keeps_meaning() {
    for input in corpus_inputs("rst_corpus") {
        let output = Command::new("python3")
            .arg("-c")
            .arg(RST_DOCTREE_CHECK)
            .arg(&input)
            .arg(expected_path(&input))
            .output()
            .expect("python3 not found");
        assert!(
            output.status.success(),
            "{}: {}",
            input.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// reStructuredText fixtures whose expected output knowingly differs from
/// docstrfmt's. docstrfmt refuses attributions, option lists, empty fields
/// and a list of separate lists; it formats Python code with black, drops
/// admonition options, sorts and lowercases targets, deletes `\ ` escapes,
/// collapses spaces in inline literals, and re-renders tables, all of which
/// moldy keeps. The kept-as-written fixture keeps input the grammar
/// misreads, and refused and malformed input is returned unchanged.
const RST_DOCSTRFMT_DEVIATIONS: &[&str] = &[
    "blocks.rst",
    "explicit_markup.rst",
    "kept_as_written.rst",
    "lists.rst",
    "sections_and_paragraphs.rst",
    "tables.rst",
];

#[test]
#[ignore = "requires docstrfmt on PATH"]
fn rst_corpus_matches_docstrfmt() {
    for input in corpus_inputs("rst_corpus") {
        if RST_DOCSTRFMT_DEVIATIONS.contains(&file_name(&input)) || is_unparsed_fixture(&input) {
            continue;
        }
        let output = Command::new("docstrfmt")
            .arg("-")
            .stdin(fs::File::open(&input).unwrap())
            .output()
            .expect("docstrfmt not found");
        assert!(output.status.success(), "{:?}", output);
        let expected = fs::read_to_string(expected_path(&input)).unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            expected,
            "{} differs from docstrfmt",
            input.display()
        );
    }
}
