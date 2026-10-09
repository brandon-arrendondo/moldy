# Development and verification

Read [AGENTS.md](../AGENTS.md) and the technical guide [CLAUDE.md](../CLAUDE.md) before contributing. Repository policy is kept in AGENTS.md. Check the task tracker as instructed there; if its marker points to an unavailable database, report that limitation rather than creating a replacement database.

## Setup

Use stable Rust with Cargo, a C/C++ compiler, Python 3, and pre-commit. The substrate refresh was checked with Rust 1.95.0; no MSRV is declared.

```sh
rustup component add rustfmt clippy llvm-tools-preview
cargo install cargo-llvm-cov --locked
python3 -m pip install pre-commit
pre-commit install
cargo build --locked
```

The first pre-commit run needs network access to install its pinned hook environments, including the Knots hook.

## Required checks

Run from the directory containing `Cargo.toml`:

```sh
cargo test --locked
pre-commit run --all-files
```

Pre-commit checks formatting, Clippy with warnings denied, file hygiene, agent-policy regressions, Knots complexity, and the coverage gate. Coverage must reach 70% for the files included by `scripts/coverage-gate.sh`; inspect its exclusion regex when interpreting the result. Generated `lcov.info` is ignored.

To check the public Rust documentation:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked
```

C/C++ corpus tests compare with checked-in expected files and verify idempotency. Acceptance tests cover positive and malformed C/C++ fixtures. Rust and Python have separate corpus tests. Python reference checks require installed `ruff` and `flake8` and are deliberately ignored by the default suite:

```sh
cargo test --test python_corpus_test -- --ignored
```

Run those reference checks when changing Python emission or fixtures. They validate the fixture expectations, not arbitrary project output.

## Updating the substrate

Update the manifest and lockfile together. Keep default features disabled and enable only implemented formatters. Prefer a targeted dependency update:

```sh
cargo update -p lang-parsing-substrate --precise 0.11.2
```

Inspect grammar and tree-sitter version changes. Build the old and new versions, format the same pinned inputs with identical presets, and compare stdout, stderr, and exit status. Include malformed acceptance inputs and recursive selection; expected-file tests alone do not establish that all previous behavior is unchanged. Re-run required checks and public API documentation checks.

## Adding a formatter

Enable its substrate grammar feature, add `src/formatter/<language>.rs`, and route its canonical key in `formatter/mod.rs`. Keep emission policy in Moldy and language metadata in the substrate. Add corpus expectations, idempotency tests, malformed-input behavior checks, and CLI discovery tests. Document supported extensions and compatibility limits. Enabling a feature alone is insufficient.
