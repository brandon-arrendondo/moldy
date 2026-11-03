// A C and C++ lexer that reads text as a compiler does, independently of
// tree-sitter: line splices are removed first, then each token is the
// longest one that can start where the last ended ("maximal munch").
// tree-sitter's grammars do not always read text that way (`:::` is `:`
// `::` to tree-sitter-cpp but `::` `:` to a compiler, and an unterminated
// `/*` is not a comment to them), so a formatter that joins two tokens
// can produce text the parser reads as before and a compiler does not.
// The emitter asks `joins` before it writes two tokens with nothing
// between them, and the token check compares `tokens` of its input and
// output.

/// The punctuators of C, longest first within each length.
const C_PUNCTUATORS: &[&str] = &[
    "%:%:", "...", "<<=", ">>=", "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||",
    "*=", "/=", "%=", "+=", "-=", "&=", "^=", "|=", "##", "<:", ":>", "<%", "%>", "%:", "::",
];

/// The punctuators C++ adds to C's.
const CPP_PUNCTUATORS: &[&str] = &["->*", "<=>", ".*"];

/// A token, with the row (from 0) of the text it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LexToken {
    pub text: String,
    pub row: usize,
}

/// The tokens of `text`, as C (or C++, with `cpp`) reads them. Comments
/// and whitespace separate tokens and are not tokens. Text no token
/// matches (a stray `\` or `@`) is a token of one character, and an
/// unterminated string or character literal ends at its line's end.
pub(super) fn tokens(text: &str, cpp: bool) -> Vec<LexToken> {
    let chars = splice(text);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (c, row) = chars[i];
        let at = |k: usize| chars.get(k).map(|&(c, _)| c);
        // A byte order mark is not part of the text.
        if c.is_whitespace() || c == '\u{feff}' {
            i += 1;
            continue;
        }
        if c == '/' && at(i + 1) == Some('/') {
            while i < chars.len() && chars[i].0 != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && at(i + 1) == Some('*') {
            i += 2;
            while i < chars.len() && !(chars[i].0 == '*' && at(i + 1) == Some('/')) {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        let start = i;
        if is_identifier_start(c) {
            while at(i).is_some_and(is_identifier_char) {
                i += 1;
            }
            let word: String = chars[start..i].iter().map(|&(c, _)| c).collect();
            match at(i) {
                Some(q @ ('"' | '\'')) if is_literal_prefix(&word, q) => {
                    if word.ends_with('R') {
                        i = raw_string_end(&chars, i);
                    } else {
                        i = quoted_end(&chars, i);
                    }
                    if cpp {
                        i = suffix_end(&chars, i);
                    }
                }
                _ => {}
            }
        } else if c.is_ascii_digit() || (c == '.' && at(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            i += 1;
            while let Some(d) = at(i) {
                if matches!(d, 'e' | 'E' | 'p' | 'P') && matches!(at(i + 1), Some('+' | '-')) {
                    i += 2;
                } else if d == '\'' && at(i + 1).is_some_and(is_identifier_char) {
                    // A digit separator (C++14, C23).
                    i += 2;
                } else if d == '.' || is_identifier_char(d) {
                    i += 1;
                } else {
                    break;
                }
            }
        } else if c == '"' || c == '\'' {
            i = quoted_end(&chars, i);
            if cpp {
                i = suffix_end(&chars, i);
            }
        } else {
            i += punctuator_len(&chars[i..], cpp).unwrap_or(1);
        }
        out.push(LexToken {
            text: chars[start..i].iter().map(|&(c, _)| c).collect(),
            row,
        });
    }
    out
}

/// Whether writing `left` and then `right` with nothing between them
/// reads as other tokens than `left`'s followed by `right`'s, as C or as
/// C++: `+` `+` is `++`, `/` `*` opens a comment, `:` `::` is `::` `:`,
/// `int` `x` is `intx`, and in C++ `"a"` `X` is a literal with a suffix.
/// Both dialects are asked because a header is often compiled as either,
/// and a space between two tokens never hurts in either.
pub(super) fn joins(left: &str, right: &str) -> bool {
    !left.is_empty()
        && !right.is_empty()
        && (joins_in(left, right, false) || joins_in(left, right, true))
}

fn joins_in(left: &str, right: &str, cpp: bool) -> bool {
    let apart = tokens(left, cpp)
        .into_iter()
        .chain(tokens(right, cpp))
        .map(|t| t.text);
    let joined = tokens(&format!("{left}{right}"), cpp)
        .into_iter()
        .map(|t| t.text);
    !apart.eq(joined)
}

/// `text`'s characters with each one's row, without line splices (a `\`
/// that ends a line, before an LF or a CRLF). Rows count the lines of
/// `text` as written.
fn splice(text: &str) -> Vec<(char, usize)> {
    let mut out = Vec::with_capacity(text.len());
    let mut row = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let mut ahead = chars.clone();
            // An LF or a CRLF after the `\` makes a splice.
            let _ = ahead.next_if_eq(&'\r');
            if ahead.next_if_eq(&'\n').is_some() {
                chars = ahead;
                row += 1;
                continue;
            }
        }
        out.push((c, row));
        if c == '\n' {
            row += 1;
        }
    }
    out
}

fn is_identifier_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$' || !c.is_ascii()
}

fn is_identifier_char(c: char) -> bool {
    is_identifier_start(c) || c.is_ascii_digit()
}

/// Whether `word` written just before the quote `q` is that literal's
/// encoding prefix, or a raw string's: C++11 has raw strings, and GCC
/// reads them in C too in its GNU modes (its default).
fn is_literal_prefix(word: &str, q: char) -> bool {
    match word {
        "L" | "u" | "U" | "u8" => true,
        "R" | "LR" | "uR" | "UR" | "u8R" => q == '"',
        _ => false,
    }
}

/// The end of the string or character literal whose quote is at `i`: past
/// its closing quote, or at the end of its line if it has none.
fn quoted_end(chars: &[(char, usize)], mut i: usize) -> usize {
    let quote = chars[i].0;
    i += 1;
    while i < chars.len() {
        match chars[i].0 {
            '\\' => i += 2,
            '\n' => return i,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

/// The end of the raw string whose `"` is at `i`: `"delim( ... )delim"`.
/// Without a `(` after a valid delimiter it is an ordinary string.
fn raw_string_end(chars: &[(char, usize)], i: usize) -> usize {
    let mut k = i + 1;
    let mut delim = String::new();
    while let Some(&(c, _)) = chars.get(k) {
        if c == '(' {
            break;
        }
        if c.is_whitespace() || matches!(c, ')' | '\\' | '"') || delim.len() >= 16 {
            return quoted_end(chars, i);
        }
        delim.push(c);
        k += 1;
    }
    let close: Vec<char> = format!("){delim}\"").chars().collect();
    k += 1;
    while k + close.len() <= chars.len() {
        if chars[k..k + close.len()]
            .iter()
            .map(|&(c, _)| c)
            .eq(close.iter().copied())
        {
            return k + close.len();
        }
        k += 1;
    }
    chars.len()
}

/// Past a C++ user-defined literal's suffix starting at `i`, if any.
fn suffix_end(chars: &[(char, usize)], mut i: usize) -> usize {
    if chars.get(i).is_some_and(|&(c, _)| is_identifier_start(c)) {
        while chars.get(i).is_some_and(|&(c, _)| is_identifier_char(c)) {
            i += 1;
        }
    }
    i
}

/// The length of the longest punctuator at the start of `chars`.
fn punctuator_len(chars: &[(char, usize)], cpp: bool) -> Option<usize> {
    let starts = |p: &str| {
        p.chars().count() <= chars.len() && p.chars().zip(chars).all(|(a, &(b, _))| a == b)
    };
    // C++11: `<::` is `<` `::` unless a `:` or `>` follows, so that
    // `std::vector<::T>` names `::T`. C++03 and C read the `<:` digraph,
    // so `joins` keeps `< ::` apart.
    if cpp && starts("<::") && !matches!(chars.get(3).map(|&(c, _)| c), Some(':' | '>')) {
        return Some(1);
    }
    let extra = if cpp { CPP_PUNCTUATORS } else { &[] };
    C_PUNCTUATORS
        .iter()
        .chain(extra)
        .filter(|p| starts(p))
        .map(|p| p.chars().count())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str, cpp: bool) -> Vec<String> {
        tokens(text, cpp).into_iter().map(|t| t.text).collect()
    }

    #[test]
    fn longest_punctuator_wins() {
        assert_eq!(texts(":::", true), ["::", ":"]);
        assert_eq!(texts("a+++b", false), ["a", "++", "+", "b"]);
        assert_eq!(texts("a->*b", true), ["a", "->*", "b"]);
        assert_eq!(texts("a->*b", false), ["a", "->", "*", "b"]);
        assert_eq!(texts("%:%:", false), ["%:%:"]);
    }

    #[test]
    fn a_global_scope_after_less_than_is_not_a_digraph_in_cpp() {
        assert_eq!(texts("v<::T>", true), ["v", "<", "::", "T", ">"]);
        assert_eq!(texts("a<:::b", true), ["a", "<:", "::", "b"]);
        assert_eq!(texts("a<::>", true), ["a", "<:", ":>"]);
        assert_eq!(texts("v<::T", false), ["v", "<:", ":", "T"]);
    }

    #[test]
    fn comments_and_splices_separate_tokens() {
        assert_eq!(texts("a/**/b // c\nd", false), ["a", "b", "d"]);
        assert_eq!(texts("a /* never closed", false), ["a"]);
        assert_eq!(texts("in\\\nt x", false), ["int", "x"]);
        assert_eq!(texts("in\\\r\nt x", false), ["int", "x"]);
        assert_eq!(texts("a // c \\\n b\nd", false), ["a", "d"]);
        assert_eq!(
            texts("\u{feff}#include <a.h>", false),
            ["#", "include", "<", "a", ".", "h", ">"]
        );
    }

    #[test]
    fn rows_count_written_lines() {
        let t = tokens("a\\\nb\nc", false);
        assert_eq!(t.iter().map(|t| t.row).collect::<Vec<_>>(), [0, 2]);
    }

    #[test]
    fn literals_are_single_tokens() {
        assert_eq!(
            texts(r#"x = "a\"b" 'c'"#, false),
            ["x", "=", r#""a\"b""#, "'c'"]
        );
        assert_eq!(
            texts(r#"u8"a" L'b' U"c""#, false),
            [r#"u8"a""#, "L'b'", r#"U"c""#]
        );
        assert_eq!(texts(r#"R"x(a)" b)x" c"#, true), [r#"R"x(a)" b)x""#, "c"]);
        assert_eq!(texts(r#"R"(a)" b"#, false), [r#"R"(a)""#, "b"]);
        assert_eq!(texts(r#"u8R"x()")x""#, false), [r#"u8R"x()")x""#]);
        assert_eq!(texts(r#"R "(a)""#, false), ["R", r#""(a)""#]);
        assert_eq!(texts(r#""a"_s"#, true), [r#""a"_s"#]);
        assert_eq!(texts("don't\nx", false), ["don", "'t", "x"]);
    }

    #[test]
    fn numbers_read_as_preprocessing_numbers() {
        assert_eq!(texts("0x1e+1", false), ["0x1e+1"]);
        assert_eq!(texts("1+2", false), ["1", "+", "2"]);
        assert_eq!(texts("1'000'000", true), ["1'000'000"]);
        assert_eq!(texts(".5f", false), [".5f"]);
        assert_eq!(texts("a.b", false), ["a", ".", "b"]);
    }

    #[test]
    fn joins_finds_tokens_that_read_differently_together() {
        for (left, right) in [
            (":", "::"),
            ("+", "+"),
            ("-", "-"),
            ("-", "--"),
            ("&", "&"),
            ("|", "|"),
            ("<", "<"),
            (">", ">="),
            ("-", ">"),
            ("/", "*"),
            ("/", "/"),
            ("int", "x"),
            ("1", "e"),
            (".", "5"),
            ("<", ":"),
            ("%", ":"),
            ("<", "::"),
            ("\"a\"", "X"),
            ("\"a\"", "L"),
            ("L", "\"b\""),
            ("R", "\"(y)\""),
            ("-", ">*"),
        ] {
            assert!(joins(left, right), "{left} {right}");
        }
        for (left, right) in [
            ("?", "x"),
            ("?", "::"),
            ("=", "::"),
            (",", "::"),
            ("(", "::"),
            ("!", "::"),
            ("return", "("),
            ("a", ")"),
            ("*", "/"),
            ("x", "\"b\""),
        ] {
            assert!(!joins(left, right), "{left} {right}");
        }
    }
}
