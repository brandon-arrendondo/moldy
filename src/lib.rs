//! moldy's public library API: config loading, formatter dispatch, toolchain
//! detection, and error types, re-exported from the modules below for
//! consumers embedding moldy rather than invoking the `moldy` binary.

#![warn(missing_docs)]

/// The `moldy.toml` schema and its loading/parsing.
pub mod config;
/// Error types returned across the crate.
pub mod error;
/// Per-language formatter dispatch and implementations.
pub mod formatter;
/// Bundled presets that override [`config::Config`] defaults to match an
/// existing tool's output (e.g. `black`, `rustfmt`).
pub mod presets;
/// Toolchain detection (locating an external formatter to compare against
/// or shell out to).
pub mod toolchain;
