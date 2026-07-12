//! Adapters that connect app-memory pattern storage to app-workflows ports.
//!
//! Keeping adapters in the composition root prevents app-workflows from
//! depending on app-memory (and vice versa).

use std::sync::Arc;

use app_memory::PatternStore;
use app_models::{AppError, OrchestrationOutcome, PatternHint};
use app_workflows::{PatternRecallQuery, PatternSink, PatternSource};
use async_trait::async_trait;

/// PatternSource/Sink over any [`PatternStore`].
pub struct PatternStoreAdapter {
    store: Arc<dyn PatternStore>,
}

impl PatternStoreAdapter {
    /// Wrap a pattern store for orchestration ports.
    #[must_use]
    pub fn new(store: Arc<dyn PatternStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl PatternSource for PatternStoreAdapter {
    async fn recall(&self, query: PatternRecallQuery) -> Result<Vec<PatternHint>, AppError> {
        self.store.recall_patterns(&query.task, query.workspace_id, query.limit).await
    }
}

#[async_trait]
impl PatternSink for PatternStoreAdapter {
    async fn observe(&self, outcome: OrchestrationOutcome) -> Result<(), AppError> {
        self.store.apply_outcome(&outcome).await
    }
}
