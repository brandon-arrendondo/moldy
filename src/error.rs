use thiserror::Error;

/// The error type returned across the crate: parsing, formatting, config,
/// and I/O failures unified so callers have one `Result<_, MoldyError>`.
#[derive(Debug, Error)]
pub enum MoldyError {
    /// A source file failed to parse.
    #[error("parse error: {0}")]
    Parse(String),

    /// A formatter implementation failed to produce output.
    #[error("formatter error: {0}")]
    Format(String),

    /// A `moldy.toml` config file failed to parse.
    #[error("config error in '{path}': {source}")]
    Config {
        /// Path (or other label) of the config source that failed.
        path: String,
        /// The underlying TOML parse error.
        #[source]
        source: toml::de::Error,
    },

    /// Reading or writing a file failed.
    #[error("I/O error for '{path}': {source}")]
    Io {
        /// Path of the file the I/O operation targeted.
        path: String,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A file's contents could not be decoded as UTF-8.
    #[error("file is not valid UTF-8: {path}")]
    NotUtf8 {
        /// Path of the non-UTF-8 file.
        path: String,
    },

    /// No formatter is registered for the file's detected language.
    #[error("unsupported language for: {0}")]
    UnsupportedLanguage(String),
}
