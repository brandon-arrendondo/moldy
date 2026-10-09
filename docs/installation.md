# Installation

Install the published package with `cargo install moldy-fmt --locked`. This installs the `moldy` executable from the published package.

For a source checkout:

```sh
git clone https://github.com/brandon-arrendondo/moldy.git
cd moldy
cargo build --release --locked
./target/release/moldy --help
```

The repository pins Rust 1.95.0 in `rust-toolchain.toml`. Rustup uses that toolchain for commands in the checkout, including in CI. A C/C++ compiler is needed to build the enabled tree-sitter grammars. The lockfile pins the dependency graph; the manifest enables only C, C++, Rust, and Python in `lang-parsing-substrate` 0.11.2.

To install directly from a local checkout, run `cargo install --path . --locked`. See [development](development.md) for the additional tools required by repository checks. Release CI builds Linux and Windows binaries, generates third-party licence records and a CycloneDX SBOM, and packages Linux deb, rpm, and AppImage artifacts. Check the repository releases for available artifacts.
