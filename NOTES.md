# Notes / resume points

## Rustfmt-compatible mode for the Rust formatter

Came up while thinking through pre-commit hook wiring: `.pre-commit-hooks.yaml`
currently scopes the `moldy` hook to `types_or: [c, c++]` only, so it doesn't
touch `.rs` files yet — no conflict with a repo's `cargo fmt` hook today. But
`src/formatter/rust.rs` was built independently of rustfmt, not as a clone,
and it diverges in ways that would make running both hooks on `.rs` files
fight each other:

- No line-width-based wrapping/fill — rustfmt packs to ~100 cols; moldy
  doesn't measure width at all, so it either preserves the source's
  single/multi-line choice (calls, tuples, etc.) or forces one (struct/enum
  field lists always explode one-per-line).
- Struct/enum field lists are *always* multi-line, even when rustfmt would
  keep a short one on a single line.
- Different heuristics for when call-argument/tuple lists break.

Rarely would you wire up two competing formatters for the same language in
one pre-commit config — but it would still be good to have the *option* of
running moldy as a rustfmt-compatible formatter, so someone could actually
replace `cargo fmt` with `moldy` for Rust instead of just adding it
alongside.

**To resume:** figure out what a `[rust]` section of `Config` (or a
`--rustfmt-compat` style flag) would need to control to close the gap:
- A real line-width budget + wrapping algorithm (the current formatter has
  none at all — see `emit_bracket_list` in `src/formatter/rust.rs`, which
  only checks "did the source already span multiple lines", not "does this
  fit in N columns").
- Whether struct/enum field lists collapse to one line when they fit
  (rustfmt does; moldy currently always explodes them —
  `emit_multiline_field_list`).
- rustfmt's "fill" mode for some list contexts (pack multiple items per
  line up to width) vs. moldy's current all-or-nothing per-item-per-line.

Once (if) that lands, `.pre-commit-hooks.yaml` could add `rust` to
`types_or` — but only for repos that use moldy *instead of* `cargo fmt`,
not alongside it.
