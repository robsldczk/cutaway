use thiserror::Error;

#[derive(Debug, Error)]
pub enum SoundError {
    #[error(transparent)]
    Core(#[from] apc_core::CoreError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("provider '{0}' is not available: {1}")]
    Unavailable(String, String),
    #[error("provider request failed: {0}")]
    Http(String),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = SoundError> = std::result::Result<T, E>;
