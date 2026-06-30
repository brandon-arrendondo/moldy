use thiserror::Error;

#[derive(Debug, Error)]
pub enum MoldyError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("formatter error: {0}")]
    Format(String),

    #[error("config error in '{path}': {source}")]
    Config {
        path: String,
        #[source]
        source: toml::de::Error,
    },

    #[error("I/O error for '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("file is not valid UTF-8: {path}")]
    NotUtf8 { path: String },

    #[error("unsupported language for: {0}")]
    UnsupportedLanguage(String),
}
