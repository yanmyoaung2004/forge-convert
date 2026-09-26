//! Cancel registry: named `CancelFlag`s keyed by job id, so a UI
//! Cancel button (or a second CLI invocation in future) can abort a
//! running conversion. Engine stays single-crate; adapters never touch this.
//!
//! Usage: `registry.register(&id)` → run with the returned flag → `registry.cancel(&id)`
//! from the cancel path → `registry.remove(&id)` when done (prevents growth).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use forge_core::JobId;

use crate::batch::CancelFlag;

/// Thread-safe job-id → cancel-flag map.
#[derive(Debug, Default)]
pub struct CancelRegistry {
    flags: Mutex<HashMap<String, Arc<CancelFlag>>>,
}

impl CancelRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a fresh flag for `id` (replaces any stale entry).
    pub fn register(&self, id: &JobId) -> Arc<CancelFlag> {
        let flag = CancelFlag::new();
        self.flags
            .lock()
            .expect("cancel registry lock")
            .insert(id.0.clone(), flag.clone());
        flag
    }

    /// Request cancellation for `id`. Returns false when unknown/already done.
    pub fn cancel(&self, id: &JobId) -> bool {
        match self.flags.lock().expect("cancel registry lock").get(&id.0) {
            Some(flag) => {
                flag.cancel();
                true
            }
            None => false,
        }
    }

    /// Drop the entry (call when the job settles; keeps the map bounded).
    pub fn remove(&self, id: &JobId) {
        self.flags
            .lock()
            .expect("cancel registry lock")
            .remove(&id.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::CancelToken;

    #[test]
    fn test_register_cancel_remove_cycle() {
        let registry = CancelRegistry::new();
        let id = JobId::generate();
        assert!(!registry.cancel(&id), "unknown id cancels nothing");
        let flag = registry.register(&id);
        assert!(!flag.is_cancelled());
        assert!(registry.cancel(&id));
        assert!(flag.is_cancelled());
        registry.remove(&id);
        assert!(!registry.cancel(&id), "removed id cancels nothing");
    }
}
