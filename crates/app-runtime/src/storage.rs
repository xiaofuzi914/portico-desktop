//! Persistent storage abstraction and `SQLite` implementation for Portico.

use app_models::{
    AgentRun, AgentRunId, AgentRunStatus, AppError, ApprovalRequest, ApprovalRequestId,
    ApprovalRequestStatus, Automation, AutomationId, AutomationTrigger, BackgroundTask,
    BackgroundTaskId, BackgroundTaskStatus, Message, MessageId, MessageRole, Notification,
    NotificationCategory, NotificationId, Orchestration, OrchestrationId, OrchestrationPlan,
    OrchestrationStatus, RunEvent, SubagentRun, TaskKind, Thread, ThreadId, ToolInvocation,
    ToolInvocationId, ToolInvocationStatus, WorkflowPatternId, Workspace, WorkspaceId, Worktree,
    WorktreeId,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use std::path::Path;

const MAX_MESSAGE_CHARS: usize = 32_000;
const MAX_CLIENT_REQUEST_ID_CHARS: usize = 128;
const INSERT_TOOL_INVOCATION_SQL: &str = "INSERT INTO tool_invocations (
        id, run_id, thread_id, workspace_id, model_call_id, tool_name, tool_version,
        action, resource, arguments_json, request_hash, policy_version, context_revision, status,
        approval_request_id, result_json, error, recovery_json, lease_token, attempts,
        cancel_requested, correlation_id, created_at, updated_at, started_at, completed_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

fn validate_message_input<'a>(
    content: &'a str,
    client_request_id: Option<&'a str>,
) -> Result<(&'a str, Option<&'a str>), AppError> {
    let content = content.trim();
    if content.is_empty() {
        return Err(AppError::Internal {
            message: "INVALID_MESSAGE: message content must not be empty".to_owned(),
        });
    }
    if content.chars().count() > MAX_MESSAGE_CHARS
        || content
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(AppError::Internal {
            message: format!(
                "INVALID_MESSAGE: message content must be at most {MAX_MESSAGE_CHARS} characters and contain no unsupported control characters"
            ),
        });
    }

    let client_request_id = client_request_id
        .map(str::trim)
        .map(|request_id| {
            let valid = !request_id.is_empty()
                && request_id.chars().count() <= MAX_CLIENT_REQUEST_ID_CHARS
                && request_id.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(character, '-' | '_' | ':' | '.')
                });
            valid.then_some(request_id).ok_or_else(|| AppError::Internal {
                message: "INVALID_CLIENT_REQUEST_ID: use 1 to 128 ASCII letters, digits, '-', '_', ':' or '.'"
                    .to_owned(),
            })
        })
        .transpose()?;

    Ok((content, client_request_id))
}

/// Persistence operations required by the Portico runtime.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Access the underlying `SQLite` connection pool.
    ///
    /// This exposes the pool so that other runtime subsystems (e.g. the memory
    /// manager) can share the same database connection.
    fn pool(&self) -> &SqlitePool;

    /// Create a new workspace.
    async fn create_workspace(
        &self,
        name: &str,
        root_path: &str,
        trusted: bool,
    ) -> Result<Workspace, AppError>;

    /// List all workspaces ordered by creation time.
    async fn list_workspaces(&self) -> Result<Vec<Workspace>, AppError>;

    /// Fetch a workspace by id.
    async fn get_workspace(&self, id: WorkspaceId) -> Result<Workspace, AppError>;

    /// Create a new thread inside a workspace.
    async fn create_thread(
        &self,
        workspace_id: WorkspaceId,
        title: &str,
    ) -> Result<Thread, AppError>;

    /// List threads in a workspace ordered by creation time.
    async fn list_threads(&self, workspace_id: WorkspaceId) -> Result<Vec<Thread>, AppError>;

    /// Delete a thread and its cascade-owned conversation data.
    async fn delete_thread(&self, workspace_id: WorkspaceId, id: ThreadId) -> Result<(), AppError>;

    /// Fetch a thread by id.
    async fn get_thread(&self, id: ThreadId) -> Result<Thread, AppError>;

    /// Update a thread's display title (topic label for the session list).
    async fn update_thread_title(&self, id: ThreadId, title: &str) -> Result<Thread, AppError>;

    /// Persist a user message and its new queued run atomically. A repeated
    /// client request id returns the original pair instead of duplicating work.
    async fn create_message_and_run(
        &self,
        thread_id: ThreadId,
        content: &str,
        client_request_id: Option<&str>,
    ) -> Result<(Message, AgentRun, bool), AppError>;

    /// List durable messages for a thread in chronological order.
    async fn list_messages(&self, thread_id: ThreadId) -> Result<Vec<Message>, AppError>;

    /// Persist one message generated during a run. Repeating the assistant
    /// completion for the same run returns the original durable message.
    async fn create_run_message(
        &self,
        thread_id: ThreadId,
        run_id: AgentRunId,
        role: MessageRole,
        content: &str,
    ) -> Result<Message, AppError>;

    /// List runs belonging to a thread, newest first.
    async fn list_runs(&self, thread_id: ThreadId) -> Result<Vec<AgentRun>, AppError>;

    /// Persist token usage for a run (uses `run_model_snapshots` for provider/model ids).
    async fn record_run_token_usage(
        &self,
        run_id: AgentRunId,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<(), AppError>;

    /// Sum token usage recorded for a run.
    async fn get_run_token_usage(
        &self,
        run_id: AgentRunId,
    ) -> Result<(u64, u64), AppError>;

    /// Mark runs that cannot survive a process restart as interrupted.
    async fn recover_incomplete_runs(&self) -> Result<u64, AppError>;

    /// Create a new run inside a workspace and thread.
    async fn create_run(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<AgentRun, AppError>;

    /// Fetch a run by id.
    async fn get_run(&self, id: AgentRunId) -> Result<AgentRun, AppError>;

    /// Update the status of a run.
    async fn update_run_status(
        &self,
        id: AgentRunId,
        status: AgentRunStatus,
    ) -> Result<(), AppError>;

    /// Atomically resume a run only when it is still waiting for approval.
    async fn resume_waiting_run(&self, id: AgentRunId) -> Result<bool, AppError>;

    /// Append an event to a run's timeline.
    async fn append_event(
        &self,
        run_id: AgentRunId,
        thread_id: ThreadId,
        sequence: i64,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<RunEvent, AppError>;

    /// List events for a run ordered by sequence.
    async fn list_run_events(&self, run_id: AgentRunId) -> Result<Vec<RunEvent>, AppError>;

    /// Save the normalized provider-independent chat checkpoint for a run.
    async fn save_agent_checkpoint(
        &self,
        run_id: AgentRunId,
        messages: &serde_json::Value,
        step: i64,
    ) -> Result<(), AppError>;

    /// Load the latest chat checkpoint for a run.
    async fn load_agent_checkpoint(
        &self,
        run_id: AgentRunId,
    ) -> Result<Option<AgentCheckpoint>, AppError>;

    /// Append a security audit log entry.
    #[allow(clippy::too_many_arguments)]
    async fn append_audit_log(
        &self,
        workspace_id: WorkspaceId,
        thread_id: Option<ThreadId>,
        run_id: Option<AgentRunId>,
        action: &str,
        resource: &str,
        decision: &str,
        reason: Option<&str>,
    ) -> Result<(), AppError>;

    /// List audit log entries, optionally filtered by workspace, thread, or run.
    async fn list_audit_log(
        &self,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        run_id: Option<AgentRunId>,
    ) -> Result<Vec<AuditLogEntry>, AppError>;

    /// Update the trusted flag of a workspace.
    async fn update_workspace_trusted(
        &self,
        id: WorkspaceId,
        trusted: bool,
    ) -> Result<Workspace, AppError>;

    /// Replace the allowed read/write paths for a workspace.
    async fn set_workspace_allowed_paths(
        &self,
        id: WorkspaceId,
        read_paths: Vec<String>,
        write_paths: Vec<String>,
    ) -> Result<(), AppError>;

    /// Create a new worktree.
    async fn create_worktree(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
        name: &str,
        path: &str,
    ) -> Result<Worktree, AppError>;

    /// Delete a worktree by id.
    async fn delete_worktree(&self, id: WorktreeId) -> Result<(), AppError>;

    /// List worktrees in a workspace.
    async fn list_worktrees(&self, workspace_id: WorkspaceId) -> Result<Vec<Worktree>, AppError>;

    /// Fetch a worktree by id.
    async fn get_worktree(&self, id: WorktreeId) -> Result<Worktree, AppError>;

    /// Persist a subagent run.
    async fn create_subagent(&self, subagent: &SubagentRun) -> Result<(), AppError>;

    /// Fetch a subagent run by id.
    async fn get_subagent(&self, id: AgentRunId) -> Result<SubagentRun, AppError>;

    /// List subagent runs for a parent run.
    async fn list_subagents(&self, parent_run_id: AgentRunId)
    -> Result<Vec<SubagentRun>, AppError>;

    /// Update the status and optional output summary of a subagent run.
    async fn update_subagent_status(
        &self,
        id: AgentRunId,
        status: AgentRunStatus,
        output_summary: Option<&str>,
    ) -> Result<(), AppError>;

    /// Associate a subagent with the concrete child run that executes its task.
    async fn update_subagent_child_run(
        &self,
        id: AgentRunId,
        child_run_id: AgentRunId,
    ) -> Result<(), AppError>;

    /// Fetch the concrete child run id for a subagent, if one has been started.
    async fn get_subagent_child_run(&self, id: AgentRunId) -> Result<Option<AgentRunId>, AppError>;

    // Background tasks

    /// Persist a background task.
    async fn create_background_task(&self, task: &BackgroundTask) -> Result<(), AppError>;

    /// Fetch a background task by id.
    async fn get_background_task(&self, id: BackgroundTaskId) -> Result<BackgroundTask, AppError>;

    /// List background tasks, optionally filtered by workspace and status.
    async fn list_background_tasks(
        &self,
        workspace_id: Option<WorkspaceId>,
        status: Option<BackgroundTaskStatus>,
        limit: i64,
    ) -> Result<Vec<BackgroundTask>, AppError>;

    /// Atomically lease the next available queued background task.
    async fn lease_next_background_task(
        &self,
        worker_id: &str,
        lease_duration: chrono::Duration,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<BackgroundTask>, AppError>;

    /// Mark a background task as completed.
    async fn complete_background_task(
        &self,
        id: BackgroundTaskId,
        result_summary: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError>;

    /// Mark a background task as failed or schedule a retry.
    async fn fail_background_task(
        &self,
        id: BackgroundTaskId,
        error_message: &str,
        next_retry_at: Option<chrono::DateTime<chrono::Utc>>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError>;

    /// Cancel a background task.
    async fn cancel_background_task(
        &self,
        id: BackgroundTaskId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError>;

    /// Reset stalled running tasks back to queued and increment attempts.
    async fn recover_stalled_background_tasks(
        &self,
        lease_timeout: chrono::DateTime<chrono::Utc>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, AppError>;

    // Automations

    /// Persist an automation.
    async fn create_automation(&self, automation: &Automation) -> Result<(), AppError>;

    /// Fetch an automation by id.
    async fn get_automation(&self, id: AutomationId) -> Result<Automation, AppError>;

    /// List automations in a workspace.
    async fn list_automations(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<Automation>, AppError>;

    /// List enabled automations that are due to run at or before `now`.
    async fn list_due_automations(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Automation>, AppError>;

    /// Update an automation.
    async fn update_automation(&self, automation: &Automation) -> Result<(), AppError>;

    /// Delete an automation.
    async fn delete_automation(&self, id: AutomationId) -> Result<(), AppError>;

    // Notifications

    /// Persist a notification.
    async fn create_notification(&self, notification: &Notification) -> Result<(), AppError>;

    /// List notifications in a workspace, or across all workspaces when `None`.
    async fn list_notifications(
        &self,
        workspace_id: Option<WorkspaceId>,
        unread_only: bool,
        limit: i64,
    ) -> Result<Vec<Notification>, AppError>;

    /// Fetch a notification by id.
    async fn get_notification(&self, id: NotificationId) -> Result<Notification, AppError>;

    /// Mark a notification as read or unread.
    async fn mark_notification_read(&self, id: NotificationId, read: bool) -> Result<(), AppError>;

    /// Delete a notification.
    async fn dismiss_notification(&self, id: NotificationId) -> Result<(), AppError>;

    // Approval requests

    /// Persist a new approval request.
    async fn create_approval_request(
        &self,
        request: &ApprovalRequest,
    ) -> Result<ApprovalRequest, AppError>;

    /// Fetch an approval request by id.
    async fn get_approval_request(
        &self,
        id: ApprovalRequestId,
    ) -> Result<ApprovalRequest, AppError>;

    /// List pending approval requests, optionally filtered by run.
    async fn list_pending_approval_requests(
        &self,
        run_id: Option<AgentRunId>,
    ) -> Result<Vec<ApprovalRequest>, AppError>;

    /// Resolve an approval request to approved or denied.
    async fn resolve_approval_request(
        &self,
        id: ApprovalRequestId,
        status: ApprovalRequestStatus,
        reason: Option<&str>,
    ) -> Result<ApprovalRequest, AppError>;

    // Durable tool invocations

    /// Persist an invocation that is ready, denied, or otherwise does not need
    /// an approval row.
    async fn create_tool_invocation(
        &self,
        invocation: &ToolInvocation,
    ) -> Result<ToolInvocation, AppError>;

    /// Persist a waiting invocation and its approval request in one transaction.
    async fn create_tool_invocation_with_approval(
        &self,
        invocation: &ToolInvocation,
        approval: &ApprovalRequest,
    ) -> Result<(ToolInvocation, ApprovalRequest), AppError>;

    /// Fetch one invocation by id.
    async fn get_tool_invocation(&self, id: ToolInvocationId) -> Result<ToolInvocation, AppError>;

    /// Fetch the invocation linked to an approval request.
    async fn get_tool_invocation_for_approval(
        &self,
        approval_id: ApprovalRequestId,
    ) -> Result<ToolInvocation, AppError>;

    /// List invocations for one run in creation order.
    async fn list_tool_invocations(
        &self,
        run_id: AgentRunId,
    ) -> Result<Vec<ToolInvocation>, AppError>;

    /// List invocations that require evidence-based restart reconciliation.
    async fn list_tool_invocations_requiring_reconciliation(
        &self,
    ) -> Result<Vec<ToolInvocation>, AppError>;

    /// Upsert a durable multi-agent orchestration session.
    async fn upsert_orchestration(&self, session: &Orchestration) -> Result<(), AppError>;

    /// Fetch an orchestration session by id.
    async fn get_orchestration(&self, id: OrchestrationId) -> Result<Orchestration, AppError>;

    /// List orchestration sessions for a thread (newest first).
    async fn list_orchestrations_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> Result<Vec<Orchestration>, AppError>;

    /// Atomically approve or deny the linked invocation. Repeating the same
    /// decision is idempotent; a conflicting decision is rejected.
    async fn resolve_tool_approval(
        &self,
        id: ApprovalRequestId,
        status: ApprovalRequestStatus,
        reason: Option<&str>,
    ) -> Result<(ApprovalRequest, ToolInvocation), AppError>;

    /// Claim a ready or approved invocation with a unique execution lease.
    async fn claim_tool_invocation(&self, id: ToolInvocationId)
    -> Result<ToolInvocation, AppError>;

    /// Complete an executing invocation held by `lease_token`.
    async fn complete_tool_invocation(
        &self,
        id: ToolInvocationId,
        lease_token: uuid::Uuid,
        result: serde_json::Value,
    ) -> Result<ToolInvocation, AppError>;

    /// Fail an executing invocation held by `lease_token`.
    async fn fail_tool_invocation(
        &self,
        id: ToolInvocationId,
        lease_token: uuid::Uuid,
        error: &str,
    ) -> Result<ToolInvocation, AppError>;

    /// Mark a reconciliation-required invocation as successfully applied.
    async fn complete_reconciled_tool_invocation(
        &self,
        id: ToolInvocationId,
        result: serde_json::Value,
    ) -> Result<ToolInvocation, AppError>;

    /// Return a reconciliation-required invocation to its approved claimable state.
    async fn requeue_reconciled_tool_invocation(
        &self,
        id: ToolInvocationId,
    ) -> Result<ToolInvocation, AppError>;

    /// Fail a reconciliation-required invocation on an external-state conflict.
    async fn fail_reconciled_tool_invocation(
        &self,
        id: ToolInvocationId,
        error: &str,
    ) -> Result<ToolInvocation, AppError>;

    /// Cancel all not-yet-executing invocations for a run and request
    /// cancellation on any invocation already executing.
    async fn cancel_tool_invocations_for_run(&self, run_id: AgentRunId) -> Result<u64, AppError>;

    /// Move invocations abandoned in `Executing` to reconciliation-required.
    async fn recover_tool_invocations(&self) -> Result<u64, AppError>;
}

/// A persisted security audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditLogEntry {
    /// Database row id.
    pub id: i64,
    /// Workspace the audited action belonged to.
    pub workspace_id: Option<uuid::Uuid>,
    /// Optional thread scope.
    pub thread_id: Option<uuid::Uuid>,
    /// Optional run scope.
    pub run_id: Option<uuid::Uuid>,
    /// Action that was attempted.
    pub action: String,
    /// Resource the action targeted.
    pub resource: String,
    /// Decision string (`Allowed` or `Denied`).
    pub decision: String,
    /// Optional denial or approval reason.
    pub reason: Option<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Provider-independent durable conversation state for one model/tool loop.
#[derive(Debug, Clone)]
pub struct AgentCheckpoint {
    /// Owning run.
    pub run_id: AgentRunId,
    /// Serialized normalized chat messages.
    pub messages: serde_json::Value,
    /// Completed model/tool step count.
    pub step: i64,
    /// Last durable update.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// SQLite-backed [`Storage`] implementation.
#[derive(Debug, Clone)]
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    /// Open (or create) a `SQLite` database at `path` and run pending migrations.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or migrations fail.
    pub async fn open<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Internal {
                message: format!("failed to create database directory: {e}"),
            })?;
        }

        let connect_options = SqliteConnectOptions::new().filename(path).create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query(
                        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;",
                    )
                    .execute(conn)
                    .await?;
                    Ok(())
                })
            })
            .connect_with(connect_options)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to connect to sqlite: {e}"),
            })?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("migration failed: {e}"),
            })?;

        Ok(Self { pool })
    }

    /// Create an in-memory `SQLite` storage instance for tests.
    ///
    /// # Errors
    ///
    /// Returns an error if the in-memory database cannot be set up.
    pub async fn open_in_memory() -> Result<Self, AppError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(":memory:")
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to connect to in-memory sqlite: {e}"),
            })?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("migration failed: {e}"),
            })?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn create_workspace(
        &self,
        name: &str,
        root_path: &str,
        trusted: bool,
    ) -> Result<Workspace, AppError> {
        let id = WorkspaceId::new();
        let now = Utc::now();
        let trusted_i64 = i64::from(trusted);

        sqlx::query(
            "INSERT INTO workspaces (id, name, root_path, trusted, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.0)
        .bind(name)
        .bind(root_path)
        .bind(trusted_i64)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("create_workspace failed: {e}") })?;

        Ok(Workspace {
            id,
            name: name.to_owned(),
            root_path: root_path.to_owned(),
            trusted,
            allowed_read_paths: Vec::new(),
            allowed_write_paths: Vec::new(),
            created_at: now,
            updated_at: now,
        })
    }

    async fn list_workspaces(&self) -> Result<Vec<Workspace>, AppError> {
        let rows = sqlx::query_as::<_, WorkspaceRow>(
            "SELECT id, name, root_path, trusted, created_at, updated_at FROM workspaces ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("list_workspaces failed: {e}") })?;

        let mut workspaces = Vec::with_capacity(rows.len());
        for row in rows {
            let paths = self.fetch_workspace_paths(WorkspaceId(row.id)).await?;
            workspaces.push(row.into_workspace(paths.0, paths.1));
        }
        Ok(workspaces)
    }

    async fn get_workspace(&self, id: WorkspaceId) -> Result<Workspace, AppError> {
        let row = sqlx::query_as::<_, WorkspaceRow>(
            "SELECT id, name, root_path, trusted, created_at, updated_at FROM workspaces WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("get_workspace failed: {e}") })?;

        match row {
            Some(row) => {
                let paths = self.fetch_workspace_paths(id).await?;
                Ok(row.into_workspace(paths.0, paths.1))
            }
            None => Err(AppError::NotFound {
                resource: format!("workspace {id:?}"),
            }),
        }
    }

    async fn create_thread(
        &self,
        workspace_id: WorkspaceId,
        title: &str,
    ) -> Result<Thread, AppError> {
        // Verify workspace exists.
        let _ = self.get_workspace(workspace_id).await?;

        let id = ThreadId::new();
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO threads (id, workspace_id, title, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.0)
        .bind(workspace_id.0)
        .bind(title)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("create_thread failed: {e}") })?;

        Ok(Thread {
            id,
            workspace_id,
            title: title.to_owned(),
            created_at: now,
            updated_at: now,
        })
    }

    async fn list_threads(&self, workspace_id: WorkspaceId) -> Result<Vec<Thread>, AppError> {
        let rows = sqlx::query_as::<_, ThreadRow>(
            "SELECT id, workspace_id, title, created_at, updated_at FROM threads WHERE workspace_id = ? ORDER BY created_at ASC",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("list_threads failed: {e}") })?;

        Ok(rows.into_iter().map(ThreadRow::into).collect())
    }

    async fn get_thread(&self, id: ThreadId) -> Result<Thread, AppError> {
        let row = sqlx::query_as::<_, ThreadRow>(
            "SELECT id, workspace_id, title, created_at, updated_at FROM threads WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_thread failed: {e}"),
        })?;

        row.map(ThreadRow::into).ok_or_else(|| AppError::NotFound {
            resource: format!("thread {id:?}"),
        })
    }

    async fn update_thread_title(&self, id: ThreadId, title: &str) -> Result<Thread, AppError> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Err(AppError::PermissionDenied {
                reason: "thread title must not be empty".to_owned(),
            });
        }
        // Keep sidebar readable; store a bounded topic string.
        let bounded: String = trimmed.chars().take(80).collect();
        let now = Utc::now();
        let result = sqlx::query("UPDATE threads SET title = ?, updated_at = ? WHERE id = ?")
            .bind(&bounded)
            .bind(now)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("update_thread_title failed: {e}"),
            })?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("thread {id:?}"),
            });
        }
        self.get_thread(id).await
    }

    async fn delete_thread(&self, workspace_id: WorkspaceId, id: ThreadId) -> Result<(), AppError> {
        let mut transaction = self.pool.begin().await.map_err(|e| AppError::Internal {
            message: format!("begin delete_thread transaction failed: {e}"),
        })?;
        let active_runs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agent_runs WHERE thread_id = ? AND status NOT IN ('Cancelled', 'Failed', 'Interrupted', 'Completed')",
        )
        .bind(id.0)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("check thread runs failed: {e}"),
        })?;
        if active_runs > 0 {
            return Err(AppError::PermissionDenied {
                reason: "THREAD_HAS_ACTIVE_RUN: cancel or wait for the active run before deleting"
                    .to_owned(),
            });
        }
        let active_tasks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM background_tasks WHERE thread_id = ? AND status NOT IN ('Completed', 'Failed', 'Cancelled')",
        )
        .bind(id.0)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("check thread background tasks failed: {e}"),
        })?;
        if active_tasks > 0 {
            return Err(AppError::PermissionDenied {
                reason:
                    "THREAD_HAS_ACTIVE_TASK: cancel or wait for background work before deleting"
                        .to_owned(),
            });
        }
        let worktrees: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM worktrees WHERE thread_id = ?")
                .bind(id.0)
                .fetch_one(&mut *transaction)
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("check thread worktrees failed: {e}"),
                })?;
        if worktrees > 0 {
            return Err(AppError::PermissionDenied {
                reason: "THREAD_HAS_WORKTREE: remove the session worktree before deleting"
                    .to_owned(),
            });
        }

        sqlx::query("DELETE FROM memories WHERE thread_id = ?")
            .bind(id.0)
            .execute(&mut *transaction)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("delete thread memories failed: {e}"),
            })?;
        sqlx::query("DELETE FROM notifications WHERE thread_id = ?")
            .bind(id.0)
            .execute(&mut *transaction)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("delete thread notifications failed: {e}"),
            })?;
        sqlx::query("DELETE FROM background_tasks WHERE thread_id = ?")
            .bind(id.0)
            .execute(&mut *transaction)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("delete thread background tasks failed: {e}"),
            })?;
        let result = sqlx::query("DELETE FROM threads WHERE id = ? AND workspace_id = ?")
            .bind(id.0)
            .bind(workspace_id.0)
            .execute(&mut *transaction)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("delete_thread failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("thread {id:?}"),
            });
        }
        transaction.commit().await.map_err(|e| AppError::Internal {
            message: format!("commit delete_thread failed: {e}"),
        })?;
        Ok(())
    }

    async fn create_message_and_run(
        &self,
        thread_id: ThreadId,
        content: &str,
        client_request_id: Option<&str>,
    ) -> Result<(Message, AgentRun, bool), AppError> {
        let (content, client_request_id) = validate_message_input(content, client_request_id)?;

        let mut tx = self.pool.begin().await.map_err(|e| AppError::Internal {
            message: format!("begin send transaction failed: {e}"),
        })?;

        if let Some(request_id) = client_request_id {
            let existing = sqlx::query_as::<_, MessageRow>(
                "SELECT id, thread_id, run_id, role, content, client_request_id, created_at FROM messages WHERE thread_id = ? AND client_request_id = ?",
            )
            .bind(thread_id.0)
            .bind(request_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::Internal { message: format!("find idempotent message failed: {e}") })?;
            if let Some(row) = existing {
                let message: Message = row.into();
                let run_id = message.run_id.ok_or_else(|| AppError::Internal {
                    message: "idempotent user message is missing its run".to_owned(),
                })?;
                let run = sqlx::query_as::<_, AgentRunRow>(
                    "SELECT id, thread_id, workspace_id, status, created_at, started_at, completed_at FROM agent_runs WHERE id = ?",
                )
                .bind(run_id.0)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| AppError::Internal { message: format!("find idempotent run failed: {e}") })?
                .into();
                tx.commit().await.map_err(|e| AppError::Internal {
                    message: format!("commit idempotent send failed: {e}"),
                })?;
                return Ok((message, run, false));
            }
        }

        let active_runs: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM agent_runs WHERE thread_id = ? AND status IN ('Queued', 'Running', 'WaitingApproval', 'Paused')",
        )
        .bind(thread_id.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("check active thread run failed: {e}"),
        })?;
        if active_runs > 0 {
            return Err(AppError::Internal {
                message: "RUN_ALREADY_ACTIVE: wait for, cancel, or resolve the current turn before sending another message"
                    .to_owned(),
            });
        }

        let workspace_id: WorkspaceId =
            sqlx::query_scalar("SELECT workspace_id FROM threads WHERE id = ?")
                .bind(thread_id.0)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("resolve thread workspace failed: {e}"),
                })?
                .map(WorkspaceId)
                .ok_or_else(|| AppError::NotFound {
                    resource: format!("thread {thread_id:?}"),
                })?;
        let now = Utc::now();
        let run = AgentRun {
            id: AgentRunId::new(),
            thread_id,
            workspace_id,
            status: AgentRunStatus::Queued,
            created_at: now,
            started_at: None,
            completed_at: None,
        };
        let message = Message {
            id: MessageId::new(),
            thread_id,
            run_id: Some(run.id),
            role: MessageRole::User,
            content: content.to_owned(),
            client_request_id: client_request_id.map(ToOwned::to_owned),
            created_at: now,
        };
        sqlx::query("INSERT INTO agent_runs (id, thread_id, workspace_id, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(run.id.0).bind(thread_id.0).bind(workspace_id.0).bind(run.status.as_str()).bind(now).bind(now)
            .execute(&mut *tx).await.map_err(|e| AppError::Internal { message: format!("create send run failed: {e}") })?;
        sqlx::query("INSERT INTO messages (id, thread_id, run_id, role, content, client_request_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(message.id.0).bind(thread_id.0).bind(run.id.0).bind(message.role.as_str()).bind(&message.content).bind(&message.client_request_id).bind(now)
            .execute(&mut *tx).await.map_err(|e| AppError::Internal { message: format!("create user message failed: {e}") })?;
        tx.commit().await.map_err(|e| AppError::Internal {
            message: format!("commit send transaction failed: {e}"),
        })?;
        Ok((message, run, true))
    }

    async fn list_messages(&self, thread_id: ThreadId) -> Result<Vec<Message>, AppError> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, thread_id, run_id, role, content, client_request_id, created_at FROM messages WHERE thread_id = ? ORDER BY created_at ASC, id ASC",
        ).bind(thread_id.0).fetch_all(&self.pool).await
            .map_err(|e| AppError::Internal { message: format!("list messages failed: {e}") })?;
        Ok(rows.into_iter().map(MessageRow::into).collect())
    }

    async fn create_run_message(
        &self,
        thread_id: ThreadId,
        run_id: AgentRunId,
        role: MessageRole,
        content: &str,
    ) -> Result<Message, AppError> {
        let content = content.trim();
        if content.is_empty() {
            return Err(AppError::Internal {
                message: "message content must not be empty".to_owned(),
            });
        }
        let message = Message {
            id: MessageId::new(),
            thread_id,
            run_id: Some(run_id),
            role,
            content: content.to_owned(),
            client_request_id: None,
            created_at: Utc::now(),
        };
        sqlx::query("INSERT OR IGNORE INTO messages (id, thread_id, run_id, role, content, client_request_id, created_at) VALUES (?, ?, ?, ?, ?, NULL, ?)")
            .bind(message.id.0).bind(thread_id.0).bind(run_id.0).bind(role.as_str()).bind(&message.content).bind(message.created_at)
            .execute(&self.pool).await
            .map_err(|e| AppError::Internal { message: format!("create run message failed: {e}") })?;
        let row = sqlx::query_as::<_, MessageRow>(
            "SELECT id, thread_id, run_id, role, content, client_request_id, created_at FROM messages WHERE run_id = ? AND role = ? ORDER BY created_at ASC LIMIT 1",
        )
        .bind(run_id.0)
        .bind(role.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("load durable run message failed: {e}") })?;
        Ok(row.into())
    }

    async fn list_runs(&self, thread_id: ThreadId) -> Result<Vec<AgentRun>, AppError> {
        let rows = sqlx::query_as::<_, AgentRunRow>(
            "SELECT id, thread_id, workspace_id, status, created_at, started_at, completed_at FROM agent_runs WHERE thread_id = ? ORDER BY created_at DESC",
        ).bind(thread_id.0).fetch_all(&self.pool).await
            .map_err(|e| AppError::Internal { message: format!("list runs failed: {e}") })?;
        Ok(rows.into_iter().map(AgentRunRow::into).collect())
    }

    async fn record_run_token_usage(
        &self,
        run_id: AgentRunId,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<(), AppError> {
        if input_tokens == 0 && output_tokens == 0 {
            return Ok(());
        }
        let now = Utc::now();
        // Prefer snapshot ids; fall back to nil UUIDs so usage is still queryable.
        let snapshot = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid)>(
            "SELECT provider_id, model_id FROM run_model_snapshots WHERE run_id = ?",
        )
        .bind(run_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("load run model snapshot for usage failed: {e}"),
        })?;
        let (provider_id, model_id) =
            snapshot.unwrap_or((uuid::Uuid::nil(), uuid::Uuid::nil()));
        sqlx::query(
            "INSERT INTO usage_records (run_id, provider_id, model_id, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?, ?, ?, ?, ?, 0, ?)",
        )
        .bind(run_id.0)
        .bind(provider_id)
        .bind(model_id)
        .bind(i64::try_from(input_tokens).unwrap_or(0))
        .bind(i64::try_from(output_tokens).unwrap_or(0))
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("record_run_token_usage failed: {e}"),
        })?;
        Ok(())
    }

    async fn get_run_token_usage(
        &self,
        run_id: AgentRunId,
    ) -> Result<(u64, u64), AppError> {
        let row = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0)
             FROM usage_records WHERE run_id = ?",
        )
        .bind(run_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_run_token_usage failed: {e}"),
        })?;
        Ok((
            u64::try_from(row.0).unwrap_or(0),
            u64::try_from(row.1).unwrap_or(0),
        ))
    }

    async fn recover_incomplete_runs(&self) -> Result<u64, AppError> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE agent_runs SET status = 'Interrupted', completed_at = ?, updated_at = ? WHERE status IN ('Queued', 'Running', 'Paused')",
        )
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("recover incomplete runs failed: {e}") })?;
        Ok(result.rows_affected())
    }

    async fn create_run(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<AgentRun, AppError> {
        // Verify both workspace and thread exist and belong together.
        let _ = self.get_workspace(workspace_id).await?;
        let thread = self.get_thread(thread_id).await?;
        if thread.workspace_id != workspace_id {
            return Err(AppError::PermissionDenied {
                reason: "thread does not belong to workspace".to_owned(),
            });
        }

        let id = AgentRunId::new();
        let now = Utc::now();
        let status = AgentRunStatus::Queued;

        sqlx::query(
            "INSERT INTO agent_runs (id, thread_id, workspace_id, status, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.0)
        .bind(thread_id.0)
        .bind(workspace_id.0)
        .bind(status.as_str())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("create_run failed: {e}") })?;

        Ok(AgentRun {
            id,
            thread_id,
            workspace_id,
            status,
            created_at: now,
            started_at: None,
            completed_at: None,
        })
    }

    async fn get_run(&self, id: AgentRunId) -> Result<AgentRun, AppError> {
        let row = sqlx::query_as::<_, AgentRunRow>(
            "SELECT id, thread_id, workspace_id, status, created_at, started_at, completed_at FROM agent_runs WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("get_run failed: {e}") })?;

        row.map(AgentRunRow::into).ok_or_else(|| AppError::NotFound {
            resource: format!("run {id:?}"),
        })
    }

    async fn update_run_status(
        &self,
        id: AgentRunId,
        status: AgentRunStatus,
    ) -> Result<(), AppError> {
        let now = Utc::now();

        if status == AgentRunStatus::Running {
            sqlx::query(
                "UPDATE agent_runs SET status = ?, started_at = COALESCE(started_at, ?), completed_at = NULL, updated_at = ? WHERE id = ?",
            )
            .bind(status.as_str())
            .bind(now)
            .bind(now)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal { message: format!("update_run_status failed: {e}") })?;
        } else if status.is_terminal() {
            sqlx::query(
                "UPDATE agent_runs SET status = ?, completed_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(status.as_str())
            .bind(now)
            .bind(now)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("update_run_status failed: {e}"),
            })?;
        } else {
            sqlx::query("UPDATE agent_runs SET status = ?, updated_at = ? WHERE id = ?")
                .bind(status.as_str())
                .bind(now)
                .bind(id.0)
                .execute(&self.pool)
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("update_run_status failed: {e}"),
                })?;
        }

        Ok(())
    }

    async fn resume_waiting_run(&self, id: AgentRunId) -> Result<bool, AppError> {
        let result = sqlx::query(
            "UPDATE agent_runs SET status = 'Running', updated_at = ?,
                    started_at = COALESCE(started_at, ?), completed_at = NULL
             WHERE id = ? AND status = 'WaitingApproval'",
        )
        .bind(Utc::now())
        .bind(Utc::now())
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("resume_waiting_run failed: {e}"),
        })?;
        Ok(result.rows_affected() == 1)
    }

    async fn append_event(
        &self,
        run_id: AgentRunId,
        thread_id: ThreadId,
        sequence: i64,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<RunEvent, AppError> {
        let payload_str = serde_json::to_string(&payload).map_err(|e| AppError::Internal {
            message: format!("serialize payload failed: {e}"),
        })?;
        let now = Utc::now();

        let id = sqlx::query(
            "INSERT INTO run_events (run_id, thread_id, sequence, event_type, payload, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(run_id.0)
        .bind(thread_id.0)
        .bind(sequence)
        .bind(event_type)
        .bind(payload_str)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("append_event failed: {e}") })?
        .last_insert_rowid();

        Ok(RunEvent {
            id,
            run_id,
            thread_id,
            sequence,
            event_type: event_type.to_owned(),
            payload,
            created_at: now,
        })
    }

    async fn list_run_events(&self, run_id: AgentRunId) -> Result<Vec<RunEvent>, AppError> {
        let rows = sqlx::query_as::<_, RunEventRow>(
            "SELECT id, run_id, thread_id, sequence, event_type, payload, created_at FROM run_events WHERE run_id = ? ORDER BY sequence ASC, id ASC",
        )
        .bind(run_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("list_run_events failed: {e}") })?;

        Ok(rows.into_iter().map(RunEventRow::into).collect())
    }

    async fn save_agent_checkpoint(
        &self,
        run_id: AgentRunId,
        messages: &serde_json::Value,
        step: i64,
    ) -> Result<(), AppError> {
        if step < 0 {
            return Err(AppError::Internal {
                message: "agent checkpoint step cannot be negative".to_owned(),
            });
        }
        let messages_json = serde_json::to_string(messages).map_err(|e| AppError::Internal {
            message: format!("serialize agent checkpoint failed: {e}"),
        })?;
        sqlx::query(
            "INSERT INTO agent_checkpoints (run_id, messages_json, step, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(run_id) DO UPDATE SET
                 messages_json = excluded.messages_json,
                 step = excluded.step,
                 updated_at = excluded.updated_at",
        )
        .bind(run_id.0)
        .bind(messages_json)
        .bind(step)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("save_agent_checkpoint failed: {e}"),
        })?;
        Ok(())
    }

    async fn load_agent_checkpoint(
        &self,
        run_id: AgentRunId,
    ) -> Result<Option<AgentCheckpoint>, AppError> {
        let row: Option<(String, i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT messages_json, step, updated_at FROM agent_checkpoints WHERE run_id = ?",
        )
        .bind(run_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("load_agent_checkpoint failed: {e}"),
        })?;
        row.map(|(messages_json, step, updated_at)| {
            let messages =
                serde_json::from_str(&messages_json).map_err(|e| AppError::Internal {
                    message: format!("deserialize agent checkpoint failed: {e}"),
                })?;
            Ok(AgentCheckpoint {
                run_id,
                messages,
                step,
                updated_at,
            })
        })
        .transpose()
    }

    async fn append_audit_log(
        &self,
        workspace_id: WorkspaceId,
        thread_id: Option<ThreadId>,
        run_id: Option<AgentRunId>,
        action: &str,
        resource: &str,
        decision: &str,
        reason: Option<&str>,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO audit_log (workspace_id, thread_id, run_id, action, resource, decision, reason, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(workspace_id.0)
        .bind(thread_id.map(|id| id.0))
        .bind(run_id.map(|id| id.0))
        .bind(action)
        .bind(resource)
        .bind(decision)
        .bind(reason)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("append_audit_log failed: {e}") })?;

        Ok(())
    }

    async fn list_audit_log(
        &self,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        run_id: Option<AgentRunId>,
    ) -> Result<Vec<AuditLogEntry>, AppError> {
        let mut sql = String::from(
            "SELECT id, workspace_id, thread_id, run_id, action, resource, decision, reason, created_at FROM audit_log WHERE 1=1",
        );

        if workspace_id.is_some() {
            sql.push_str(" AND workspace_id = ?");
        }
        if thread_id.is_some() {
            sql.push_str(" AND thread_id = ?");
        }
        if run_id.is_some() {
            sql.push_str(" AND run_id = ?");
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC");

        let mut query = sqlx::query_as::<_, AuditLogEntry>(&sql);
        if let Some(id) = workspace_id {
            query = query.bind(id.0);
        }
        if let Some(id) = thread_id {
            query = query.bind(id.0);
        }
        if let Some(id) = run_id {
            query = query.bind(id.0);
        }

        let rows = query.fetch_all(&self.pool).await.map_err(|e| AppError::Internal {
            message: format!("list_audit_log failed: {e}"),
        })?;

        Ok(rows)
    }

    async fn update_workspace_trusted(
        &self,
        id: WorkspaceId,
        trusted: bool,
    ) -> Result<Workspace, AppError> {
        let now = Utc::now();
        let trusted_i64 = i64::from(trusted);

        let result = sqlx::query("UPDATE workspaces SET trusted = ?, updated_at = ? WHERE id = ?")
            .bind(trusted_i64)
            .bind(now)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("update_workspace_trusted failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("workspace {id:?}"),
            });
        }

        self.get_workspace(id).await
    }

    async fn set_workspace_allowed_paths(
        &self,
        id: WorkspaceId,
        read_paths: Vec<String>,
        write_paths: Vec<String>,
    ) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await.map_err(|e| AppError::Internal {
            message: format!("set_workspace_allowed_paths transaction failed: {e}"),
        })?;

        sqlx::query("DELETE FROM workspace_paths WHERE workspace_id = ?")
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("set_workspace_allowed_paths delete failed: {e}"),
            })?;

        for path in read_paths {
            sqlx::query(
                "INSERT INTO workspace_paths (workspace_id, kind, path) VALUES (?, 'read', ?)",
            )
            .bind(id.0)
            .bind(path)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("set_workspace_allowed_paths insert read failed: {e}"),
            })?;
        }

        for path in write_paths {
            sqlx::query(
                "INSERT INTO workspace_paths (workspace_id, kind, path) VALUES (?, 'write', ?)",
            )
            .bind(id.0)
            .bind(path)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("set_workspace_allowed_paths insert write failed: {e}"),
            })?;
        }

        let touched = sqlx::query("UPDATE workspaces SET updated_at = ? WHERE id = ?")
            .bind(Utc::now())
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("set_workspace_allowed_paths revision update failed: {e}"),
            })?;
        if touched.rows_affected() != 1 {
            return Err(AppError::NotFound {
                resource: format!("workspace {id:?}"),
            });
        }

        tx.commit().await.map_err(|e| AppError::Internal {
            message: format!("set_workspace_allowed_paths commit failed: {e}"),
        })
    }

    async fn create_worktree(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
        name: &str,
        path: &str,
    ) -> Result<Worktree, AppError> {
        let id = WorktreeId::new();
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO worktrees (id, workspace_id, thread_id, name, path, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.0)
        .bind(workspace_id.0)
        .bind(thread_id.0)
        .bind(name)
        .bind(path)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("create_worktree failed: {e}") })?;

        Ok(Worktree {
            id,
            workspace_id,
            thread_id,
            name: name.to_owned(),
            path: path.to_owned(),
            created_at: now,
        })
    }

    async fn delete_worktree(&self, id: WorktreeId) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM worktrees WHERE id = ?")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("delete_worktree failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("worktree {id:?}"),
            });
        }
        Ok(())
    }

    async fn list_worktrees(&self, workspace_id: WorkspaceId) -> Result<Vec<Worktree>, AppError> {
        let rows = sqlx::query_as::<_, WorktreeRow>(
            "SELECT id, workspace_id, thread_id, name, path, created_at FROM worktrees WHERE workspace_id = ? ORDER BY created_at ASC",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("list_worktrees failed: {e}") })?;

        Ok(rows.into_iter().map(WorktreeRow::into).collect())
    }

    async fn get_worktree(&self, id: WorktreeId) -> Result<Worktree, AppError> {
        let row = sqlx::query_as::<_, WorktreeRow>(
            "SELECT id, workspace_id, thread_id, name, path, created_at FROM worktrees WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("get_worktree failed: {e}") })?;

        row.map(WorktreeRow::into).ok_or_else(|| AppError::NotFound {
            resource: format!("worktree {id:?}"),
        })
    }

    async fn create_subagent(&self, subagent: &SubagentRun) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO subagents (id, parent_run_id, agent_name, status, task_description, output_summary, created_at, completed_at, child_run_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(subagent.id.0)
        .bind(subagent.parent_run_id.0)
        .bind(&subagent.agent_name)
        .bind(subagent.status.as_str())
        .bind(&subagent.task_description)
        .bind(subagent.output_summary.as_ref())
        .bind(subagent.created_at)
        .bind(subagent.completed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("create_subagent failed: {e}") })?;
        Ok(())
    }

    async fn get_subagent(&self, id: AgentRunId) -> Result<SubagentRun, AppError> {
        let row = sqlx::query_as::<_, SubagentRunRow>(
            "SELECT id, parent_run_id, agent_name, status, task_description, output_summary, created_at, completed_at FROM subagents WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("get_subagent failed: {e}") })?;

        row.map(SubagentRunRow::into).ok_or_else(|| AppError::NotFound {
            resource: format!("subagent {id:?}"),
        })
    }

    async fn list_subagents(
        &self,
        parent_run_id: AgentRunId,
    ) -> Result<Vec<SubagentRun>, AppError> {
        let rows = sqlx::query_as::<_, SubagentRunRow>(
            "SELECT id, parent_run_id, agent_name, status, task_description, output_summary, created_at, completed_at FROM subagents WHERE parent_run_id = ? ORDER BY created_at ASC",
        )
        .bind(parent_run_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("list_subagents failed: {e}") })?;

        Ok(rows.into_iter().map(SubagentRunRow::into).collect())
    }

    async fn update_subagent_status(
        &self,
        id: AgentRunId,
        status: AgentRunStatus,
        output_summary: Option<&str>,
    ) -> Result<(), AppError> {
        let now = Utc::now();

        if status.is_terminal() {
            sqlx::query(
                "UPDATE subagents SET status = ?, output_summary = COALESCE(?, output_summary), completed_at = COALESCE(completed_at, ?) WHERE id = ?",
            )
            .bind(status.as_str())
            .bind(output_summary)
            .bind(now)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal { message: format!("update_subagent_status failed: {e}") })?;
        } else {
            sqlx::query(
                "UPDATE subagents SET status = ?, output_summary = COALESCE(?, output_summary), completed_at = NULL WHERE id = ?",
            )
            .bind(status.as_str())
            .bind(output_summary)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal { message: format!("update_subagent_status failed: {e}") })?;
        }
        Ok(())
    }

    async fn update_subagent_child_run(
        &self,
        id: AgentRunId,
        child_run_id: AgentRunId,
    ) -> Result<(), AppError> {
        let result = sqlx::query("UPDATE subagents SET child_run_id = ? WHERE id = ?")
            .bind(child_run_id.0)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("update_subagent_child_run failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("subagent {id:?}"),
            });
        }
        Ok(())
    }

    async fn get_subagent_child_run(&self, id: AgentRunId) -> Result<Option<AgentRunId>, AppError> {
        let row: Option<(Option<uuid::Uuid>,)> =
            sqlx::query_as("SELECT child_run_id FROM subagents WHERE id = ?")
                .bind(id.0)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("get_subagent_child_run failed: {e}"),
                })?;

        Ok(row.and_then(|r| r.0.map(AgentRunId)))
    }

    async fn create_background_task(&self, task: &BackgroundTask) -> Result<(), AppError> {
        let payload = serde_json::to_string(&task.payload).map_err(|e| AppError::Internal {
            message: format!("serialize background task payload failed: {e}"),
        })?;

        sqlx::query(
            "INSERT INTO background_tasks (
                id, workspace_id, thread_id, run_id, task_kind, payload, status,
                priority, attempts, max_attempts, scheduled_at, leased_at, leased_by,
                next_retry_at, error_message, result_summary, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(task.id.0)
        .bind(task.workspace_id.0)
        .bind(task.thread_id.map(|id| id.0))
        .bind(task.run_id.map(|id| id.0))
        .bind(task.task_kind.as_str())
        .bind(payload)
        .bind(task.status.as_str())
        .bind(task.priority)
        .bind(i64::from(task.attempts))
        .bind(i64::from(task.max_attempts))
        .bind(task.scheduled_at)
        .bind(task.leased_at)
        .bind(task.leased_by.as_ref())
        .bind(task.next_retry_at)
        .bind(task.error_message.as_ref())
        .bind(task.result_summary.as_ref())
        .bind(task.created_at)
        .bind(task.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create_background_task failed: {e}"),
        })?;

        Ok(())
    }

    async fn get_background_task(&self, id: BackgroundTaskId) -> Result<BackgroundTask, AppError> {
        let row = sqlx::query_as::<_, BackgroundTaskRow>(
            "SELECT id, workspace_id, thread_id, run_id, task_kind, payload, status,
                    priority, attempts, max_attempts, scheduled_at, leased_at, leased_by,
                    next_retry_at, error_message, result_summary, created_at, updated_at
             FROM background_tasks WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_background_task failed: {e}"),
        })?;

        row.map(BackgroundTaskRow::into).ok_or_else(|| AppError::NotFound {
            resource: format!("background task {id:?}"),
        })
    }

    async fn list_background_tasks(
        &self,
        workspace_id: Option<WorkspaceId>,
        status: Option<BackgroundTaskStatus>,
        limit: i64,
    ) -> Result<Vec<BackgroundTask>, AppError> {
        let mut sql = String::from(
            "SELECT id, workspace_id, thread_id, run_id, task_kind, payload, status,
                    priority, attempts, max_attempts, scheduled_at, leased_at, leased_by,
                    next_retry_at, error_message, result_summary, created_at, updated_at
             FROM background_tasks WHERE 1=1",
        );
        if workspace_id.is_some() {
            sql.push_str(" AND workspace_id = ?");
        }
        if status.is_some() {
            sql.push_str(" AND status = ?");
        }
        sql.push_str(" ORDER BY priority DESC, scheduled_at ASC, created_at ASC LIMIT ?");

        let mut query = sqlx::query_as::<_, BackgroundTaskRow>(&sql);
        if let Some(id) = workspace_id {
            query = query.bind(id.0);
        }
        if let Some(s) = status {
            query = query.bind(s.as_str());
        }
        query = query.bind(limit);

        let rows = query.fetch_all(&self.pool).await.map_err(|e| AppError::Internal {
            message: format!("list_background_tasks failed: {e}"),
        })?;

        Ok(rows.into_iter().map(BackgroundTaskRow::into).collect())
    }

    async fn lease_next_background_task(
        &self,
        worker_id: &str,
        _lease_duration: chrono::Duration,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<BackgroundTask>, AppError> {
        let row = sqlx::query_as::<_, BackgroundTaskRow>(
            "UPDATE background_tasks
             SET status = 'Running', leased_at = ?, leased_by = ?, attempts = attempts + 1, updated_at = ?
             WHERE id = (
                 SELECT id FROM background_tasks
                 WHERE status = 'Queued'
                   AND scheduled_at <= ?
                   AND (next_retry_at IS NULL OR next_retry_at <= ?)
                 ORDER BY priority DESC, scheduled_at ASC, created_at ASC
                 LIMIT 1
             )
             RETURNING id, workspace_id, thread_id, run_id, task_kind, payload, status,
                       priority, attempts, max_attempts, scheduled_at, leased_at, leased_by,
                       next_retry_at, error_message, result_summary, created_at, updated_at",
        )
        .bind(now)
        .bind(worker_id)
        .bind(now)
        .bind(now)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("lease_next_background_task failed: {e}"),
        })?;

        Ok(row.map(BackgroundTaskRow::into))
    }

    async fn complete_background_task(
        &self,
        id: BackgroundTaskId,
        result_summary: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError> {
        let result = sqlx::query(
            "UPDATE background_tasks SET status = 'Completed', result_summary = ?, updated_at = ? WHERE id = ?",
        )
        .bind(result_summary)
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("complete_background_task failed: {e}"),
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("background task {id:?}"),
            });
        }
        Ok(())
    }

    async fn fail_background_task(
        &self,
        id: BackgroundTaskId,
        error_message: &str,
        next_retry_at: Option<chrono::DateTime<chrono::Utc>>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError> {
        let retrying = next_retry_at.is_some();
        let status = if retrying { "Queued" } else { "Failed" };
        let attempts_sql = if retrying {
            "attempts = attempts + 1, "
        } else {
            ""
        };

        let result = sqlx::query(
            &format!(
                "UPDATE background_tasks SET status = ?, error_message = ?, next_retry_at = ?, {attempts_sql}updated_at = ? WHERE id = ?"
            ),
        )
        .bind(status)
        .bind(error_message)
        .bind(next_retry_at)
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("fail_background_task failed: {e}"),
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("background task {id:?}"),
            });
        }
        Ok(())
    }

    async fn cancel_background_task(
        &self,
        id: BackgroundTaskId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError> {
        let result = sqlx::query(
            "UPDATE background_tasks SET status = 'Cancelled', updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("cancel_background_task failed: {e}"),
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("background task {id:?}"),
            });
        }
        Ok(())
    }

    async fn recover_stalled_background_tasks(
        &self,
        lease_timeout: chrono::DateTime<chrono::Utc>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, AppError> {
        let result = sqlx::query(
            "UPDATE background_tasks
             SET status = 'Queued', leased_at = NULL, leased_by = NULL, attempts = attempts + 1, updated_at = ?
             WHERE status = 'Running' AND leased_at <= ?",
        )
        .bind(now)
        .bind(lease_timeout)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("recover_stalled_background_tasks failed: {e}"),
        })?;

        Ok(result.rows_affected())
    }

    async fn create_automation(&self, automation: &Automation) -> Result<(), AppError> {
        let permission_policy =
            serde_json::to_string(&automation.permission_policy).map_err(|e| {
                AppError::Internal {
                    message: format!("serialize automation permission policy failed: {e}"),
                }
            })?;
        let enabled_i64 = i64::from(automation.enabled);

        sqlx::query(
            "INSERT INTO automations (
                id, workspace_id, name, description, trigger, cron_expr, enabled,
                permission_policy, next_run_at, last_run_at, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(automation.id.0)
        .bind(automation.workspace_id.0)
        .bind(&automation.name)
        .bind(&automation.description)
        .bind(automation.trigger.as_str())
        .bind(automation.cron_expr.as_ref())
        .bind(enabled_i64)
        .bind(permission_policy)
        .bind(automation.next_run_at)
        .bind(automation.last_run_at)
        .bind(automation.created_at)
        .bind(automation.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create_automation failed: {e}"),
        })?;

        Ok(())
    }

    async fn get_automation(&self, id: AutomationId) -> Result<Automation, AppError> {
        let row = sqlx::query_as::<_, AutomationRow>(
            "SELECT id, workspace_id, name, description, trigger, cron_expr, enabled,
                    permission_policy, next_run_at, last_run_at, created_at, updated_at
             FROM automations WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_automation failed: {e}"),
        })?;

        row.map(AutomationRow::into).ok_or_else(|| AppError::NotFound {
            resource: format!("automation {id:?}"),
        })
    }

    async fn list_automations(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<Automation>, AppError> {
        let rows = sqlx::query_as::<_, AutomationRow>(
            "SELECT id, workspace_id, name, description, trigger, cron_expr, enabled,
                    permission_policy, next_run_at, last_run_at, created_at, updated_at
             FROM automations WHERE workspace_id = ? ORDER BY created_at ASC",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list_automations failed: {e}"),
        })?;

        Ok(rows.into_iter().map(AutomationRow::into).collect())
    }

    async fn list_due_automations(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Automation>, AppError> {
        let rows = sqlx::query_as::<_, AutomationRow>(
            "SELECT id, workspace_id, name, description, trigger, cron_expr, enabled,
                    permission_policy, next_run_at, last_run_at, created_at, updated_at
             FROM automations WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?
             ORDER BY next_run_at ASC",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list_due_automations failed: {e}"),
        })?;

        Ok(rows.into_iter().map(AutomationRow::into).collect())
    }

    async fn update_automation(&self, automation: &Automation) -> Result<(), AppError> {
        let permission_policy =
            serde_json::to_string(&automation.permission_policy).map_err(|e| {
                AppError::Internal {
                    message: format!("serialize automation permission policy failed: {e}"),
                }
            })?;
        let enabled_i64 = i64::from(automation.enabled);

        let result = sqlx::query(
            "UPDATE automations SET
                workspace_id = ?, name = ?, description = ?, trigger = ?, cron_expr = ?,
                enabled = ?, permission_policy = ?, next_run_at = ?, last_run_at = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(automation.workspace_id.0)
        .bind(&automation.name)
        .bind(&automation.description)
        .bind(automation.trigger.as_str())
        .bind(automation.cron_expr.as_ref())
        .bind(enabled_i64)
        .bind(permission_policy)
        .bind(automation.next_run_at)
        .bind(automation.last_run_at)
        .bind(automation.updated_at)
        .bind(automation.id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("update_automation failed: {e}"),
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("automation {:?}", automation.id),
            });
        }
        Ok(())
    }

    async fn delete_automation(&self, id: AutomationId) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM automations WHERE id = ?")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("delete_automation failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("automation {id:?}"),
            });
        }
        Ok(())
    }

    async fn create_notification(&self, notification: &Notification) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO notifications (
                id, workspace_id, thread_id, run_id, title, body, category, read, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(notification.id.0)
        .bind(notification.workspace_id.0)
        .bind(notification.thread_id.map(|id| id.0))
        .bind(notification.run_id.map(|id| id.0))
        .bind(&notification.title)
        .bind(&notification.body)
        .bind(notification.category.as_str())
        .bind(i64::from(notification.read))
        .bind(notification.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create_notification failed: {e}"),
        })?;

        Ok(())
    }

    async fn list_notifications(
        &self,
        workspace_id: Option<WorkspaceId>,
        unread_only: bool,
        limit: i64,
    ) -> Result<Vec<Notification>, AppError> {
        let mut sql = String::from(
            "SELECT id, workspace_id, thread_id, run_id, title, body, category, read, created_at
             FROM notifications WHERE 1=1",
        );
        if workspace_id.is_some() {
            sql.push_str(" AND workspace_id = ?");
        }
        if unread_only {
            sql.push_str(" AND read = 0");
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");

        let mut query = sqlx::query_as::<_, NotificationRow>(&sql);
        if let Some(id) = workspace_id {
            query = query.bind(id.0);
        }
        query = query.bind(limit);

        let rows = query.fetch_all(&self.pool).await.map_err(|e| AppError::Internal {
            message: format!("list_notifications failed: {e}"),
        })?;

        Ok(rows.into_iter().map(NotificationRow::into).collect())
    }

    async fn get_notification(&self, id: NotificationId) -> Result<Notification, AppError> {
        let row = sqlx::query_as::<_, NotificationRow>(
            "SELECT id, workspace_id, thread_id, run_id, title, body, category, read, created_at
             FROM notifications WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_notification failed: {e}"),
        })?;

        row.map(NotificationRow::into).ok_or_else(|| AppError::NotFound {
            resource: format!("notification {id:?}"),
        })
    }

    async fn mark_notification_read(&self, id: NotificationId, read: bool) -> Result<(), AppError> {
        let result = sqlx::query("UPDATE notifications SET read = ? WHERE id = ?")
            .bind(i64::from(read))
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("mark_notification_read failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("notification {id:?}"),
            });
        }
        Ok(())
    }

    async fn dismiss_notification(&self, id: NotificationId) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM notifications WHERE id = ?")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("dismiss_notification failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("notification {id:?}"),
            });
        }
        Ok(())
    }

    async fn create_approval_request(
        &self,
        request: &ApprovalRequest,
    ) -> Result<ApprovalRequest, AppError> {
        let now = Utc::now();
        let status = request.status.as_str();

        let id = sqlx::query(
            "INSERT INTO approval_requests (
                run_id, workspace_id, thread_id, action, resource, status,
                created_at, resolved_at, resolution_reason
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(request.run_id.0)
        .bind(request.workspace_id.0)
        .bind(request.thread_id.0)
        .bind(&request.action)
        .bind(&request.resource)
        .bind(status)
        .bind(now)
        .bind(request.resolved_at)
        .bind(request.resolution_reason.as_ref())
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create_approval_request failed: {e}"),
        })?
        .last_insert_rowid();

        Ok(ApprovalRequest {
            id: ApprovalRequestId(id),
            run_id: request.run_id,
            workspace_id: request.workspace_id,
            thread_id: request.thread_id,
            action: request.action.clone(),
            resource: request.resource.clone(),
            status: request.status,
            created_at: now,
            resolved_at: request.resolved_at,
            resolution_reason: request.resolution_reason.clone(),
        })
    }

    async fn get_approval_request(
        &self,
        id: ApprovalRequestId,
    ) -> Result<ApprovalRequest, AppError> {
        let row = sqlx::query_as::<_, ApprovalRequestRow>(
            "SELECT id, run_id, workspace_id, thread_id, action, resource, status,
                    created_at, resolved_at, resolution_reason
             FROM approval_requests WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_approval_request failed: {e}"),
        })?;

        row.map(ApprovalRequestRow::into).ok_or_else(|| AppError::NotFound {
            resource: format!("approval request {id:?}"),
        })
    }

    async fn list_pending_approval_requests(
        &self,
        run_id: Option<AgentRunId>,
    ) -> Result<Vec<ApprovalRequest>, AppError> {
        let mut sql = String::from(
            "SELECT id, run_id, workspace_id, thread_id, action, resource, status,
                    created_at, resolved_at, resolution_reason
             FROM approval_requests WHERE status = 'Pending'",
        );
        if run_id.is_some() {
            sql.push_str(" AND run_id = ?");
        }
        sql.push_str(" ORDER BY created_at ASC");

        let mut query = sqlx::query_as::<_, ApprovalRequestRow>(&sql);
        if let Some(id) = run_id {
            query = query.bind(id.0);
        }

        let rows = query.fetch_all(&self.pool).await.map_err(|e| AppError::Internal {
            message: format!("list_pending_approval_requests failed: {e}"),
        })?;

        Ok(rows.into_iter().map(ApprovalRequestRow::into).collect())
    }

    async fn resolve_approval_request(
        &self,
        id: ApprovalRequestId,
        status: ApprovalRequestStatus,
        reason: Option<&str>,
    ) -> Result<ApprovalRequest, AppError> {
        let now = Utc::now();
        let status_str = status.as_str();

        let result = sqlx::query(
            "UPDATE approval_requests
             SET status = ?, resolved_at = ?, resolution_reason = ?
             WHERE id = ? AND status = 'Pending'",
        )
        .bind(status_str)
        .bind(now)
        .bind(reason)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("resolve_approval_request failed: {e}"),
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::Internal {
                message: format!("approval request {id:?} is missing or no longer pending"),
            });
        }

        self.get_approval_request(id).await
    }

    async fn create_tool_invocation(
        &self,
        invocation: &ToolInvocation,
    ) -> Result<ToolInvocation, AppError> {
        if invocation.status == ToolInvocationStatus::WaitingApproval
            || invocation.approval_request_id.is_some()
        {
            return Err(AppError::Internal {
                message: "waiting tool invocations must be created atomically with approval"
                    .to_owned(),
            });
        }
        validate_tool_invocation_owner(&self.pool, invocation).await?;
        insert_tool_invocation(&self.pool, invocation).await?;
        self.get_tool_invocation(invocation.id).await
    }

    async fn create_tool_invocation_with_approval(
        &self,
        invocation: &ToolInvocation,
        approval: &ApprovalRequest,
    ) -> Result<(ToolInvocation, ApprovalRequest), AppError> {
        if invocation.status != ToolInvocationStatus::WaitingApproval
            || invocation.approval_request_id.is_some()
            || approval.status != ApprovalRequestStatus::Pending
            || invocation.run_id != approval.run_id
            || invocation.workspace_id != approval.workspace_id
            || invocation.thread_id != approval.thread_id
            || invocation.action != approval.action
            || invocation.resource != approval.resource
        {
            return Err(AppError::PermissionDenied {
                reason: "tool invocation and approval ownership or immutable request differ"
                    .to_owned(),
            });
        }
        validate_tool_invocation_owner(&self.pool, invocation).await?;

        let mut tx = self.pool.begin().await.map_err(|e| AppError::Internal {
            message: format!("create tool approval transaction failed: {e}"),
        })?;
        insert_tool_invocation_in_transaction(&mut tx, invocation).await?;

        let now = Utc::now();
        let approval_id = sqlx::query(
            "INSERT INTO approval_requests (
                run_id, workspace_id, thread_id, action, resource, status,
                created_at, resolved_at, resolution_reason
             ) VALUES (?, ?, ?, ?, ?, 'Pending', ?, NULL, NULL)",
        )
        .bind(approval.run_id.0)
        .bind(approval.workspace_id.0)
        .bind(approval.thread_id.0)
        .bind(&approval.action)
        .bind(&approval.resource)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create linked approval failed: {e}"),
        })?
        .last_insert_rowid();

        sqlx::query("UPDATE tool_invocations SET approval_request_id = ? WHERE id = ?")
            .bind(approval_id)
            .bind(invocation.id.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("link tool invocation approval failed: {e}"),
            })?;
        let run_update = sqlx::query(
            "UPDATE agent_runs SET status = 'WaitingApproval', updated_at = ?
             WHERE id = ? AND status NOT IN ('Cancelled', 'Failed', 'Interrupted', 'Completed')",
        )
        .bind(now)
        .bind(invocation.run_id.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("mark run waiting for tool approval failed: {e}"),
        })?;
        if run_update.rows_affected() != 1 {
            return Err(AppError::PermissionDenied {
                reason: "terminal run cannot wait for tool approval".to_owned(),
            });
        }
        tx.commit().await.map_err(|e| AppError::Internal {
            message: format!("commit tool approval transaction failed: {e}"),
        })?;

        Ok((
            self.get_tool_invocation(invocation.id).await?,
            self.get_approval_request(ApprovalRequestId(approval_id)).await?,
        ))
    }

    async fn get_tool_invocation(&self, id: ToolInvocationId) -> Result<ToolInvocation, AppError> {
        let row = sqlx::query_as::<_, ToolInvocationRow>(
            "SELECT id, run_id, thread_id, workspace_id, model_call_id, tool_name,
                    tool_version, action, resource, arguments_json, request_hash,
                    policy_version, context_revision, status, approval_request_id, result_json, error,
                    recovery_json, lease_token, attempts, cancel_requested, correlation_id, created_at,
                    updated_at, started_at, completed_at
             FROM tool_invocations WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_tool_invocation failed: {e}"),
        })?;
        row.map(ToolInvocation::try_from)
            .transpose()?
            .ok_or_else(|| AppError::NotFound {
                resource: format!("tool invocation {id:?}"),
            })
    }

    async fn get_tool_invocation_for_approval(
        &self,
        approval_id: ApprovalRequestId,
    ) -> Result<ToolInvocation, AppError> {
        get_tool_invocation_by_approval(&self.pool, approval_id).await
    }

    async fn list_tool_invocations(
        &self,
        run_id: AgentRunId,
    ) -> Result<Vec<ToolInvocation>, AppError> {
        let rows = sqlx::query_as::<_, ToolInvocationRow>(
            "SELECT id, run_id, thread_id, workspace_id, model_call_id, tool_name,
                    tool_version, action, resource, arguments_json, request_hash,
                    policy_version, context_revision, status, approval_request_id, result_json, error,
                    recovery_json, lease_token, attempts, cancel_requested, correlation_id, created_at,
                    updated_at, started_at, completed_at
             FROM tool_invocations WHERE run_id = ? ORDER BY created_at, id",
        )
        .bind(run_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list_tool_invocations failed: {e}"),
        })?;
        rows.into_iter().map(ToolInvocation::try_from).collect()
    }

    async fn list_tool_invocations_requiring_reconciliation(
        &self,
    ) -> Result<Vec<ToolInvocation>, AppError> {
        let rows = sqlx::query_as::<_, ToolInvocationRow>(
            "SELECT id, run_id, thread_id, workspace_id, model_call_id, tool_name,
                    tool_version, action, resource, arguments_json, request_hash,
                    policy_version, context_revision, status, approval_request_id, result_json, error,
                    recovery_json, lease_token, attempts, cancel_requested, correlation_id, created_at,
                    updated_at, started_at, completed_at
             FROM tool_invocations WHERE status = 'NeedsReconciliation'
             ORDER BY updated_at, id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list reconciliation tool invocations failed: {e}"),
        })?;
        rows.into_iter().map(ToolInvocation::try_from).collect()
    }

    async fn resolve_tool_approval(
        &self,
        id: ApprovalRequestId,
        status: ApprovalRequestStatus,
        reason: Option<&str>,
    ) -> Result<(ApprovalRequest, ToolInvocation), AppError> {
        let target_invocation_status = match status {
            ApprovalRequestStatus::Approved => ToolInvocationStatus::Approved,
            ApprovalRequestStatus::Denied => ToolInvocationStatus::Denied,
            ApprovalRequestStatus::Pending => {
                return Err(AppError::Internal {
                    message: "cannot resolve an approval back to Pending".to_owned(),
                });
            }
        };

        let mut tx = self.pool.begin().await.map_err(|e| AppError::Internal {
            message: format!("resolve tool approval transaction failed: {e}"),
        })?;
        let now = Utc::now();
        let approval_update = sqlx::query(
            "UPDATE approval_requests
             SET status = ?, resolved_at = ?, resolution_reason = ?
             WHERE id = ? AND status = 'Pending'",
        )
        .bind(status.as_str())
        .bind(now)
        .bind(reason)
        .bind(id.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("resolve linked approval failed: {e}"),
        })?;
        if approval_update.rows_affected() == 0 {
            tx.rollback().await.map_err(|e| AppError::Internal {
                message: format!("rollback idempotent tool approval failed: {e}"),
            })?;
            let approval = self.get_approval_request(id).await?;
            if approval.status != status {
                return Err(AppError::PermissionDenied {
                    reason: "approval already has a conflicting final decision".to_owned(),
                });
            }
            let invocation = get_tool_invocation_by_approval(&self.pool, id).await?;
            return Ok((approval, invocation));
        }
        let invocation_update = sqlx::query(
            "UPDATE tool_invocations SET status = ?, updated_at = ?,
                    completed_at = CASE WHEN ? = 'Denied' THEN ? ELSE completed_at END
             WHERE approval_request_id = ? AND status = 'WaitingApproval'
               AND context_revision = (
                   SELECT updated_at FROM workspaces WHERE id = tool_invocations.workspace_id
               )",
        )
        .bind(target_invocation_status.as_str())
        .bind(now)
        .bind(target_invocation_status.as_str())
        .bind(now)
        .bind(id.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("resolve linked tool invocation failed: {e}"),
        })?;
        if invocation_update.rows_affected() != 1 {
            return Err(AppError::Internal {
                message: "approval and tool invocation could not transition atomically".to_owned(),
            });
        }
        if status == ApprovalRequestStatus::Denied {
            sqlx::query(
                "UPDATE agent_runs SET status = 'Failed', updated_at = ?, completed_at = ?
                 WHERE id = (
                     SELECT run_id FROM tool_invocations WHERE approval_request_id = ?
                 ) AND status = 'WaitingApproval'",
            )
            .bind(now)
            .bind(now)
            .bind(id.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("fail run for denied tool approval failed: {e}"),
            })?;
        }
        tx.commit().await.map_err(|e| AppError::Internal {
            message: format!("commit tool approval resolution failed: {e}"),
        })?;
        Ok((
            self.get_approval_request(id).await?,
            get_tool_invocation_by_approval(&self.pool, id).await?,
        ))
    }

    async fn claim_tool_invocation(
        &self,
        id: ToolInvocationId,
    ) -> Result<ToolInvocation, AppError> {
        let lease_token = uuid::Uuid::new_v4();
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE tool_invocations
             SET status = 'Executing', lease_token = ?, attempts = attempts + 1,
                 started_at = COALESCE(started_at, ?), updated_at = ?
             WHERE id = ? AND status IN ('Ready', 'Approved')
               AND context_revision = (
                   SELECT updated_at FROM workspaces WHERE id = tool_invocations.workspace_id
               )",
        )
        .bind(lease_token)
        .bind(now)
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("claim_tool_invocation failed: {e}"),
        })?;
        if result.rows_affected() != 1 {
            return Err(AppError::PermissionDenied {
                reason: "tool invocation is not claimable".to_owned(),
            });
        }
        self.get_tool_invocation(id).await
    }

    async fn complete_tool_invocation(
        &self,
        id: ToolInvocationId,
        lease_token: uuid::Uuid,
        result: serde_json::Value,
    ) -> Result<ToolInvocation, AppError> {
        let result_json = serde_json::to_string(&result).map_err(|e| AppError::Internal {
            message: format!("serialize tool result failed: {e}"),
        })?;
        let now = Utc::now();
        let update = sqlx::query(
            "UPDATE tool_invocations
             SET status = 'Succeeded', result_json = ?, error = NULL,
                 lease_token = NULL, updated_at = ?, completed_at = ?
             WHERE id = ? AND status = 'Executing' AND lease_token = ?",
        )
        .bind(result_json)
        .bind(now)
        .bind(now)
        .bind(id.0)
        .bind(lease_token)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("complete_tool_invocation failed: {e}"),
        })?;
        if update.rows_affected() != 1 {
            return Err(AppError::PermissionDenied {
                reason: "tool invocation lease is stale or invocation is no longer executing"
                    .to_owned(),
            });
        }
        self.get_tool_invocation(id).await
    }

    async fn fail_tool_invocation(
        &self,
        id: ToolInvocationId,
        lease_token: uuid::Uuid,
        error: &str,
    ) -> Result<ToolInvocation, AppError> {
        let now = Utc::now();
        let safe_error: String = error.chars().take(512).collect();
        let update = sqlx::query(
            "UPDATE tool_invocations
             SET status = 'Failed', error = ?, lease_token = NULL,
                 updated_at = ?, completed_at = ?
             WHERE id = ? AND status = 'Executing' AND lease_token = ?",
        )
        .bind(safe_error)
        .bind(now)
        .bind(now)
        .bind(id.0)
        .bind(lease_token)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("fail_tool_invocation failed: {e}"),
        })?;
        if update.rows_affected() != 1 {
            return Err(AppError::PermissionDenied {
                reason: "tool invocation lease is stale or invocation is no longer executing"
                    .to_owned(),
            });
        }
        self.get_tool_invocation(id).await
    }

    async fn complete_reconciled_tool_invocation(
        &self,
        id: ToolInvocationId,
        result: serde_json::Value,
    ) -> Result<ToolInvocation, AppError> {
        let result_json = serde_json::to_string(&result).map_err(|e| AppError::Internal {
            message: format!("serialize reconciled tool result failed: {e}"),
        })?;
        let now = Utc::now();
        let update = sqlx::query(
            "UPDATE tool_invocations
             SET status = 'Succeeded', result_json = ?, error = NULL,
                 updated_at = ?, completed_at = ?
             WHERE id = ? AND status = 'NeedsReconciliation'",
        )
        .bind(result_json)
        .bind(now)
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("complete_reconciled_tool_invocation failed: {e}"),
        })?;
        if update.rows_affected() != 1 {
            return Err(AppError::PermissionDenied {
                reason: "tool invocation no longer requires reconciliation".to_owned(),
            });
        }
        self.get_tool_invocation(id).await
    }

    async fn requeue_reconciled_tool_invocation(
        &self,
        id: ToolInvocationId,
    ) -> Result<ToolInvocation, AppError> {
        let now = Utc::now();
        let update = sqlx::query(
            "UPDATE tool_invocations
             SET status = CASE WHEN approval_request_id IS NULL THEN 'Ready' ELSE 'Approved' END,
                 error = NULL, updated_at = ?
             WHERE id = ? AND status = 'NeedsReconciliation'
               AND context_revision = (
                   SELECT updated_at FROM workspaces WHERE id = tool_invocations.workspace_id
               )",
        )
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("requeue_reconciled_tool_invocation failed: {e}"),
        })?;
        if update.rows_affected() != 1 {
            return Err(AppError::PermissionDenied {
                reason: "tool invocation cannot be safely requeued".to_owned(),
            });
        }
        self.get_tool_invocation(id).await
    }

    async fn fail_reconciled_tool_invocation(
        &self,
        id: ToolInvocationId,
        error: &str,
    ) -> Result<ToolInvocation, AppError> {
        let now = Utc::now();
        let safe_error: String = error.chars().take(512).collect();
        let update = sqlx::query(
            "UPDATE tool_invocations
             SET status = 'Failed', error = ?, updated_at = ?, completed_at = ?
             WHERE id = ? AND status = 'NeedsReconciliation'",
        )
        .bind(safe_error)
        .bind(now)
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("fail_reconciled_tool_invocation failed: {e}"),
        })?;
        if update.rows_affected() != 1 {
            return Err(AppError::PermissionDenied {
                reason: "tool invocation no longer requires reconciliation".to_owned(),
            });
        }
        self.get_tool_invocation(id).await
    }

    async fn cancel_tool_invocations_for_run(&self, run_id: AgentRunId) -> Result<u64, AppError> {
        let mut tx = self.pool.begin().await.map_err(|e| AppError::Internal {
            message: format!("cancel tool invocations transaction failed: {e}"),
        })?;
        let now = Utc::now();
        sqlx::query(
            "UPDATE approval_requests
             SET status = 'Denied', resolved_at = ?, resolution_reason = 'run cancelled'
             WHERE status = 'Pending' AND id IN (
                 SELECT approval_request_id FROM tool_invocations
                 WHERE run_id = ? AND approval_request_id IS NOT NULL
             )",
        )
        .bind(now)
        .bind(run_id.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("cancel linked approvals failed: {e}"),
        })?;
        let update = sqlx::query(
            "UPDATE tool_invocations
             SET status = CASE WHEN status = 'Executing' THEN status ELSE 'Cancelled' END,
                 cancel_requested = CASE WHEN status = 'Executing' THEN 1 ELSE cancel_requested END,
                 updated_at = ?,
                 completed_at = CASE WHEN status = 'Executing' THEN completed_at ELSE ? END
             WHERE run_id = ? AND status IN (
                 'Ready', 'WaitingApproval', 'Approved', 'Executing', 'NeedsReconciliation'
             )",
        )
        .bind(now)
        .bind(now)
        .bind(run_id.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("cancel tool invocations failed: {e}"),
        })?;
        tx.commit().await.map_err(|e| AppError::Internal {
            message: format!("commit tool invocation cancellation failed: {e}"),
        })?;
        Ok(update.rows_affected())
    }

    async fn recover_tool_invocations(&self) -> Result<u64, AppError> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE tool_invocations
             SET status = 'NeedsReconciliation', lease_token = NULL, updated_at = ?
             WHERE status = 'Executing'",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("recover_tool_invocations failed: {e}"),
        })?;
        Ok(result.rows_affected())
    }

    async fn upsert_orchestration(&self, session: &Orchestration) -> Result<(), AppError> {
        let plan_json = serde_json::to_string(&session.plan).map_err(|e| AppError::Internal {
            message: format!("serialize orchestration plan failed: {e}"),
        })?;
        let pattern_ids_json =
            serde_json::to_string(&session.pattern_ids).map_err(|e| AppError::Internal {
                message: format!("serialize orchestration pattern ids failed: {e}"),
            })?;
        sqlx::query(
            "INSERT INTO orchestrations (
                id, parent_run_id, workspace_id, thread_id, task, status,
                plan_json, pattern_ids_json, result_summary, created_at, updated_at, completed_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                parent_run_id = excluded.parent_run_id,
                workspace_id = excluded.workspace_id,
                thread_id = excluded.thread_id,
                task = excluded.task,
                status = excluded.status,
                plan_json = excluded.plan_json,
                pattern_ids_json = excluded.pattern_ids_json,
                result_summary = excluded.result_summary,
                updated_at = excluded.updated_at,
                completed_at = excluded.completed_at",
        )
        .bind(session.id.0)
        .bind(session.parent_run_id.0)
        .bind(session.workspace_id.0)
        .bind(session.thread_id.0)
        .bind(&session.task)
        .bind(session.status.as_str())
        .bind(plan_json)
        .bind(pattern_ids_json)
        .bind(&session.result_summary)
        .bind(session.created_at)
        .bind(session.updated_at)
        .bind(session.completed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("upsert_orchestration failed: {e}"),
        })?;
        Ok(())
    }

    async fn get_orchestration(&self, id: OrchestrationId) -> Result<Orchestration, AppError> {
        let row = sqlx::query_as::<_, OrchestrationRow>(
            "SELECT id, parent_run_id, workspace_id, thread_id, task, status,
                    plan_json, pattern_ids_json, result_summary, created_at, updated_at, completed_at
             FROM orchestrations WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_orchestration failed: {e}"),
        })?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("orchestration:{}", id.0),
        })?;
        orchestration_from_row(row)
    }

    async fn list_orchestrations_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> Result<Vec<Orchestration>, AppError> {
        let rows = sqlx::query_as::<_, OrchestrationRow>(
            "SELECT id, parent_run_id, workspace_id, thread_id, task, status,
                    plan_json, pattern_ids_json, result_summary, created_at, updated_at, completed_at
             FROM orchestrations
             WHERE thread_id = ?
             ORDER BY created_at DESC",
        )
        .bind(thread_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list_orchestrations_for_thread failed: {e}"),
        })?;
        rows.into_iter().map(orchestration_from_row).collect()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct OrchestrationRow {
    id: uuid::Uuid,
    parent_run_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    thread_id: uuid::Uuid,
    task: String,
    status: String,
    plan_json: String,
    pattern_ids_json: String,
    result_summary: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn orchestration_from_row(row: OrchestrationRow) -> Result<Orchestration, AppError> {
    let plan: OrchestrationPlan =
        serde_json::from_str(&row.plan_json).map_err(|e| AppError::Internal {
            message: format!("deserialize orchestration plan failed: {e}"),
        })?;
    let pattern_ids: Vec<WorkflowPatternId> = serde_json::from_str(&row.pattern_ids_json)
        .map_err(|e| AppError::Internal {
            message: format!("deserialize orchestration pattern ids failed: {e}"),
        })?;
    let status = OrchestrationStatus::try_from(row.status.as_str())?;
    Ok(Orchestration {
        id: OrchestrationId(row.id),
        parent_run_id: AgentRunId(row.parent_run_id),
        workspace_id: WorkspaceId(row.workspace_id),
        thread_id: ThreadId(row.thread_id),
        task: row.task,
        status,
        plan,
        pattern_ids,
        result_summary: row.result_summary,
        created_at: row.created_at,
        updated_at: row.updated_at,
        completed_at: row.completed_at,
    })
}

impl SqliteStorage {
    /// Fetch the allowed read/write paths for a workspace.
    async fn fetch_workspace_paths(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<(Vec<String>, Vec<String>), AppError> {
        let rows = sqlx::query_as::<_, WorkspacePathRow>(
            "SELECT kind, path FROM workspace_paths WHERE workspace_id = ?",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("fetch_workspace_paths failed: {e}"),
        })?;

        let mut read = Vec::new();
        let mut write = Vec::new();
        for row in rows {
            match row.kind.as_str() {
                "read" => read.push(row.path),
                "write" => write.push(row.path),
                _ => {}
            }
        }
        Ok((read, write))
    }
}

async fn validate_tool_invocation_owner(
    pool: &SqlitePool,
    invocation: &ToolInvocation,
) -> Result<(), AppError> {
    let owner: Option<(uuid::Uuid, uuid::Uuid)> =
        sqlx::query_as("SELECT workspace_id, thread_id FROM agent_runs WHERE id = ?")
            .bind(invocation.run_id.0)
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("validate tool invocation owner failed: {e}"),
            })?;
    if owner == Some((invocation.workspace_id.0, invocation.thread_id.0)) {
        Ok(())
    } else {
        Err(AppError::PermissionDenied {
            reason: "tool invocation does not belong to the referenced run".to_owned(),
        })
    }
}

async fn insert_tool_invocation(
    pool: &SqlitePool,
    invocation: &ToolInvocation,
) -> Result<(), AppError> {
    let arguments_json =
        serde_json::to_string(&invocation.arguments).map_err(|e| AppError::Internal {
            message: format!("serialize tool invocation arguments failed: {e}"),
        })?;
    let result_json =
        invocation.result.as_ref().map(serde_json::to_string).transpose().map_err(|e| {
            AppError::Internal {
                message: format!("serialize tool invocation result failed: {e}"),
            }
        })?;
    let recovery_json = invocation
        .recovery
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| AppError::Internal {
            message: format!("serialize tool invocation recovery failed: {e}"),
        })?;
    sqlx::query(INSERT_TOOL_INVOCATION_SQL)
        .bind(invocation.id.0)
        .bind(invocation.run_id.0)
        .bind(invocation.thread_id.0)
        .bind(invocation.workspace_id.0)
        .bind(&invocation.model_call_id)
        .bind(&invocation.tool_name)
        .bind(&invocation.tool_version)
        .bind(&invocation.action)
        .bind(&invocation.resource)
        .bind(arguments_json)
        .bind(&invocation.request_hash)
        .bind(&invocation.policy_version)
        .bind(invocation.context_revision)
        .bind(invocation.status.as_str())
        .bind(invocation.approval_request_id.map(|id| id.0))
        .bind(result_json)
        .bind(&invocation.error)
        .bind(recovery_json)
        .bind(invocation.lease_token)
        .bind(i64::from(invocation.attempts))
        .bind(i64::from(invocation.cancel_requested))
        .bind(invocation.correlation_id)
        .bind(invocation.created_at)
        .bind(invocation.updated_at)
        .bind(invocation.started_at)
        .bind(invocation.completed_at)
        .execute(pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create_tool_invocation failed: {e}"),
        })?;
    Ok(())
}

async fn insert_tool_invocation_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    invocation: &ToolInvocation,
) -> Result<(), AppError> {
    let arguments_json =
        serde_json::to_string(&invocation.arguments).map_err(|e| AppError::Internal {
            message: format!("serialize tool invocation arguments failed: {e}"),
        })?;
    let recovery_json = invocation
        .recovery
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| AppError::Internal {
            message: format!("serialize tool invocation recovery failed: {e}"),
        })?;
    sqlx::query(INSERT_TOOL_INVOCATION_SQL)
        .bind(invocation.id.0)
        .bind(invocation.run_id.0)
        .bind(invocation.thread_id.0)
        .bind(invocation.workspace_id.0)
        .bind(&invocation.model_call_id)
        .bind(&invocation.tool_name)
        .bind(&invocation.tool_version)
        .bind(&invocation.action)
        .bind(&invocation.resource)
        .bind(arguments_json)
        .bind(&invocation.request_hash)
        .bind(&invocation.policy_version)
        .bind(invocation.context_revision)
        .bind(invocation.status.as_str())
        .bind(Option::<i64>::None)
        .bind(Option::<String>::None)
        .bind(&invocation.error)
        .bind(recovery_json)
        .bind(invocation.lease_token)
        .bind(i64::from(invocation.attempts))
        .bind(i64::from(invocation.cancel_requested))
        .bind(invocation.correlation_id)
        .bind(invocation.created_at)
        .bind(invocation.updated_at)
        .bind(invocation.started_at)
        .bind(invocation.completed_at)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create tool invocation failed: {e}"),
        })?;
    Ok(())
}

async fn get_tool_invocation_by_approval(
    pool: &SqlitePool,
    approval_id: ApprovalRequestId,
) -> Result<ToolInvocation, AppError> {
    let row = sqlx::query_as::<_, ToolInvocationRow>(
        "SELECT id, run_id, thread_id, workspace_id, model_call_id, tool_name,
                tool_version, action, resource, arguments_json, request_hash,
                policy_version, context_revision, status, approval_request_id, result_json, error,
                recovery_json, lease_token, attempts, cancel_requested, correlation_id, created_at,
                updated_at, started_at, completed_at
         FROM tool_invocations WHERE approval_request_id = ?",
    )
    .bind(approval_id.0)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal {
        message: format!("get tool invocation by approval failed: {e}"),
    })?;
    row.map(ToolInvocation::try_from)
        .transpose()?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("tool invocation for approval {approval_id:?}"),
        })
}

// Row types for sqlx decoding.
#[derive(sqlx::FromRow)]
struct WorkspaceRow {
    id: uuid::Uuid,
    name: String,
    root_path: String,
    trusted: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl WorkspaceRow {
    fn into_workspace(
        self,
        allowed_read_paths: Vec<String>,
        allowed_write_paths: Vec<String>,
    ) -> Workspace {
        Workspace {
            id: WorkspaceId(self.id),
            name: self.name,
            root_path: self.root_path,
            trusted: self.trusted != 0,
            allowed_read_paths,
            allowed_write_paths,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct WorkspacePathRow {
    kind: String,
    path: String,
}

#[derive(sqlx::FromRow)]
struct ThreadRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    title: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<ThreadRow> for Thread {
    fn from(row: ThreadRow) -> Self {
        Self {
            id: ThreadId(row.id),
            workspace_id: WorkspaceId(row.workspace_id),
            title: row.title,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: uuid::Uuid,
    thread_id: uuid::Uuid,
    run_id: Option<uuid::Uuid>,
    role: String,
    content: String,
    client_request_id: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<MessageRow> for Message {
    fn from(row: MessageRow) -> Self {
        let role = row.role.as_str().try_into().unwrap_or(MessageRole::System);
        Self {
            id: MessageId(row.id),
            thread_id: ThreadId(row.thread_id),
            run_id: row.run_id.map(AgentRunId),
            role,
            content: row.content,
            client_request_id: row.client_request_id,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AgentRunRow {
    id: uuid::Uuid,
    thread_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<AgentRunRow> for AgentRun {
    fn from(row: AgentRunRow) -> Self {
        let status = row.status.as_str().try_into().unwrap_or_else(|_| {
            tracing::error!(status = %row.status, "failed to deserialize AgentRun status");
            AgentRunStatus::Failed
        });
        Self {
            id: AgentRunId(row.id),
            thread_id: ThreadId(row.thread_id),
            workspace_id: WorkspaceId(row.workspace_id),
            status,
            created_at: row.created_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct RunEventRow {
    id: i64,
    run_id: uuid::Uuid,
    thread_id: uuid::Uuid,
    sequence: i64,
    event_type: String,
    payload: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<RunEventRow> for RunEvent {
    fn from(row: RunEventRow) -> Self {
        let payload = serde_json::from_str(&row.payload).unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to deserialize RunEvent payload");
            serde_json::Value::Null
        });
        Self {
            id: row.id,
            run_id: AgentRunId(row.run_id),
            thread_id: ThreadId(row.thread_id),
            sequence: row.sequence,
            event_type: row.event_type,
            payload,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct WorktreeRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    thread_id: uuid::Uuid,
    name: String,
    path: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<WorktreeRow> for Worktree {
    fn from(row: WorktreeRow) -> Self {
        Self {
            id: WorktreeId(row.id),
            workspace_id: WorkspaceId(row.workspace_id),
            thread_id: ThreadId(row.thread_id),
            name: row.name,
            path: row.path,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SubagentRunRow {
    id: uuid::Uuid,
    parent_run_id: uuid::Uuid,
    agent_name: String,
    status: String,
    task_description: String,
    output_summary: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<SubagentRunRow> for SubagentRun {
    fn from(row: SubagentRunRow) -> Self {
        let status = row.status.as_str().try_into().unwrap_or_else(|_| {
            tracing::error!(status = %row.status, "failed to deserialize SubagentRun status");
            AgentRunStatus::Failed
        });
        Self {
            id: AgentRunId(row.id),
            parent_run_id: AgentRunId(row.parent_run_id),
            agent_name: row.agent_name,
            status,
            task_description: row.task_description,
            output_summary: row.output_summary,
            created_at: row.created_at,
            completed_at: row.completed_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct BackgroundTaskRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    thread_id: Option<uuid::Uuid>,
    run_id: Option<uuid::Uuid>,
    task_kind: String,
    payload: String,
    status: String,
    priority: i64,
    attempts: i64,
    max_attempts: i64,
    scheduled_at: chrono::DateTime<chrono::Utc>,
    leased_at: Option<chrono::DateTime<chrono::Utc>>,
    leased_by: Option<String>,
    next_retry_at: Option<chrono::DateTime<chrono::Utc>>,
    error_message: Option<String>,
    result_summary: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl BackgroundTaskRow {
    fn into(self) -> BackgroundTask {
        let payload = serde_json::from_str(&self.payload).unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to deserialize BackgroundTask payload");
            serde_json::Value::Null
        });
        let task_kind = self.task_kind.as_str().try_into().unwrap_or_else(|_| {
            tracing::error!(task_kind = %self.task_kind, "failed to deserialize BackgroundTask task_kind");
            TaskKind::Routine
        });
        let status = self.status.as_str().try_into().unwrap_or_else(|_| {
            tracing::error!(status = %self.status, "failed to deserialize BackgroundTask status");
            BackgroundTaskStatus::Failed
        });
        let attempts = u32::try_from(self.attempts).unwrap_or_else(|e| {
            tracing::error!(error = %e, attempts = self.attempts, "failed to convert BackgroundTask attempts");
            u32::default()
        });
        let max_attempts = u32::try_from(self.max_attempts).unwrap_or_else(|e| {
            tracing::error!(error = %e, max_attempts = self.max_attempts, "failed to convert BackgroundTask max_attempts");
            u32::default()
        });
        BackgroundTask {
            id: BackgroundTaskId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            thread_id: self.thread_id.map(ThreadId),
            run_id: self.run_id.map(AgentRunId),
            task_kind,
            payload,
            status,
            priority: self.priority,
            attempts,
            max_attempts,
            scheduled_at: self.scheduled_at,
            leased_at: self.leased_at,
            leased_by: self.leased_by,
            next_retry_at: self.next_retry_at,
            error_message: self.error_message,
            result_summary: self.result_summary,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AutomationRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    name: String,
    description: String,
    trigger: String,
    cron_expr: Option<String>,
    enabled: i64,
    permission_policy: String,
    next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl AutomationRow {
    fn into(self) -> Automation {
        let permission_policy = serde_json::from_str(&self.permission_policy).unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to deserialize Automation permission_policy");
            serde_json::Value::Null
        });
        let trigger = self.trigger.as_str().try_into().unwrap_or_else(|_| {
            tracing::error!(trigger = %self.trigger, "failed to deserialize Automation trigger");
            AutomationTrigger::ManualRoutine
        });
        Automation {
            id: AutomationId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            name: self.name,
            description: self.description,
            trigger,
            cron_expr: self.cron_expr,
            enabled: self.enabled != 0,
            permission_policy,
            next_run_at: self.next_run_at,
            last_run_at: self.last_run_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    thread_id: Option<uuid::Uuid>,
    run_id: Option<uuid::Uuid>,
    title: String,
    body: String,
    category: String,
    read: i64,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl NotificationRow {
    fn into(self) -> Notification {
        let category = self.category.as_str().try_into().unwrap_or_else(|_| {
            tracing::error!(category = %self.category, "failed to deserialize Notification category");
            NotificationCategory::System
        });
        Notification {
            id: NotificationId(self.id),
            workspace_id: WorkspaceId(self.workspace_id),
            thread_id: self.thread_id.map(ThreadId),
            run_id: self.run_id.map(AgentRunId),
            title: self.title,
            body: self.body,
            category,
            read: self.read != 0,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ApprovalRequestRow {
    id: i64,
    run_id: uuid::Uuid,
    workspace_id: Option<uuid::Uuid>,
    thread_id: Option<uuid::Uuid>,
    action: String,
    resource: String,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    resolution_reason: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ToolInvocationRow {
    id: uuid::Uuid,
    run_id: uuid::Uuid,
    thread_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
    model_call_id: Option<String>,
    tool_name: String,
    tool_version: String,
    action: String,
    resource: String,
    arguments_json: String,
    request_hash: String,
    policy_version: String,
    context_revision: chrono::DateTime<chrono::Utc>,
    status: String,
    approval_request_id: Option<i64>,
    result_json: Option<String>,
    error: Option<String>,
    recovery_json: Option<String>,
    lease_token: Option<uuid::Uuid>,
    attempts: i64,
    cancel_requested: i64,
    correlation_id: uuid::Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl TryFrom<ToolInvocationRow> for ToolInvocation {
    type Error = AppError;

    fn try_from(row: ToolInvocationRow) -> Result<Self, Self::Error> {
        let attempts = u32::try_from(row.attempts).map_err(|e| AppError::Internal {
            message: format!("invalid tool invocation attempts: {e}"),
        })?;
        Ok(Self {
            id: ToolInvocationId(row.id),
            run_id: AgentRunId(row.run_id),
            thread_id: ThreadId(row.thread_id),
            workspace_id: WorkspaceId(row.workspace_id),
            model_call_id: row.model_call_id,
            tool_name: row.tool_name,
            tool_version: row.tool_version,
            action: row.action,
            resource: row.resource,
            arguments: serde_json::from_str(&row.arguments_json).map_err(|e| {
                AppError::Internal {
                    message: format!("deserialize tool invocation arguments failed: {e}"),
                }
            })?,
            request_hash: row.request_hash,
            policy_version: row.policy_version,
            context_revision: row.context_revision,
            status: row.status.as_str().try_into()?,
            approval_request_id: row.approval_request_id.map(ApprovalRequestId),
            result: row.result_json.map(|json| serde_json::from_str(&json)).transpose().map_err(
                |e| AppError::Internal {
                    message: format!("deserialize tool invocation result failed: {e}"),
                },
            )?,
            error: row.error,
            recovery: row
                .recovery_json
                .map(|json| serde_json::from_str(&json))
                .transpose()
                .map_err(|e| AppError::Internal {
                    message: format!("deserialize tool invocation recovery failed: {e}"),
                })?,
            lease_token: row.lease_token,
            attempts,
            cancel_requested: row.cancel_requested != 0,
            correlation_id: row.correlation_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
        })
    }
}

impl ApprovalRequestRow {
    fn into(self) -> ApprovalRequest {
        let workspace_id = self.workspace_id.map_or_else(
            || {
                tracing::error!("ApprovalRequest workspace_id is null, using nil fallback");
                WorkspaceId(uuid::Uuid::nil())
            },
            WorkspaceId,
        );
        let thread_id = self.thread_id.map_or_else(
            || {
                tracing::error!("ApprovalRequest thread_id is null, using nil fallback");
                ThreadId(uuid::Uuid::nil())
            },
            ThreadId,
        );
        let status = self.status.as_str().try_into().unwrap_or_else(|_| {
            tracing::error!(status = %self.status, "failed to deserialize ApprovalRequest status");
            ApprovalRequestStatus::Pending
        });
        ApprovalRequest {
            id: ApprovalRequestId(self.id),
            run_id: AgentRunId(self.run_id),
            workspace_id,
            thread_id,
            action: self.action,
            resource: self.resource,
            status,
            created_at: self.created_at,
            resolved_at: self.resolved_at,
            resolution_reason: self.resolution_reason,
        }
    }
}
