//! Cutaway audio: the play schedule built from the timeline, a mixer shared by real-time
//! playback and offline render, and the cpal output engine.

pub mod engine;
pub mod error;
pub mod mixer;
pub mod offline;
pub mod schedule;

pub use error::{AudioError, Result};
pub use mixer::{Mixer, SourceSet};
pub use schedule::{MixSettings, PlaySchedule, ScheduledClip};
