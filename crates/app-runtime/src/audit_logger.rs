//! SQLite-backed [`AuditLogger`] implementation.

use crate::storage::Storage;
use app_models::{AgentRunId, AppError, ThreadId, WorkspaceId};
use app_security::{AuditEvent, AuditLogger, PermissionResult};
use std::sync::Arc;

/// Persists audit events via a [`Storage`] backend.
#[derive(Clone)]
pub struct SqliteAuditLogger {
    storage: Arc<dyn Storage>,
}

impl SqliteAuditLogger {
    /// Create a new SQLite-backed audit logger.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }
}

impl AuditLogger for SqliteAuditLogger {
    fn log(&self, event: AuditEvent) -> Result<(), AppError> {
        let (decision, reason) = match &event.outcome {
            PermissionResult::Allowed => ("Allowed".to_owned(), None),
            PermissionResult::Ask { .. } => ("Ask".to_owned(), None),
            PermissionResult::Denied { reason } => ("Denied".to_owned(), Some(reason.clone())),
        };

        let fut = self.storage.append_audit_log(
            event.workspace_id,
            event.thread_id,
            event.run_id,
            &event.action,
            &event.resource,
            &decision,
            reason.as_deref(),
        );

        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut))
    }
}

/// Helper to build an [`AuditEvent`] for run lifecycle actions.
#[must_use]
pub fn run_audit_event(
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
    run_id: AgentRunId,
    action: &str,
    outcome: PermissionResult,
) -> AuditEvent {
    AuditEvent {
        workspace_id,
        thread_id: Some(thread_id),
        run_id: Some(run_id),
        action: action.to_owned(),
        resource: run_id.0.to_string(),
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStorage;
    use std::sync::Arc;

    #[tokio::test(flavor = "multi_thread")]
    async fn sqlite_audit_logger_persists_events() {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let logger = SqliteAuditLogger::new(storage.clone());
        let workspace_id = WorkspaceId::new();

        logger
            .log(AuditEvent {
                workspace_id,
                thread_id: None,
                run_id: None,
                action: "test.action".to_owned(),
                resource: "resource".to_owned(),
                outcome: PermissionResult::Allowed,
            })
            .expect("log event");

        let entries = storage
            .list_audit_log(Some(workspace_id), None, None)
            .await
            .expect("list audit log");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "test.action");
        assert_eq!(entries[0].decision, "Allowed");
    }
}
