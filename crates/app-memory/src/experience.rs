//! Durable experience events produced from finished agent runs.

use app_models::{
    AgentRunId, AppError, ExperienceEvent, ExperienceEventId, OutcomeSignal, ThreadId, WorkspaceId,
};
use async_trait::async_trait;
use sqlx::SqlitePool;

/// Current experience event schema / extractor version for idempotent re-analysis.
pub const EXPERIENCE_SCHEMA_VERSION: u32 = 1;

/// Persistence port for experience events.
#[async_trait]
pub trait ExperienceStore: Send + Sync {
    /// Insert or replace an experience event for `(run_id, schema_version)`.
    async fn upsert_event(&self, event: &ExperienceEvent) -> Result<(), AppError>;

    /// Fetch the experience event for a run at the current schema version.
    async fn get_by_run(
        &self,
        run_id: AgentRunId,
        schema_version: u32,
    ) -> Result<Option<ExperienceEvent>, AppError>;

    /// List recent experience events for a workspace.
    async fn list_for_workspace(
        &self,
        workspace_id: WorkspaceId,
        limit: usize,
    ) -> Result<Vec<ExperienceEvent>, AppError>;
}

/// SQLite-backed [`ExperienceStore`].
#[derive(Debug, Clone)]
pub struct SqliteExperienceStore {
    pool: SqlitePool,
}

impl SqliteExperienceStore {
    /// Create a store over an existing pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExperienceStore for SqliteExperienceStore {
    async fn upsert_event(&self, event: &ExperienceEvent) -> Result<(), AppError> {
        let payload = serde_json::to_string(event).map_err(|e| AppError::Internal {
            message: format!("serialize experience event failed: {e}"),
        })?;
        sqlx::query(
            r"
            INSERT INTO experience_events (
                id, run_id, workspace_id, thread_id, task_kind, execution_mode,
                payload_json, outcome, schema_version, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id, schema_version) DO UPDATE SET
                payload_json = excluded.payload_json,
                outcome = excluded.outcome,
                task_kind = excluded.task_kind,
                execution_mode = excluded.execution_mode
            ",
        )
        .bind(event.id.0)
        .bind(event.run_id.0)
        .bind(event.workspace_id.0)
        .bind(event.thread_id.0)
        .bind(event.task_kind.as_str())
        .bind(event.execution_mode.as_str())
        .bind(payload)
        .bind(event.outcome.as_str())
        .bind(i64::from(event.schema_version))
        .bind(event.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("upsert experience event failed: {e}"),
        })?;
        Ok(())
    }

    async fn get_by_run(
        &self,
        run_id: AgentRunId,
        schema_version: u32,
    ) -> Result<Option<ExperienceEvent>, AppError> {
        let row = sqlx::query_as::<_, ExperienceRow>(
            r"
            SELECT payload_json FROM experience_events
            WHERE run_id = ? AND schema_version = ?
            ",
        )
        .bind(run_id.0)
        .bind(i64::from(schema_version))
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get experience event failed: {e}"),
        })?;
        row.map(|r| r.into_event()).transpose()
    }

    async fn list_for_workspace(
        &self,
        workspace_id: WorkspaceId,
        limit: usize,
    ) -> Result<Vec<ExperienceEvent>, AppError> {
        let limit = i64::try_from(limit.clamp(1, 200)).unwrap_or(50);
        let rows = sqlx::query_as::<_, ExperienceRow>(
            r"
            SELECT payload_json FROM experience_events
            WHERE workspace_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            ",
        )
        .bind(workspace_id.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list experience events failed: {e}"),
        })?;
        rows.into_iter().map(ExperienceRow::into_event).collect()
    }
}

#[derive(sqlx::FromRow)]
struct ExperienceRow {
    payload_json: String,
}

impl ExperienceRow {
    fn into_event(self) -> Result<ExperienceEvent, AppError> {
        serde_json::from_str(&self.payload_json).map_err(|e| AppError::Internal {
            message: format!("deserialize experience event failed: {e}"),
        })
    }
}

/// Build a skeleton experience event id + timestamps for callers that fill the rest.
#[must_use]
pub fn new_event_id() -> ExperienceEventId {
    ExperienceEventId::new()
}

/// Map terminal agent status to a coarse outcome before user feedback.
#[must_use]
pub fn outcome_from_status(status: app_models::AgentRunStatus) -> OutcomeSignal {
    use app_models::AgentRunStatus;
    match status {
        AgentRunStatus::Completed => OutcomeSignal::Unknown,
        AgentRunStatus::Failed => OutcomeSignal::Failed,
        AgentRunStatus::Cancelled | AgentRunStatus::Interrupted => OutcomeSignal::Cancelled,
        _ => OutcomeSignal::Unknown,
    }
}

/// Helper for tests and callers that only need ids.
#[must_use]
pub fn ids_for_run(
    run_id: AgentRunId,
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
) -> (ExperienceEventId, AgentRunId, WorkspaceId, ThreadId) {
    (ExperienceEventId::new(), run_id, workspace_id, thread_id)
}
