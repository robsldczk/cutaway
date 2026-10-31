use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error(transparent)]
    Core(#[from] apc_core::CoreError),
    #[error(transparent)]
    Media(#[from] apc_media::MediaError),
    #[error(transparent)]
    Asr(#[from] crate::asr::AsrError),
    #[error(transparent)]
    Ai(#[from] apc_ai::AiError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = AnalysisError> = std::result::Result<T, E>;
