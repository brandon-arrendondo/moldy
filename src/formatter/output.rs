// Shared output-buffer behaviour for the CST-walking formatters
// (c_cpp/python/rust). Each formatter's `Fmt<'a>` owns its own `src`,
// `config`, `out`, and `depth` fields (plus whatever per-language state it
// needs) and implements the small accessor set below; the rest of this
// trait's methods — the buffer bookkeeping every formatter reimplemented
// byte-for-byte — are provided once as defaults.
//
// A default-method trait (rather than a `buf: OutputBuffer` field) is used
// deliberately: `render_scratch` needs to swap out a formatter's `out`
// buffer while calling back into that formatter's *own* dispatch methods
// (`emit_node` and friends), which only works if the closure it runs is
// generic over `Self` — i.e. the whole `Fmt`, not just a buffer field.
//
// `at_bol`/`set_at_bol` exist only because c_cpp.rs tracks a real `at_bol`
// field for its own unrelated bookkeeping (comment/column alignment) beyond
// what `raw`/`nl` touch; the default implementation here (derived from
// `out`) is behaviorally identical for python/rust, which never needed a
// cached flag.

use crate::config::{Config, IndentStyle};
use tree_sitter::Node;

pub(super) trait OutputOps<'a>: Sized {
    fn src(&self) -> &'a str;
    fn config(&self) -> &Config;
    fn depth(&self) -> u32;
    fn out(&self) -> &str;
    fn out_mut(&mut self) -> &mut String;

    fn at_bol(&self) -> bool {
        self.out().is_empty() || self.out().ends_with('\n')
    }

    fn set_at_bol(&mut self, _at_bol: bool) {}

    fn finish(mut self) -> String {
        let trimmed_len = self.out().trim_end_matches(['\n', '\r', ' ', '\t']).len();
        self.out_mut().truncate(trimmed_len);
        if self.config().newlines.final_newline && !self.out().is_empty() {
            self.out_mut().push('\n');
        }
        std::mem::take(self.out_mut())
    }

    fn indent_str_at(&self, d: u32) -> String {
        match self.config().indent.style {
            IndentStyle::Spaces => " ".repeat(self.config().indent.width as usize * d as usize),
            IndentStyle::Tabs => "\t".repeat(d as usize),
        }
    }

    fn raw(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let ends_with_nl = s.ends_with('\n');
        self.out_mut().push_str(s);
        self.set_at_bol(ends_with_nl);
    }

    fn nl(&mut self) {
        self.out_mut().push('\n');
        self.set_at_bol(true);
    }

    fn ensure_nl(&mut self) {
        if !self.at_bol() {
            self.nl();
        }
    }

    fn emit_indent(&mut self) {
        let s = self.indent_str_at(self.depth());
        self.raw(&s);
    }

    fn space(&mut self) {
        if !self.at_bol() && !self.out().ends_with(' ') && !self.out().ends_with('\n') {
            self.out_mut().push(' ');
        }
    }

    fn node_text(&self, node: Node) -> &'a str {
        &self.src()[node.start_byte()..node.end_byte()]
    }

    /// Column (in `char`s) of the current end of output, for width budgeting.
    fn current_column(&self) -> usize {
        match self.out().rfind('\n') {
            Some(i) => self.out()[i + 1..].chars().count(),
            None => self.out().chars().count(),
        }
    }

    /// Render `f` into a scratch buffer instead of the real output, returning
    /// what it produced. Used to measure a candidate single-line rendering
    /// before committing to it (width-based wrapping, field-list collapsing).
    fn render_scratch<F: FnOnce(&mut Self)>(&mut self, f: F) -> String {
        let saved = std::mem::take(self.out_mut());
        f(self);
        std::mem::replace(self.out_mut(), saved)
    }

    fn emit_blank_lines(&mut self, n: usize) {
        self.ensure_nl();
        for _ in 0..n {
            self.nl();
        }
    }
}
