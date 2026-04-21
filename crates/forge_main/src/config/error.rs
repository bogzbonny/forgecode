/// Errors produced by the `forge_config` module.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to read or parse configuration from a file or environment.
    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),

    /// Failed to serialize or write configuration.
    #[error("Serialization error: {0}")]
    Serialization(#[from] toml_edit::ser::Error),

    /// An I/O error occurred while reading or writing configuration files.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// A `Result` type alias for this module's [`Error`] type.
pub type Result<T> = std::result::Result<T, Error>;
