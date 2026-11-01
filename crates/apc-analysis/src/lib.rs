//! Cutaway analysis: ASR providers, the script aligner, take detection, room
//! classification, note linking and the pipeline that ties them together.

pub mod align;
pub mod asr;
pub mod edges;
pub mod error;
pub mod fuzzy;
#[cfg(feature = "parakeet")]
pub mod parakeet;
pub mod pipeline;
pub mod roles;
pub mod sessions;
pub mod vocals;
pub mod wordmap;

pub use error::{AnalysisError, Result};
pub use pipeline::{analyze, AnalysisOutput, MatchReport, Progress, Providers};
