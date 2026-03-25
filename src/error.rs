/// Custom error types for TermEdit using thiserror.
use thiserror::Error;

/// Top-level error type for the editor.
#[derive(Error, Debug)]
pub enum TermEditError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    ConfigParse(#[from] toml::de::Error),

    #[error("Theme not found: {0}")]
    ThemeNotFound(String),

    #[error("Unsupported encoding: {0}")]
    UnsupportedEncoding(String),

    #[error("Buffer error: {0}")]
    Buffer(String),

    #[error("Clipboard error: {0}")]
    Clipboard(String),

    #[error("Terminal error: {0}")]
    Terminal(String),
}

/// Convenience Result type alias.
pub type Result<T> = std::result::Result<T, TermEditError>;
