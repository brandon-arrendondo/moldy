# Installation

Install the published package with `cargo install moldy-fmt --locked`. This installs the `moldy` executable; it does not install the unreleased changes in this branch.

For a source checkout:

```sh
git clone https://github.com/brandon-arrendondo/moldy.git
cd moldy
cargo build --release --locked
./target/release/moldy --help
```

Use Rust and Cargo from a stable toolchain. CI tracks stable; the project has not declared a minimum supported Rust version. This refresh was verified with Rust 1.95.0. A C/C++ compiler is needed to build the enabled tree-sitter grammars. The lockfile pins the dependency graph; the manifest enables only C, C++, Rust, and Python in `lang-parsing-substrate` 0.11.2.

To install directly from a local checkout, run `cargo install --path . --locked`. See [development](development.md) for the additional tools required by repository checks. Release CI builds Linux and Windows binaries, generates third-party licence records and a CycloneDX SBOM, and packages Linux deb, rpm, and AppImage artifacts. This guide does not assert that a release artifact exists for the current branch.
