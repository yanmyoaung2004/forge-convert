//! Application layer: orchestrator, batch pool, filesystem, naming.
//!
//! Rule: depends on `forge-core` ONLY (ports injected — never concrete
//! adapters). Adapters (`forge-image`, `forge-pdf`, `forge-store`) and
//! `apps/*` compose these pieces with real implementations.

mod batch;
mod filesystem;
mod naming;
mod orchestrator;

pub use batch::{run_batch, split_request, BatchConfig, BatchReport, CancelFlag, NullSink};
pub use filesystem::StdFileSystem;
pub use naming::resolve_output;
pub use orchestrator::{EngineDeps, Orchestrator};
