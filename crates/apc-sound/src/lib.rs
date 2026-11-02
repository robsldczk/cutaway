//! Cutaway sound: the show library (dedup by hash, tags, folders), the bucket of cues,
//! tagging, and the sound search/generate provider interfaces.

pub mod epidemic;
pub mod error;
pub mod library;
pub mod providers;
pub mod secrets;
pub mod show_library;
pub mod tagging;

pub use error::{Result, SoundError};
pub use library::{Library, LibraryAsset, SharedShowLibrary};
pub use providers::{
    GenerateRequest, GeneratedSound, SoundCandidate, SoundGenerateProvider, SoundQuery,
    SoundSearchProvider,
};
pub use show_library::ShowLibrary;
