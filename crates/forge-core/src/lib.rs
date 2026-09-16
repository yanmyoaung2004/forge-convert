// forge-core: domain types + ports + structured errors.
// Hexagonal rule: this crate depends on NOTHING internal, no Tauri,
// no SQLite, no codec crates. Phase 1 fills in the model (docs/PLAN.md).
//! ForgeConvert core domain: types, ports, and structured errors.
//!
//! Layer: domain + ports (hexagonal). No internal, Tauri, SQLite,
//! or codec dependencies allowed here.

pub mod domain;
pub mod error;
pub mod ports;

// Phase 1 adds: pub use domain::*; pub use error::ForgeError;
