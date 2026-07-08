//! Background task queue backed by persistent storage.

use app_models::{
    AgentRunId, AppError, BackgroundTask, BackgroundTaskId, BackgroundTaskStatus, TaskKind,
    ThreadId, WorkspaceId,
};
use chrono::{Duration, Utc};
use std::sync::Arc;

use crate::storage::Storage;

const DEFAULT_MAX_ATTEMPTS: u32 = 3;
const INITIAL_BACKOFF: Duration = Duration::seconds(2);
const MAX_BACKOFF: Duration = Duration::seconds(300);

/// A durable FIFO-ish queue for background work.
///
/// Tasks are ordered primarily by priority, then by scheduled time. Workers
/// lease tasks atomically; stalled leases can be recovered and retried with
/// exponential backoff.
#[derive(Clone)]
pub struct BackgroundTaskQueue {
    storage: Arc<dyn Storage>,
}

impl BackgroundTaskQueue {
    /// Create a new queue backed by the given storage.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Enqueue a new background task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task cannot be persisted.
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue(
        &self,
        workspace_id: WorkspaceId,
        thread_id: Option<ThreadId>,
        run_id: Option<AgentRunId>,
        task_kind: TaskKind,
        payload: serde_json::Value,
        priority: i64,
        scheduled_at: Option<chrono::DateTime<Utc>>,
        max_attempts: Option<u32>,
    ) -> Result<BackgroundTask, AppError> {
        let now = Utc::now();
        let task = BackgroundTask {
            id: BackgroundTaskId::new(),
            workspace_id,
            thread_id,
            run_id,
            task_kind,
            payload,
            status: BackgroundTaskStatus::Queued,
            priority,
            attempts: 0,
            max_attempts: max_attempts.unwrap_or(DEFAULT_MAX_ATTEMPTS),
            scheduled_at: scheduled_at.unwrap_or(now),
            leased_at: None,
            leased_by: None,
            next_retry_at: None,
            error_message: None,
            result_summary: None,
            created_at: now,
            updated_at: now,
        };
        self.storage.create_background_task(&task).await?;
        Ok(task)
    }

    /// Atomically lease the next available queued task.
    ///
    /// # Errors
    ///
    /// Returns an error if the lease query fails.
    pub async fn lease_next(
        &self,
        worker_id: impl Into<String>,
        lease_duration: Duration,
    ) -> Result<Option<BackgroundTask>, AppError> {
        let worker_id = worker_id.into();
        self.storage
            .lease_next_background_task(&worker_id, lease_duration, Utc::now())
            .await
    }

    /// Mark a task as completed with a short result summary.
    ///
    /// # Errors
    ///
    /// Returns an error if the task is missing.
    pub async fn complete(
        &self,
        id: BackgroundTaskId,
        result_summary: impl Into<String>,
    ) -> Result<(), AppError> {
        self.storage
            .complete_background_task(id, &result_summary.into(), Utc::now())
            .await
    }

    /// Record a task failure and schedule a retry if attempts remain.
    ///
    /// # Errors
    ///
    /// Returns an error if the task is missing or cannot be updated.
    pub async fn fail(
        &self,
        id: BackgroundTaskId,
        error_message: impl Into<String>,
    ) -> Result<(), AppError> {
        let error_message = error_message.into();
        let task = self.storage.get_background_task(id).await?;

        if task.attempts >= task.max_attempts {
            self.storage.fail_background_task(id, &error_message, None, Utc::now()).await
        } else {
            let backoff = Self::compute_backoff(task.attempts);
            let next_retry_at = Utc::now() + backoff;
            self.storage
                .fail_background_task(id, &error_message, Some(next_retry_at), Utc::now())
                .await
        }
    }

    /// Cancel a queued or running task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task is missing.
    pub async fn cancel(&self, id: BackgroundTaskId) -> Result<(), AppError> {
        self.storage.cancel_background_task(id, Utc::now()).await
    }

    /// Reset tasks whose leases have expired back to queued.
    ///
    /// # Errors
    ///
    /// Returns an error if the recovery query fails.
    pub async fn recover_stalled(&self, lease_timeout: Duration) -> Result<u64, AppError> {
        let cutoff = Utc::now() - lease_timeout;
        self.storage.recover_stalled_background_tasks(cutoff, Utc::now()).await
    }

    fn compute_backoff(attempts: u32) -> Duration {
        let multiplier = 2_u64.saturating_pow(attempts);
        let backoff = INITIAL_BACKOFF * i32::try_from(multiplier).unwrap_or(i32::MAX);
        if backoff > MAX_BACKOFF {
            MAX_BACKOFF
        } else {
            backoff
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStorage;

    async fn setup() -> (BackgroundTaskQueue, WorkspaceId, ThreadId, AgentRunId) {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let workspace = storage
            .create_workspace("test", "/tmp/test", false)
            .await
            .expect("create workspace");
        let thread = storage.create_thread(workspace.id, "thread").await.expect("create thread");
        let run = storage.create_run(workspace.id, thread.id).await.expect("create run");
        (
            BackgroundTaskQueue::new(storage),
            workspace.id,
            thread.id,
            run.id,
        )
    }

    #[tokio::test]
    async fn enqueue_and_lease_task() {
        let (queue, workspace_id, thread_id, run_id) = setup().await;

        let task = queue
            .enqueue(
                workspace_id,
                Some(thread_id),
                Some(run_id),
                TaskKind::AgentRun,
                serde_json::json!({"key": "value"}),
                1,
                None,
                None,
            )
            .await
            .expect("enqueue");

        let leased = queue.lease_next("worker-1", Duration::seconds(60)).await.expect("lease");
        assert!(leased.is_some());
        let leased = leased.unwrap();
        assert_eq!(leased.id, task.id);
        assert_eq!(leased.status, BackgroundTaskStatus::Running);
        assert_eq!(leased.leased_by.as_deref(), Some("worker-1"));
        assert_eq!(leased.attempts, 1);

        let second =
            queue.lease_next("worker-2", Duration::seconds(60)).await.expect("lease second");
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn complete_task() {
        let (queue, workspace_id, _, _) = setup().await;

        let task = queue
            .enqueue(
                workspace_id,
                None,
                None,
                TaskKind::Routine,
                serde_json::Value::Null,
                0,
                None,
                None,
            )
            .await
            .expect("enqueue");

        queue.lease_next("worker", Duration::seconds(60)).await.expect("lease");
        queue.complete(task.id, "done").await.expect("complete");

        let completed = queue.storage.get_background_task(task.id).await.expect("get");
        assert_eq!(completed.status, BackgroundTaskStatus::Completed);
        assert_eq!(completed.result_summary.as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn fail_with_retry_then_terminal() {
        let (queue, workspace_id, _, _) = setup().await;

        let task = queue
            .enqueue(
                workspace_id,
                None,
                None,
                TaskKind::ScheduledJob,
                serde_json::Value::Null,
                0,
                None,
                Some(2),
            )
            .await
            .expect("enqueue");

        queue.lease_next("worker", Duration::seconds(60)).await.expect("lease");
        queue.fail(task.id, "first error").await.expect("fail");

        let retry = queue.storage.get_background_task(task.id).await.expect("get");
        assert_eq!(retry.status, BackgroundTaskStatus::Queued);
        assert!(retry.next_retry_at.is_some());

        queue.lease_next("worker", Duration::seconds(60)).await.expect("lease");
        queue.fail(task.id, "second error").await.expect("fail");

        let failed = queue.storage.get_background_task(task.id).await.expect("get");
        assert_eq!(failed.status, BackgroundTaskStatus::Failed);
    }

    #[tokio::test]
    async fn cancel_task() {
        let (queue, workspace_id, _, _) = setup().await;

        let task = queue
            .enqueue(
                workspace_id,
                None,
                None,
                TaskKind::Routine,
                serde_json::Value::Null,
                0,
                None,
                None,
            )
            .await
            .expect("enqueue");

        queue.cancel(task.id).await.expect("cancel");
        let cancelled = queue.storage.get_background_task(task.id).await.expect("get");
        assert_eq!(cancelled.status, BackgroundTaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn recover_stalled_leases() {
        let (queue, workspace_id, _, _) = setup().await;

        let task = queue
            .enqueue(
                workspace_id,
                None,
                None,
                TaskKind::Routine,
                serde_json::Value::Null,
                0,
                None,
                None,
            )
            .await
            .expect("enqueue");

        queue.lease_next("worker", Duration::seconds(1)).await.expect("lease");
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        let recovered = queue.recover_stalled(Duration::seconds(1)).await.expect("recover");
        assert_eq!(recovered, 1);

        let recovered_task = queue.storage.get_background_task(task.id).await.expect("get");
        assert_eq!(recovered_task.status, BackgroundTaskStatus::Queued);
        assert_eq!(recovered_task.attempts, 2);
    }
}
