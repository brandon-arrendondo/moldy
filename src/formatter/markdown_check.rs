// An independent CommonMark reading of a Markdown document, used to check
// the formatter's output.
//
// The formatter parses with tree-sitter, and its own output check
// (`Md::shape`) re-parses with the same grammar, so a misreading the input
// and output share goes unnoticed. This module reads both with
// pulldown-cmark, a CommonMark parser, and reduces each to what a reader
// sees: block and inline structure, list looseness and start numbers,
// heading levels, link targets, hard breaks, text with whitespace runs
// collapsed, and code block contents. YAML code blocks are left out
// because the formatter reformats them as YAML; the caller removes front
// matter for the same reason.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// The byte offset in `before` where its reading first differs from
/// `after`'s, or `None` when both read the same.
pub(super) fn first_difference(before: &str, after: &str) -> Option<usize> {
    let (a, b) = (tokens(before), tokens(after));
    if a.iter().map(|t| &t.0).eq(b.iter().map(|t| &t.0)) {
        return None;
    }
    let at = a
        .iter()
        .zip(&b)
        .position(|(x, y)| x.0 != y.0)
        .unwrap_or(a.len().min(b.len()));
    Some(a.get(at).or(a.last()).map_or(0, |t| t.1))
}

/// What `source` renders to under CommonMark (with GFM tables, task lists
/// and strikethrough), as a comparable string.
#[cfg(test)]
fn rendering(source: &str) -> String {
    tokens(source).into_iter().map(|t| t.0).collect()
}

/// `source`'s reading as tokens, each with the byte offset where it
/// starts.
fn tokens(source: &str) -> Vec<(String, usize)> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let mut out: Vec<(String, usize)> = Vec::new();
    // Pending text and the offset where it starts.
    let mut text = String::new();
    let mut text_at = 0;
    // Inside a code block: `Some(true)` for YAML, whose body is skipped.
    let mut code: Option<bool> = None;
    // The formatter drops a byte order mark, as renderers do.
    let body = source.strip_prefix('\u{feff}').unwrap_or(source);
    let shift = source.len() - body.len();
    for (event, range) in Parser::new_ext(body, options).into_offset_iter() {
        let at = range.start + shift;
        let mut add_text = |t: &str| {
            if text.is_empty() {
                text_at = at;
            }
            text.push_str(t);
        };
        match event {
            Event::Text(_) if code == Some(true) => {}
            Event::Text(t) | Event::Html(t) | Event::InlineHtml(t) => add_text(&t),
            Event::Code(t) => add_text(&format!("`{t}`")),
            Event::SoftBreak => add_text(" "),
            event => {
                flush(&mut text, text_at, code.is_some(), &mut out);
                let token = match event {
                    Event::Start(Tag::CodeBlock(kind)) => {
                        let info = match &kind {
                            CodeBlockKind::Fenced(info) => info
                                .split_ascii_whitespace()
                                .next()
                                .unwrap_or("")
                                .to_string(),
                            CodeBlockKind::Indented => String::new(),
                        };
                        code = Some(matches!(info.as_str(), "yaml" | "yml"));
                        format!("(code {info}|")
                    }
                    Event::End(TagEnd::CodeBlock) => {
                        code = None;
                        ")".to_string()
                    }
                    Event::Start(tag) => start(&tag),
                    Event::End(_) => ")".to_string(),
                    Event::HardBreak => "(br)".to_string(),
                    Event::Rule => "(hr)".to_string(),
                    Event::TaskListMarker(done) => if done { "[x]" } else { "[ ]" }.to_string(),
                    other => format!("{other:?}"),
                };
                // `***a***` and `**_a_**` render alike.
                let both = matches!(
                    (out.last().map(|t| t.0.as_str()), token.as_str()),
                    (Some("(em"), "(strong") | (Some("(strong"), "(em")
                );
                match out.last_mut() {
                    Some(last) if both => last.0 = "(both".to_string(),
                    _ => out.push((token, at)),
                }
            }
        }
    }
    flush(&mut text, text_at, false, &mut out);
    out
}

fn start(tag: &Tag) -> String {
    match tag {
        Tag::Heading { level, .. } => format!("(h{}", *level as u8),
        Tag::List(Some(n)) => format!("(ol{n}"),
        Tag::List(None) => "(ul".to_string(),
        Tag::Item => "(li".to_string(),
        Tag::Paragraph => "(p".to_string(),
        Tag::BlockQuote(_) => "(quote".to_string(),
        Tag::Emphasis => "(em".to_string(),
        Tag::Strong => "(strong".to_string(),
        Tag::Strikethrough => "(del".to_string(),
        Tag::Link {
            dest_url, title, ..
        } => format!("(a {dest_url} {title}|"),
        Tag::Image {
            dest_url, title, ..
        } => format!("(img {dest_url} {title}|"),
        Tag::Table(align) => format!("(table {align:?}"),
        Tag::TableHead => "(thead".to_string(),
        Tag::TableRow => "(tr".to_string(),
        Tag::TableCell => "(td".to_string(),
        Tag::HtmlBlock => "(html".to_string(),
        other => format!("({other:?}"),
    }
}

/// Adds pending text as a token: code keeps its lines (without trailing
/// spaces or trailing blank lines); other text has whitespace runs
/// collapsed, as HTML renders it.
fn flush(text: &mut String, at: usize, code: bool, out: &mut Vec<(String, usize)>) {
    let token = if code {
        let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
        format!("|{}|", lines.join("\n").trim_end_matches('\n'))
    } else {
        let words: Vec<&str> = text.split_ascii_whitespace().collect();
        if words.is_empty() {
            String::new()
        } else {
            format!("\"{}\"", words.join(" "))
        }
    };
    if !token.is_empty() {
        out.push((token, at));
    }
    text.clear();
}

#[cfg(test)]
mod tests {
    use super::rendering;

    #[test]
    fn layout_differences_render_alike() {
        assert_eq!(rendering("* a\n* b\n"), rendering("- a\n- b\n"));
        assert_eq!(rendering("Title\n=====\n"), rendering("# Title\n"));
        assert_eq!(rendering("*a*  b\nc\n"), rendering("_a_ b\nc\n"));
        assert_eq!(rendering("***a***\n"), rendering("**_a_**\n"));
    }

    #[test]
    fn meaning_differences_do_not() {
        assert_ne!(rendering("- a\n- b\n"), rendering("- a\n\n- b\n"));
        assert_ne!(rendering("a\t\nb\n"), rendering("a  \nb\n"));
        assert_ne!(rendering("Para\n3. x\n"), rendering("Para\n\n3. x\n"));
        assert_ne!(rendering("    a\n    b\n"), rendering("    a\nb\n"));
        assert_ne!(rendering("ota_0 or ota_1\n"), rendering("ota*0 or ota*1\n"));
    }
}
