//! Cutaway media: decode, envelopes, peaks, loudness, sync, BWF metadata, WAV writing.

pub mod analysis;
pub mod bwf;
pub mod decode;
pub mod error;
pub mod loudness;
pub mod sync;
pub mod wav;

pub use analysis::{AudioAnalysis, ENV_FPS, FINE_FPS, PEAK_FPS};
pub use decode::{AudioReader, MediaInfo};
pub use error::{MediaError, Result};
