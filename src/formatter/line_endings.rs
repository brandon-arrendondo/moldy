//! Restore mapped source endings after formatting the original parser input.

use std::collections::{HashMap, VecDeque};

/// Explicit output convention. Formatting preserves source endings by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineEndings {
    /// Retain mapped line endings; use the predominant ending for new lines.
    #[default]
    Preserve,
    /// Normalize every ending to LF.
    Lf,
    /// Normalize every ending to CRLF.
    Crlf,
}

struct SourceLine {
    key: String,
    ending: Option<&'static str>,
}

fn key(line: &str) -> String {
    line.chars().filter(|c| !c.is_whitespace()).collect()
}

fn source_lines(source: &str) -> Vec<SourceLine> {
    source
        .split_inclusive('\n')
        .map(|line| {
            let ending = if line.ends_with("\r\n") {
                Some("\r\n")
            } else if line.ends_with('\n') {
                Some("\n")
            } else {
                None
            };
            SourceLine {
                key: key(line),
                ending,
            }
        })
        .collect()
}

fn predominant(lines: &[SourceLine]) -> &'static str {
    let crlf = lines.iter().filter(|l| l.ending == Some("\r\n")).count();
    let lf = lines.iter().filter(|l| l.ending == Some("\n")).count();
    match crlf.cmp(&lf) {
        std::cmp::Ordering::Greater => "\r\n",
        std::cmp::Ordering::Less => "\n",
        std::cmp::Ordering::Equal => lines.iter().find_map(|l| l.ending).unwrap_or("\n"),
    }
}

// Patience anchors keep repeated braces and blank lines between their unique
// neighbors. A created brace cannot consume an unrelated later source brace.
fn anchors(source: &[SourceLine], output: &[String]) -> Vec<(usize, usize)> {
    let mut positions: HashMap<&str, Option<usize>> = HashMap::new();
    for (index, line) in source.iter().enumerate() {
        positions
            .entry(&line.key)
            .and_modify(|position| *position = None)
            .or_insert(Some(index));
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in output {
        *counts.entry(line).or_default() += 1;
    }
    let pairs: Vec<(usize, usize)> = output
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            if counts[line.as_str()] != 1 {
                return None;
            }
            positions
                .get(line.as_str())
                .copied()
                .flatten()
                .map(|source_index| (index, source_index))
        })
        .collect();
    ordered_anchors(&pairs)
}

fn ordered_anchors(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![None; pairs.len()];
    for (index, &(_, source_index)) in pairs.iter().enumerate() {
        let slot = tails.partition_point(|&tail| pairs[tail].1 < source_index);
        if slot > 0 {
            previous[index] = Some(tails[slot - 1]);
        }
        if slot == tails.len() {
            tails.push(index);
        } else {
            tails[slot] = index;
        }
    }
    let mut result = Vec::new();
    let mut index = tails.last().copied();
    while let Some(current) = index {
        result.push(pairs[current]);
        index = previous[current];
    }
    result.reverse();
    result
}

fn map_gap(
    source: &[SourceLine],
    output: &[String],
    endings: &mut [&'static str],
    fallback: &'static str,
) {
    let mut positions: HashMap<&str, VecDeque<usize>> = HashMap::new();
    for (index, line) in source.iter().enumerate() {
        positions.entry(&line.key).or_default().push_back(index);
    }
    let mut cursor = 0;
    let mut suffix_cursor = 0;
    for (index, line) in output.iter().enumerate() {
        let full_match = positions.get_mut(line.as_str()).and_then(|candidates| {
            while candidates
                .front()
                .is_some_and(|&position| position < cursor)
            {
                candidates.pop_front();
            }
            candidates.pop_front()
        });
        // A wrapped final fragment owns the original line's ending. In
        // particular, moving a literal's prefix must not convert its first
        // embedded newline. Examine only the next nonempty source line.
        while suffix_cursor < source.len()
            && (suffix_cursor < cursor || source[suffix_cursor].key.is_empty())
        {
            suffix_cursor += 1;
        }
        let position = full_match.or_else(|| {
            source.get(suffix_cursor).and_then(|candidate| {
                (line.len() > 1 && candidate.key.ends_with(line)).then_some(suffix_cursor)
            })
        });
        if let Some(position) = position {
            endings[index] = source[position].ending.unwrap_or(fallback);
            cursor = position + 1;
        }
    }
}

fn preserved_endings(source: &[SourceLine], output: &[String]) -> Vec<&'static str> {
    let fallback = predominant(source);
    let mut endings = vec![fallback; output.len()];
    let mut boundaries = anchors(source, output);
    boundaries.push((output.len(), source.len()));
    let (mut output_start, mut source_start) = (0, 0);
    for (output_end, source_end) in boundaries {
        map_gap(
            &source[source_start..source_end],
            &output[output_start..output_end],
            &mut endings[output_start..output_end],
            fallback,
        );
        if output_end < output.len() {
            endings[output_end] = source[source_end].ending.unwrap_or(fallback);
        }
        output_start = output_end + 1;
        source_start = source_end + 1;
    }
    endings
}

// Opaque/error recovery can retain only the CR of a source CRLF. Treat that
// emitted CR as a boundary, rather than deleting it and joining code/comments.
// Actual standalone source CR remains text; CR-only files are not a policy.
fn canonical_render(source: &str, formatted: &str) -> String {
    let mut rendered = formatted.replace("\r\n", "\n");
    let source_has_crlf = source.contains("\r\n");
    let source_has_lone_cr = source
        .bytes()
        .enumerate()
        .any(|(i, byte)| byte == b'\r' && source.as_bytes().get(i + 1) != Some(&b'\n'));
    if source_has_crlf && !source_has_lone_cr {
        rendered = rendered.replace('\r', "\n");
    }
    rendered
}

pub(super) fn restore(source: &str, formatted: &str, policy: LineEndings) -> String {
    if policy == LineEndings::Preserve && !source.contains("\r\n") {
        return formatted.to_owned();
    }
    let canonical = canonical_render(source, formatted);
    let lines: Vec<&str> = canonical.split_inclusive('\n').collect();
    let endings = match policy {
        LineEndings::Lf => vec!["\n"; lines.len()],
        LineEndings::Crlf => vec!["\r\n"; lines.len()],
        LineEndings::Preserve => {
            let output_keys: Vec<String> = lines.iter().map(|line| key(line)).collect();
            preserved_endings(&source_lines(source), &output_keys)
        }
    };
    let mut result = String::with_capacity(formatted.len());
    for (line, ending) in lines.into_iter().zip(endings) {
        if let Some(text) = line.strip_suffix('\n') {
            result.push_str(text);
            result.push_str(ending);
        } else {
            result.push_str(line);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carriage_returns_from_recovered_crlf_remain_boundaries() {
        let source = "#define VALUE 1\r\n// comment\r\nint value;\r\n";
        let rendered = "#define VALUE 1\r// comment\rint value;\n";
        assert_eq!(restore(source, rendered, LineEndings::Preserve), source);
        assert_eq!(
            restore(source, rendered, LineEndings::Lf),
            source.replace("\r\n", "\n")
        );
        assert_eq!(
            canonical_render("literal\rtext\n", "literal\rtext\n"),
            "literal\rtext\n"
        );
    }

    #[test]
    fn mapped_mixed_and_repeated_lines_keep_their_endings() {
        let source = "first\r\n\r\n{\n    value;\r\n}\n\nlast\r\n";
        let output = "first\n\n{\nvalue;\n}\n\nlast\n";
        assert_eq!(
            restore(source, output, LineEndings::Preserve),
            source.replace("    ", "")
        );
    }

    #[test]
    fn new_lines_use_predominant_and_ties_use_first() {
        assert_eq!(
            restore("first\r\nlast\n", "joined\n", LineEndings::Preserve),
            "joined\r\n"
        );
        assert_eq!(
            restore("first\nlast\r\n", "joined\n", LineEndings::Preserve),
            "joined\n"
        );
        assert_eq!(
            restore("first\r\nmid\r\nlast\n", "joined\n", LineEndings::Preserve),
            "joined\r\n"
        );
        assert_eq!(
            restore("first", "first\n", LineEndings::Preserve),
            "first\n"
        );
    }

    #[test]
    fn inserted_brace_does_not_take_a_later_braces_ending() {
        let source = "if (ready)\r\n    use();\nnext();\r\n{\nother();\r\n}\n";
        let output = "if (ready)\n{\nuse();\n}\nnext();\n{\nother();\n}\n";
        let restored = restore(source, output, LineEndings::Preserve);
        assert!(restored.contains("use();\n"));
        assert!(restored.ends_with("{\nother();\r\n}\n"));
    }
}
