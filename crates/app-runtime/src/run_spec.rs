//! In-process binding of [`RunExecutionSpec`] to a run id for one resolve pass.

use app_models::{AgentRunId, RunExecutionSpec};
use std::collections::HashMap;
use std::sync::Mutex;

/// Shared map used by the runtime and executor resolver.
#[derive(Debug, Default)]
pub struct RunExecutionSpecStore {
    inner: Mutex<HashMap<AgentRunId, RunExecutionSpec>>,
}

impl RunExecutionSpecStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a spec for the next resolve of `run_id` (overwrites prior binding).
    pub fn bind(&self, run_id: AgentRunId, spec: RunExecutionSpec) {
        self.inner
            .lock()
            .expect("run spec store poisoned")
            .insert(run_id, spec);
    }

    /// Take a bound spec (consumes so a second resolve falls back to full tools).
    pub fn take(&self, run_id: AgentRunId) -> Option<RunExecutionSpec> {
        self.inner
            .lock()
            .expect("run spec store poisoned")
            .remove(&run_id)
    }

    /// Peek without removing.
    #[must_use]
    pub fn get(&self, run_id: AgentRunId) -> Option<RunExecutionSpec> {
        self.inner
            .lock()
            .expect("run spec store poisoned")
            .get(&run_id)
            .cloned()
    }
}
