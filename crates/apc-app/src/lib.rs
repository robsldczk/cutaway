//! The service layer shared by every client: the Tauri UI, the `cutaway` CLI, the MCP
//! server and the HTTP dev bridge all go through `Session::call` / `Service::call`.

pub mod bridge;
pub mod buckets;
pub mod cleanup;
pub mod demo;
pub mod dev;
pub mod error;
pub mod exports;
pub mod import;
pub mod jobs;
pub mod mapping;
pub mod methods;
pub mod performances;
pub mod player;
pub mod review;
pub mod service;
pub mod shows;
pub mod sound;
pub mod sound_edit;
pub mod studio;
pub mod views;
pub mod welcome;

pub use error::{AppError, Result};
pub use methods::{methods, Access, MethodInfo, Session};
pub use service::Service;
