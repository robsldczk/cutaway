//! Cutaway core: document model, command bus, undo, project I/O.

pub mod brand {
    //! Product names, generated from `brand.json` (see `docs/specs/architecture.md`, Naming).
    include!(concat!(env!("OUT_DIR"), "/brand.rs"));

    #[cfg(test)]
    mod tests {
        /// The app bundle says who it's from (D161): the same as brand.json, which can't be read
        /// from tauri.conf.json.
        #[test]
        fn the_bundle_names_the_owner_and_the_copyright() {
            let conf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../app/src-tauri/tauri.conf.json");
            let text = std::fs::read_to_string(conf).unwrap();
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["bundle"]["copyright"], super::COPYRIGHT);
            assert_eq!(v["bundle"]["publisher"], super::OWNER);
            assert!(super::COPYRIGHT.contains(super::OWNER));
        }
    }
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
pub mod review;
pub mod script;
pub mod show;
pub mod sounds;
pub mod store;
pub mod story;
pub mod text;
pub mod thrl;
pub mod timeline;

pub use bus::{Change, CommandBus, Executed, Tx};
pub use commands::{AnalysisPayload, Command};
pub use error::{CoreError, Result};
pub use model::*;
