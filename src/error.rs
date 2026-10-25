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

    /// The source has a syntax error, so it is left unchanged. The JSON,
    /// YAML, Markdown, CMake and Bash formatters return this rather than
    /// format a partly understood file.
    #[error("{language} syntax error at line {line}; file left unchanged")]
    Syntax {
        /// The language whose grammar rejected the file.
        language: String,
        /// 1-based line of the first syntax error.
        line: usize,
    },

    /// Formatting would have changed what the document means, so it is
    /// left unchanged. The Markdown formatter checks its output this way
    /// because its grammar misreads some unusual input.
    #[error(
        "{language} could not be formatted without changing its meaning (line {line}); file left unchanged"
    )]
    Unverified {
        /// The language of the file.
        language: String,
        /// 1-based line where the input and the formatted output first
        /// differ, or where the grammar misreads the input.
        line: usize,
    },

    /// The file nests deeper than the formatter accepts, so it is left
    /// unchanged. The JSON, YAML, Markdown, CMake and Bash formatters
    /// return this rather than exhaust the stack or the grammar's limits.
    #[error("{language} is nested too deeply to format (line {line}); file left unchanged")]
    TooDeep {
        /// The language of the file.
        language: String,
        /// 1-based line where the nesting passes the limit.
        line: usize,
    },

    /// No formatter is registered for the file's detected language.
    #[error("unsupported language for: {0}")]
    UnsupportedLanguage(String),

    /// `--preset` named a preset that is not built in.
    #[error("unknown preset '{name}' (available: {available})")]
    UnknownPreset {
        /// The name that was asked for.
        name: String,
        /// The built-in preset names, comma-separated.
        available: String,
    },
}
