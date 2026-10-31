//! Cutaway export: FCPXML (Resolve, Logic), SRT, cut list (CSV, Markdown), mix bounce.

pub mod bounce;
pub mod cutlist;
pub mod error;
pub mod fcpxml;
pub mod srt;
pub mod timeline;

pub use error::{ExportError, Result};
pub use timeline::{EpisodeTimeline, ExportScope, TimelineClip};
