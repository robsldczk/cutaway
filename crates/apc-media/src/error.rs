use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot decode {path}: {msg}")]
    Decode { path: String, msg: String },
    #[error("WAV error: {0}")]
    Wav(#[from] hound::Error),
    #[error("loudness error: {0}")]
    Loudness(String),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = MediaError> = std::result::Result<T, E>;
