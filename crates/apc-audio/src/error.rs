use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error(transparent)]
    Media(#[from] apc_media::MediaError),
    #[error("unknown source '{0}'")]
    UnknownSource(String),
    #[error("audio device: {0}")]
    Device(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T, E = AudioError> = std::result::Result<T, E>;
