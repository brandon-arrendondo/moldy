// CommonMark emphasis pairing for the Markdown formatter.
//
// The `markdown_inline` grammar pairs `*` and `_` delimiters differently
// from CommonMark in some text: it accepts `_` between letters and digits
// (`ota_0 or ota_1`) and can nest a run inside another
// (`*_a* and *_b*`). This module runs CommonMark's delimiter-run algorithm
// ("process emphasis") so the formatter can tell which emphasis nodes are
// real, and so it can check that its output pairs every delimiter the same
// way as its input.
//
// Code spans, raw HTML, autolinks and link destinations are excluded
// ranges supplied by the caller. Link brackets are not modelled: emphasis
// that crosses a link boundary is paired as if the link were plain text.

use std::collections::HashSet;
use std::ops::Range;

/// A delimiter run: a maximal sequence of `*` or of `_`.
struct Run {
    start: usize,
    len: usize,
    ch: u8,
    can_open: bool,
    can_close: bool,
    /// Characters used as a closer, taken from the left.
    used_left: usize,
    /// Characters used as an opener, taken from the right.
    used_right: usize,
    active: bool,
}

impl Run {
    fn remaining(&self) -> usize {
        self.len - self.used_left - self.used_right
    }
}

fn is_punct(c: Option<char>) -> bool {
    c.is_some_and(|c| !c.is_alphanumeric() && !c.is_whitespace())
}

fn is_space(c: Option<char>) -> bool {
    c.is_none_or(char::is_whitespace)
}

/// CommonMark's left- and right-flanking classes for a delimiter run
/// between `before` and `after` (`None` at the start or end of the text).
pub(super) fn flanking(before: Option<char>, after: Option<char>) -> (bool, bool) {
    let left = !is_space(after) && (!is_punct(after) || is_space(before) || is_punct(before));
    let right = !is_space(before) && (!is_punct(before) || is_space(after) || is_punct(after));
    (left, right)
}

/// Whether `ch` could open (`open`) or close (`!open`) emphasis between
/// `before` and `after`.
pub(super) fn can_delimit(ch: char, before: Option<char>, after: Option<char>, open: bool) -> bool {
    let (left, right) = flanking(before, after);
    match (ch, open) {
        ('*', true) => left,
        ('*', false) => right,
        (_, true) => left && (!right || is_punct(before)),
        (_, false) => right && (!left || is_punct(after)),
    }
}

fn runs(text: &str, excluded: &[Range<usize>]) -> Vec<Run> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(range) = excluded.iter().find(|r| r.contains(&i)) {
            i = range.end;
            continue;
        }
        let b = bytes[i];
        if b == b'\\' && bytes.get(i + 1).is_some_and(u8::is_ascii_punctuation) {
            i += 2;
            continue;
        }
        if b != b'*' && b != b'_' {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < bytes.len() && bytes[j] == b && !excluded.iter().any(|r| r.contains(&j)) {
            j += 1;
        }
        let before = text[..i].chars().next_back();
        let after = text[j..].chars().next();
        out.push(Run {
            start: i,
            len: j - i,
            ch: b,
            can_open: can_delimit(b as char, before, after, true),
            can_close: can_delimit(b as char, before, after, false),
            used_left: 0,
            used_right: 0,
            active: true,
        });
        i = j;
    }
    out
}

/// An upper bound on how deeply emphasis can nest in `text`: delimiter
/// runs that can open count up, and runs of the same character that can
/// only close count down. `*` and `_` are counted apart, because the
/// grammar nests one inside the other where CommonMark would not. Checked
/// before the inline grammar parses `text`, whose time grows faster than
/// the text when emphasis nests thousands deep.
pub(super) fn nesting_bound(text: &str) -> usize {
    let (mut stars, mut lines) = (0usize, 0usize);
    let mut deepest = 0;
    for run in runs(text, &[]) {
        let depth = if run.ch == b'*' {
            &mut stars
        } else {
            &mut lines
        };
        if run.can_open {
            *depth += 1;
        } else if run.can_close {
            *depth = depth.saturating_sub(1);
        }
        deepest = deepest.max(stars + lines);
    }
    deepest
}

/// CommonMark's emphasis pairs in `text`: `(opener, closer, width)`, where
/// `opener` and `closer` are the byte offsets of the first delimiter each
/// side uses and `width` is 1 for emphasis and 2 for strong emphasis.
pub(super) fn pairs(text: &str, excluded: &[Range<usize>]) -> HashSet<(usize, usize, usize)> {
    let mut runs = runs(text, excluded);
    let mut found = HashSet::new();
    for c in 0..runs.len() {
        if !runs[c].can_close {
            continue;
        }
        while runs[c].remaining() > 0 {
            let closer = &runs[c];
            let opener = (0..c).rev().find(|&o| {
                let run = &runs[o];
                let multiple_of_3 = (run.can_close || closer.can_open)
                    && (run.len + closer.len).is_multiple_of(3)
                    && !(run.len.is_multiple_of(3) && closer.len.is_multiple_of(3));
                run.active
                    && run.remaining() > 0
                    && run.ch == closer.ch
                    && run.can_open
                    && !multiple_of_3
            });
            let Some(o) = opener else {
                break;
            };
            let width = if runs[o].remaining() >= 2 && runs[c].remaining() >= 2 {
                2
            } else {
                1
            };
            let open_at = runs[o].start + runs[o].len - runs[o].used_right - width;
            let close_at = runs[c].start + runs[c].used_left;
            found.insert((open_at, close_at, width));
            runs[o].used_right += width;
            runs[c].used_left += width;
            for run in &mut runs[o + 1..c] {
                run.active = false;
            }
        }
        if runs[c].remaining() > 0 && !runs[c].can_open {
            runs[c].active = false;
        }
    }
    found
}

/// `text` with whitespace removed and every CommonMark emphasis pair
/// replaced by markers, so two texts compare equal when they emphasize the
/// same characters. `***a***` and `**_a_**` give the same result.
pub(super) fn shape(text: &str, excluded: &[Range<usize>]) -> String {
    let mut tags: Vec<(usize, &str)> = Vec::new();
    let mut used = vec![false; text.len()];
    for (open, close, width) in pairs(text, excluded) {
        let (o, c) = if width == 2 {
            ("(s", "s)")
        } else {
            ("(e", "e)")
        };
        tags.push((open, o));
        tags.push((close, c));
        for flag in &mut used[open..open + width] {
            *flag = true;
        }
        for flag in &mut used[close..close + width] {
            *flag = true;
        }
    }
    tags.sort_unstable();
    let mut out = String::new();
    let mut tag = tags.iter().peekable();
    for (i, ch) in text.char_indices() {
        while let Some((_, t)) = tag.next_if(|(at, _)| *at == i) {
            out.push_str(t);
        }
        if !used[i] && !ch.is_whitespace() {
            out.push(ch);
        }
    }
    out.replace("(e(s", "(es")
        .replace("(s(e", "(es")
        .replace("s)e)", "es)")
        .replace("e)s)", "es)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn em(text: &str) -> String {
        shape(text, &[])
    }

    #[test]
    fn intraword_underscores_are_not_emphasis() {
        assert_eq!(em("ota_0 or ota_1"), "ota_0orota_1");
        assert_eq!(em("_a_ and *b*"), "(ea e)and(eb e)".replace(' ', ""));
    }

    #[test]
    fn nested_runs_pair_like_commonmark() {
        // Each `*` pairs with the next one; the `_` stay literal.
        assert_eq!(em("*_a* and *_b*"), "(e_ae)and(e_be)");
        assert_eq!(em("**(*)**"), "(s(*)s)");
    }

    #[test]
    fn rule_of_three_and_strong() {
        assert_eq!(em("***a***"), em("**_a_**"));
        assert_eq!(em("*a**b*"), "(ea**be)");
        assert_eq!(em("__a__"), "(sas)");
    }

    #[test]
    fn excluded_ranges_and_escapes_hide_delimiters() {
        let code: Vec<Range<usize>> = vec![Range { start: 0, end: 5 }];
        assert_eq!(shape("`*a*` *b*", &code), "`*a*`(ebe)");
        assert_eq!(em(r"\*a*"), r"\*a*");
    }
}
