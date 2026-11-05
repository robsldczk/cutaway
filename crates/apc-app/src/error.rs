use serde::Serialize;
use thiserror::Error;

/// Errors returned to clients. `code` is stable for programmatic use.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Core(#[from] apc_core::CoreError),
    #[error("{0}")]
    Analysis(#[from] apc_analysis::AnalysisError),
    #[error("{0}")]
    Export(#[from] apc_export::ExportError),
    #[error("{0}")]
    Media(#[from] apc_media::MediaError),
    #[error("{0}")]
    Audio(#[from] apc_audio::AudioError),
    #[error("{0}")]
    Sound(#[from] apc_sound::SoundError),
    #[error("{0}")]
    Ai(#[from] apc_ai::AiError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad parameters: {0}")]
    Params(String),
    #[error("unknown method '{0}'")]
    UnknownMethod(String),
    #[error("{0} needs \"confirm\": true")]
    NeedsConfirm(String),
    #[error("no project is open")]
    NoProject,
    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Params(e.to_string())
    }
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Params(_) => "bad_params",
            AppError::UnknownMethod(_) => "unknown_method",
            AppError::NeedsConfirm(_) => "needs_confirm",
            AppError::NoProject => "no_project",
            AppError::Core(apc_core::CoreError::NotFound { .. }) => "not_found",
            AppError::Core(apc_core::CoreError::Invalid(_)) => "invalid",
            AppError::Core(apc_core::CoreError::SceneLocked(_)) => "scene_locked",
            _ => "error",
        }
    }
}

#[derive(Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

impl From<&AppError> for ErrorBody {
    fn from(e: &AppError) -> Self {
        ErrorBody {
            code: e.code().into(),
            message: e.to_string(),
        }
    }
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;
