//! The service layer shared by every client: the Tauri UI, the `cutaway` CLI, the MCP
//! server and the HTTP dev bridge all go through `Session::call` / `Service::call`.

pub mod bridge;
pub mod buckets;
pub mod demo;
pub mod dev;
pub mod error;
pub mod import;
pub mod jobs;
pub mod methods;
pub mod player;
pub mod service;
pub mod shows;
pub mod sound;
pub mod sound_edit;
pub mod studio;
pub mod views;
pub mod vocals;

pub use error::{AppError, Result};
pub use methods::{methods, Access, MethodInfo, Session};
pub use service::Service;
