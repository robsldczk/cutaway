use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(transparent)]
    Audio(#[from] apc_audio::AudioError),
    #[error(transparent)]
    Media(#[from] apc_media::MediaError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = ExportError> = std::result::Result<T, E>;
