//! Cutaway audio: the play schedule built from the timeline, a mixer shared by real-time
//! playback and offline render, the room bus, the cpal output engine and playback leveling.

pub mod depth;
pub mod engine;
pub mod error;
pub mod level;
pub mod mixer;
pub mod offline;
pub mod room;
pub mod schedule;

pub use depth::{depth_mix, DepthMix};
pub use error::{AudioError, Result};
pub use mixer::{Mixer, Renderer, SourceSet};
pub use room::{Room, RoomShape};
pub use schedule::{ClipSpace, MixSettings, PlaySchedule, ScheduledClip};
