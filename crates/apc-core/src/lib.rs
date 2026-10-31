//! Cutaway core: document model, command bus, undo, project I/O.

pub mod brand {
    //! Product names, generated from `brand.json` (see `docs/specs/architecture.md`, Naming).
    include!(concat!(env!("OUT_DIR"), "/brand.rs"));
}
