# moldy

A tree-sitter based formatter for C, C++, Rust, and Python. C/C++ formatting targets [funky](https://github.com/brandon-arrendondo/funky); the checked-in corpus verifies matching expected output. Rust and Python have their own corpus and idempotency tests, with compatibility limits described in the [usage guide](docs/usage.md).

## Installation

```sh
cargo install moldy-fmt --locked
```

To build from source:

```sh
cargo build --release --locked
./target/release/moldy --version
```

The package is `moldy-fmt`; the executable and library are `moldy`. See [installation](docs/installation.md) for development prerequisites.

## Usage

```sh
# Write one formatted file to stdout
moldy src/foo.c

# Edit files in place
moldy --in-place src/foo.c include/foo.h

# Check recursively for CI
moldy --check --recursive src/

# Apply a built-in style
moldy --preset black example.py

# Inspect the concrete syntax tree
moldy --dump-tree src/foo.c
```

Use filenames with recognized extensions. Stdin is not implemented. Recursive discovery skips `.h`; pass C headers explicitly. `.hpp` and `.hxx` use C++ and are discovered recursively. See [usage](docs/usage.md) for all options, extensions, exit behavior, and limitations.

## Configuration

Moldy loads `moldy.toml` from the current working directory, or a file selected by `--config`. It does not automatically load `funky.toml`; pass `--config funky.toml` to reuse a compatible C/C++ configuration. `--preset` replaces config-file loading and conflicts with `--config`.

```toml
[indent]
width = 4

[ignore]
patterns = ["vendor/**", "*.pb.h"]

[python]
max_width = 79
```

The nearest `toolchain.toml` found by walking up from the current working directory contributes shared `[ignore].paths`. See [configuration](docs/configuration.md) for complete defaults and preset names.

## Documentation

- [Installation and development prerequisites](docs/installation.md)
- [CLI usage, language support, and limitations](docs/usage.md)
- [Configuration and shared ignores](docs/configuration.md)
- [Architecture and substrate responsibilities](docs/architecture.md)
- [Contributing and verification](docs/development.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Command manual](doc/moldy.1)

## Pre-commit integration

```yaml
repos:
  - repo: https://github.com/brandon-arrendondo/moldy
    rev: v0.1.0
    hooks:
      - id: moldy  # runs moldy --in-place
```

Pin a released tag when using the hook; choose the release whose behavior you want to use.

## Relationship to funky

Moldy uses a concrete syntax tree instead of funky's lexer and token stream. Its C/C++ formatting configuration aims to be compatible with funky, and `--dump-tree` replaces `--dump-tokens`. Passing the local corpus is evidence for those fixtures, rather than a guarantee of identical output on every C/C++ program. Rust and Python formatting do not have a funky parity target.

## AI Assistance

moldy was developed with assistance from [Claude](https://claude.ai) (Anthropic), used for code generation for its formatters, bug fixes, and project setup. From October 2026, [Codex](https://openai.com/codex/) (OpenAI) also contributed the tooling that enforces the agent and commit guidelines. Each of its changes was reviewed before it was merged.

Many earlier commits have a `Co-Authored-By: Claude` trailer, but not every AI-assisted commit does, so the trailers are not a complete record. From October 2026 the contribution is acknowledged once, here, and not with a co-author trailer on each commit.

## License

MIT. Copyright 2026 BISSELL Homecare, Inc. See [LICENSE](LICENSE).
