//! Cutaway core: document model, command bus, undo, project I/O.

pub mod brand {
    //! Product names, generated from `brand.json` (see `docs/specs/architecture.md`, Naming).
    include!(concat!(env!("OUT_DIR"), "/brand.rs"));
}

pub mod backup;
pub mod bus;
pub mod commands;
pub mod config;
pub mod error;
pub mod fountain;
pub mod ids;
pub mod media;
pub mod model;
pub mod project;
pub mod script;
pub mod store;
pub mod text;
pub mod timeline;

pub use bus::{Change, CommandBus, Executed, Tx};
pub use commands::{AnalysisPayload, Command};
pub use error::{CoreError, Result};
pub use model::*;
