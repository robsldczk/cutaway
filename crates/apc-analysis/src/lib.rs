//! Cutaway analysis: ASR providers, the script aligner, take detection, room
//! classification, note linking and the pipeline that ties them together.

pub mod align;
pub mod asr;
pub mod edges;
pub mod error;
pub mod fuzzy;
#[cfg(feature = "parakeet")]
pub mod parakeet;
pub mod performance;
pub mod pipeline;
pub mod rescue;
pub mod roles;
pub mod sentences;
pub mod sessions;
pub mod vocals;
pub mod wordmap;

pub use error::{AnalysisError, Result};
pub use pipeline::{
    analyze, analyze_scoped, AnalysisOutput, AnalysisScope, MatchReport, Progress, Providers,
};
