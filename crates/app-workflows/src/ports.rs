//! Loose-coupling ports between orchestration and the memory layer.
//!
//! Orchestration never imports `app-memory`. The composition root (Tauri)
//! provides adapters that implement these ports. No-op defaults keep
//! multi-agent runnable when memory is unavailable or being upgraded.

use app_models::{AppError, OrchestrationOutcome, PatternHint, WorkspaceId};
use async_trait::async_trait;

/// Query used to recall user/workspace work patterns.
#[derive(Debug, Clone)]
pub struct PatternRecallQuery {
    pub task: String,
    pub workspace_id: Option<WorkspaceId>,
    pub limit: usize,
}

/// Read-side port: orchestration pulls planning priors from memory.
#[async_trait]
pub trait PatternSource: Send + Sync {
    /// Recall ranked pattern hints for a task.
    async fn recall(&self, query: PatternRecallQuery) -> Result<Vec<PatternHint>, AppError>;
}

/// Write-side port: orchestration reports outcomes so memory can learn.
#[async_trait]
pub trait PatternSink: Send + Sync {
    /// Observe a finished orchestration without coupling to workflow internals.
    async fn observe(&self, outcome: OrchestrationOutcome) -> Result<(), AppError>;
}

/// Default source that returns no patterns (safe when memory is offline).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPatternSource;

#[async_trait]
impl PatternSource for NoopPatternSource {
    async fn recall(&self, _query: PatternRecallQuery) -> Result<Vec<PatternHint>, AppError> {
        Ok(Vec::new())
    }
}

/// Default sink that discards outcomes.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPatternSink;

#[async_trait]
impl PatternSink for NoopPatternSink {
    async fn observe(&self, _outcome: OrchestrationOutcome) -> Result<(), AppError> {
        Ok(())
    }
}
