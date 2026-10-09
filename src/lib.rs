//! moldy's public library API: config loading, formatter dispatch, shared
//! ignore configuration, and error types. These modules serve consumers
//! embedding moldy rather than invoking the `moldy` binary.

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
/// Shared `toolchain.toml` ignore-path discovery.
pub mod toolchain;
