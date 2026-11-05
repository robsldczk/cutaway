use thiserror::Error;

/// Errors from the core document, store and project layers.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("config error: {0}")]
    Toml(String),
    #[error("{kind} '{id}' not found")]
    NotFound { kind: String, id: String },
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("unsupported project schema version {found} (this build reads up to {supported})")]
    SchemaVersion { found: i64, supported: i64 },
    /// An edit would have moved something in a locked scene. The message names the lock.
    #[error("{0}")]
    SceneLocked(String),
    #[error("{0}")]
    Other(String),
}

impl CoreError {
    pub fn not_found(kind: impl Into<String>, id: impl Into<String>) -> Self {
        CoreError::NotFound {
            kind: kind.into(),
            id: id.into(),
        }
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        CoreError::Invalid(msg.into())
    }
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
