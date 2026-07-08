//! High-level runtime handle that coordinates storage, events, and execution.

use crate::{
    audit_logger::run_audit_event,
    context::ContextInspector,
    events::{EventBus, EventStream, RuntimeEvent},
    executor::{AgentExecutor, MockAgentExecutor},
    notification_center::NotificationCenter,
    provider_registry::ModelProviderRegistry,
    repo_root_provider::StorageRepoRootProvider,
    storage::Storage,
    task_queue::BackgroundTaskQueue,
    workspace::WorkspaceManager,
    worktree::WorktreeManager,
};
use app_memory::{EmbeddingProvider, MemoryManager, SqliteMemoryManager, SqliteRagStore};
use app_models::{
    AgentRun, AgentRunId, AgentRunStatus, AppError, ApprovalRequest, ApprovalRequestId,
    ApprovalRequestStatus, BackgroundTask, BackgroundTaskId, BackgroundTaskStatus, Notification,
    NotificationCategory, NotificationId, RunEvent, TaskKind, Thread, ThreadId, Workspace,
    WorkspaceId,
};
use app_plugins::{McpClientManager, PluginRegistry, SqlitePluginRegistry};
use app_security::{AuditEvent, PermissionRequest, PermissionResult, SecurityContext};
use app_tools::{git::GitTool, terminal::TerminalManager};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// Default timeout for a single run operation (e.g. `submit_message`).
const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(300);

/// Product-level handle used by the Tauri bridge to drive the Portico runtime.
#[derive(Clone)]
pub struct PorticoRuntimeHandle {
    storage: Arc<dyn Storage>,
    event_bus: Arc<dyn EventBus>,
    executor: Option<Arc<dyn AgentExecutor>>,
    registry: Arc<dyn ModelProviderRegistry>,
    plugin_registry: Arc<dyn PluginRegistry>,
    mcp_manager: Arc<Mutex<McpClientManager>>,
    cancellation: Arc<Mutex<HashMap<AgentRunId, CancellationToken>>>,
    security: Option<Arc<SecurityContext>>,
    workspace_manager: Arc<WorkspaceManager>,
    worktree_manager: Arc<WorktreeManager>,
    terminal_manager: Option<Arc<TerminalManager>>,
    git_tool: Option<Arc<GitTool>>,
    memory_manager: Arc<dyn MemoryManager>,
    context_inspector: Arc<ContextInspector>,
    task_queue: BackgroundTaskQueue,
    notification_center: NotificationCenter,
}

impl PorticoRuntimeHandle {
    /// Create a new runtime handle from storage, event bus, registry, and optional executor.
    ///
    /// When `executor` is `None`, the built-in [`MockAgentExecutor`] is used for
    /// `submit_message`.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin registry or MCP manager cannot be initialized.
    pub async fn new(
        storage: Arc<dyn Storage>,
        event_bus: Arc<dyn EventBus>,
        registry: Arc<dyn ModelProviderRegistry>,
        executor: Option<Arc<dyn AgentExecutor>>,
        embedding: Option<Arc<dyn EmbeddingProvider>>,
    ) -> Result<Self, AppError> {
        let default_security = Arc::new(SecurityContext::new(
            Arc::new(app_security::PolicyPermissionEngine::default_rules()),
            Arc::new(app_security::DefaultCommandPolicy::new()),
            Arc::new(app_security::DefaultNetworkPolicy::new()),
            Arc::new(app_security::MemoryAuditLogger::new()),
        ));
        let workspace_manager = Arc::new(WorkspaceManager::new(
            storage.clone(),
            (*default_security).clone(),
        ));
        let worktree_manager = Arc::new(WorktreeManager::new(storage.clone()));
        let terminal_manager = Arc::new(TerminalManager::new(default_security.clone()));
        let git_tool = Arc::new(GitTool::new(
            default_security.clone(),
            Arc::new(StorageRepoRootProvider::new(storage.clone())),
        ));
        let memory_manager: Arc<dyn MemoryManager> =
            Arc::new(SqliteMemoryManager::new(storage.pool().clone()));
        let rag_store = SqliteRagStore::new(storage.pool().clone());
        let context_inspector = Arc::new(ContextInspector::new(
            memory_manager.clone(),
            embedding,
            rag_store,
        ));

        let plugin_registry: Arc<dyn PluginRegistry> =
            Arc::new(SqlitePluginRegistry::new(storage.pool().clone()));
        let mcp_manager = Arc::new(Mutex::new(
            McpClientManager::new_with_pool(storage.pool().clone()).await?,
        ));
        let task_queue = BackgroundTaskQueue::new(storage.clone());
        let notification_center = NotificationCenter::new(storage.clone());

        Ok(Self {
            storage,
            event_bus,
            executor,
            registry,
            plugin_registry,
            mcp_manager,
            cancellation: Arc::new(Mutex::new(HashMap::new())),
            security: Some(default_security),
            workspace_manager,
            worktree_manager,
            terminal_manager: Some(terminal_manager),
            git_tool: Some(git_tool),
            memory_manager,
            context_inspector,
            task_queue,
            notification_center,
        })
    }

    /// Attach a security context to this runtime handle.
    #[must_use]
    pub fn with_security(mut self, security: Arc<SecurityContext>) -> Self {
        self.security = Some(security.clone());
        self.workspace_manager = Arc::new(WorkspaceManager::new(
            self.storage.clone(),
            (*security).clone(),
        ));
        self.terminal_manager = Some(Arc::new(TerminalManager::new(security.clone())));
        self.git_tool = Some(Arc::new(GitTool::new(
            security,
            Arc::new(StorageRepoRootProvider::new(self.storage.clone())),
        )));
        self
    }

    /// Evaluate a permission request against the configured permission engine.
    ///
    /// If no security context is configured, the request is allowed by default.
    #[must_use]
    pub fn evaluate_permission(&self, request: PermissionRequest) -> PermissionResult {
        self.security.as_ref().map_or(PermissionResult::Allowed, |ctx| {
            ctx.permissions.evaluate(request)
        })
    }

    /// Record a security-relevant audit event.
    ///
    /// If no security context is configured, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error if the audit logger fails to persist the event.
    pub fn audit(&self, event: AuditEvent) -> Result<(), AppError> {
        self.security.as_ref().map_or(Ok(()), |ctx| ctx.audit.log(event))
    }

    /// Ensure a cancellation token exists for the given run.
    async fn token_for(&self, run_id: AgentRunId) -> CancellationToken {
        let mut map = self.cancellation.lock().await;
        map.entry(run_id).or_insert_with(CancellationToken::new).clone()
    }

    /// Access the underlying storage.
    #[must_use]
    pub fn storage(&self) -> &Arc<dyn Storage> {
        &self.storage
    }

    /// Access the model provider registry.
    #[must_use]
    pub fn registry(&self) -> &Arc<dyn ModelProviderRegistry> {
        &self.registry
    }

    /// Access the plugin registry.
    #[must_use]
    pub fn plugin_registry(&self) -> &Arc<dyn PluginRegistry> {
        &self.plugin_registry
    }

    /// Access the MCP client manager.
    #[must_use]
    pub const fn mcp_manager(&self) -> &Arc<Mutex<McpClientManager>> {
        &self.mcp_manager
    }

    /// Access the workspace manager.
    #[must_use]
    pub const fn workspace_manager(&self) -> &Arc<WorkspaceManager> {
        &self.workspace_manager
    }

    /// Access the worktree manager.
    #[must_use]
    pub const fn worktree_manager(&self) -> &Arc<WorktreeManager> {
        &self.worktree_manager
    }

    /// Access the terminal manager, if one is configured.
    #[must_use]
    pub const fn terminal_manager(&self) -> Option<&Arc<TerminalManager>> {
        self.terminal_manager.as_ref()
    }

    /// Access the git tool, if one is configured.
    #[must_use]
    pub const fn git_tool(&self) -> Option<&Arc<GitTool>> {
        self.git_tool.as_ref()
    }

    /// Access the memory manager.
    #[must_use]
    pub const fn memory_manager(&self) -> &Arc<dyn MemoryManager> {
        &self.memory_manager
    }

    /// Access the context inspector.
    #[must_use]
    pub const fn context_inspector(&self) -> &Arc<ContextInspector> {
        &self.context_inspector
    }

    /// Access the background task queue.
    #[must_use]
    pub fn task_queue(&self) -> BackgroundTaskQueue {
        self.task_queue.clone()
    }

    /// Access the notification center.
    #[must_use]
    pub fn notification_center(&self) -> NotificationCenter {
        self.notification_center.clone()
    }

    /// Create a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace cannot be persisted.
    pub async fn create_workspace(
        &self,
        name: &str,
        root_path: &str,
        trusted: bool,
    ) -> Result<Workspace, AppError> {
        self.storage.create_workspace(name, root_path, trusted).await
    }

    /// Fetch a workspace by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace is missing.
    pub async fn get_workspace(&self, id: WorkspaceId) -> Result<Workspace, AppError> {
        self.storage.get_workspace(id).await
    }

    /// List all workspaces.
    ///
    /// # Errors
    ///
    /// Returns an error if workspaces cannot be read.
    pub async fn list_workspaces(&self) -> Result<Vec<Workspace>, AppError> {
        self.storage.list_workspaces().await
    }

    /// Fetch a thread by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the thread is missing.
    pub async fn get_thread(&self, id: ThreadId) -> Result<Thread, AppError> {
        self.storage.get_thread(id).await
    }

    /// Fetch a run by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the run is missing.
    pub async fn get_run(&self, id: AgentRunId) -> Result<AgentRun, AppError> {
        self.storage.get_run(id).await
    }

    /// Create a thread in a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace is missing or the thread cannot be persisted.
    pub async fn create_thread(
        &self,
        workspace_id: WorkspaceId,
        title: &str,
    ) -> Result<Thread, AppError> {
        self.storage.create_thread(workspace_id, title).await
    }

    /// List threads in a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if threads cannot be read.
    pub async fn list_threads(&self, workspace_id: WorkspaceId) -> Result<Vec<Thread>, AppError> {
        self.storage.list_threads(workspace_id).await
    }

    /// Start a new run in a workspace/thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be created or started.
    pub async fn start_run(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<AgentRun, AppError> {
        let run = self.storage.create_run(workspace_id, thread_id).await?;

        // Reset/create an uncancelled token for this run.
        {
            let mut map = self.cancellation.lock().await;
            map.insert(run.id, CancellationToken::new());
        }

        self.storage.update_run_status(run.id, AgentRunStatus::Running).await?;
        let updated = self.storage.get_run(run.id).await?;

        self.event_bus
            .publish(RuntimeEvent::RunStarted {
                run: updated.clone(),
                timestamp: chrono::Utc::now(),
            })
            .await?;
        self.event_bus
            .publish(RuntimeEvent::RunStatusChanged {
                run_id: updated.id,
                thread_id: updated.thread_id,
                status: AgentRunStatus::Running,
                timestamp: chrono::Utc::now(),
            })
            .await?;

        Ok(updated)
    }

    /// Submit a user message to a run and drive the configured executor.
    ///
    /// # Errors
    ///
    /// Returns an error if the run is missing, already finished, the executor
    /// returns an error, or the run times out.
    #[allow(clippy::too_many_lines)]
    pub async fn submit_message(&self, run_id: AgentRunId, content: &str) -> Result<(), AppError> {
        let run = self.storage.get_run(run_id).await?;
        if run.status.is_terminal() {
            return Err(AppError::Internal {
                message: format!(
                    "run {} is already in terminal state {:?}",
                    run_id.0, run.status
                ),
            });
        }

        // Move run to Running if it was queued/paused/waiting.
        if run.status != AgentRunStatus::Running {
            self.storage.update_run_status(run_id, AgentRunStatus::Running).await?;
            self.event_bus
                .publish(RuntimeEvent::RunStatusChanged {
                    run_id,
                    thread_id: run.thread_id,
                    status: AgentRunStatus::Running,
                    timestamp: chrono::Utc::now(),
                })
                .await?;
        }

        self.audit(run_audit_event(
            run.workspace_id,
            run.thread_id,
            run_id,
            "run.start",
            PermissionResult::Allowed,
        ))?;

        let token = self.token_for(run_id).await;
        let executor = self.executor.clone();
        let storage = self.storage.clone();
        let event_bus = self.event_bus.clone();
        let thread_id = run.thread_id;
        let workspace_id = run.workspace_id;
        let content = content.to_owned();
        let work_token = token.clone();

        let work = async move {
            match executor {
                Some(exec) => {
                    exec.execute(
                        run_id,
                        thread_id,
                        workspace_id,
                        &content,
                        storage,
                        event_bus,
                        work_token,
                    )
                    .await
                }
                None => {
                    MockAgentExecutor
                        .execute(
                            run_id,
                            thread_id,
                            workspace_id,
                            &content,
                            storage,
                            event_bus,
                            work_token,
                        )
                        .await
                }
            }
        };

        let result = timeout(DEFAULT_RUN_TIMEOUT, work).await;

        match result {
            Ok(Ok(())) => {
                if token.is_cancelled() {
                    self.finalize_run_status(run_id, AgentRunStatus::Cancelled).await?;
                    let _ = self.audit(run_audit_event(
                        workspace_id,
                        thread_id,
                        run_id,
                        "run.cancel",
                        PermissionResult::Allowed,
                    ));
                } else {
                    self.finalize_run_status(run_id, AgentRunStatus::Completed).await?;
                    self.event_bus
                        .publish(RuntimeEvent::RunCompleted {
                            run_id,
                            thread_id,
                            timestamp: chrono::Utc::now(),
                        })
                        .await?;
                    let _ = self.audit(run_audit_event(
                        workspace_id,
                        thread_id,
                        run_id,
                        "run.complete",
                        PermissionResult::Allowed,
                    ));
                }
                self.cancellation.lock().await.remove(&run_id);
                Ok(())
            }
            Ok(Err(err)) => {
                self.finalize_run_status(run_id, AgentRunStatus::Failed).await?;
                self.event_bus
                    .publish(RuntimeEvent::RunFailed {
                        run_id,
                        thread_id,
                        error: err.to_string(),
                        timestamp: chrono::Utc::now(),
                    })
                    .await?;
                let _ = self.audit(run_audit_event(
                    workspace_id,
                    thread_id,
                    run_id,
                    "run.failed",
                    PermissionResult::Denied {
                        reason: err.to_string(),
                    },
                ));
                self.cancellation.lock().await.remove(&run_id);
                Err(err)
            }
            Err(_) => {
                token.cancel();
                self.finalize_run_status(run_id, AgentRunStatus::Failed).await?;
                let err = AppError::Internal {
                    message: format!(
                        "run {run_id:?} timed out after {} seconds",
                        DEFAULT_RUN_TIMEOUT.as_secs()
                    ),
                };
                self.event_bus
                    .publish(RuntimeEvent::RunFailed {
                        run_id,
                        thread_id,
                        error: err.to_string(),
                        timestamp: chrono::Utc::now(),
                    })
                    .await?;
                let _ = self.audit(run_audit_event(
                    workspace_id,
                    thread_id,
                    run_id,
                    "run.failed",
                    PermissionResult::Denied {
                        reason: err.to_string(),
                    },
                ));
                self.cancellation.lock().await.remove(&run_id);
                Err(err)
            }
        }
    }

    /// Update a run's status to a terminal value and emit `RunStatusChanged`.
    ///
    /// This is a no-op if the run is already in the requested status, which
    /// avoids duplicate events when `cancel_run` races with `submit_message`.
    async fn finalize_run_status(
        &self,
        run_id: AgentRunId,
        status: AgentRunStatus,
    ) -> Result<(), AppError> {
        let run = self.storage.get_run(run_id).await?;
        if run.status == status {
            return Ok(());
        }
        self.storage.update_run_status(run_id, status).await?;
        self.event_bus
            .publish(RuntimeEvent::RunStatusChanged {
                run_id,
                thread_id: run.thread_id,
                status,
                timestamp: chrono::Utc::now(),
            })
            .await?;
        Ok(())
    }

    /// Cancel a running run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be updated.
    pub async fn cancel_run(&self, run_id: AgentRunId) -> Result<(), AppError> {
        let token = self.token_for(run_id).await;
        token.cancel();

        let run = self.storage.get_run(run_id).await?;
        if !run.status.is_terminal() {
            self.storage.update_run_status(run_id, AgentRunStatus::Cancelled).await?;
            self.event_bus
                .publish(RuntimeEvent::RunStatusChanged {
                    run_id,
                    thread_id: run.thread_id,
                    status: AgentRunStatus::Cancelled,
                    timestamp: chrono::Utc::now(),
                })
                .await?;
            let _ = self.audit(run_audit_event(
                run.workspace_id,
                run.thread_id,
                run_id,
                "run.cancel",
                PermissionResult::Allowed,
            ));
        }
        Ok(())
    }

    /// Pause a running run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be updated.
    pub async fn pause_run(&self, run_id: AgentRunId) -> Result<(), AppError> {
        let run = self.storage.get_run(run_id).await?;
        if run.status == AgentRunStatus::Running {
            self.storage.update_run_status(run_id, AgentRunStatus::Paused).await?;
            self.event_bus
                .publish(RuntimeEvent::RunStatusChanged {
                    run_id,
                    thread_id: run.thread_id,
                    status: AgentRunStatus::Paused,
                    timestamp: chrono::Utc::now(),
                })
                .await?;
        }
        Ok(())
    }

    /// Resume a paused run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be updated.
    pub async fn resume_run(&self, run_id: AgentRunId) -> Result<(), AppError> {
        let run = self.storage.get_run(run_id).await?;
        if run.status == AgentRunStatus::Paused || run.status == AgentRunStatus::WaitingApproval {
            self.storage.update_run_status(run_id, AgentRunStatus::Running).await?;
            self.event_bus
                .publish(RuntimeEvent::RunStatusChanged {
                    run_id,
                    thread_id: run.thread_id,
                    status: AgentRunStatus::Running,
                    timestamp: chrono::Utc::now(),
                })
                .await?;
        }
        Ok(())
    }

    /// Evaluate a permission request in the context of an active run.
    ///
    /// If the engine returns [`PermissionResult::Ask`] and the request includes a
    /// run id, an [`ApprovalRequest`] is persisted, the run is moved to
    /// [`AgentRunStatus::WaitingApproval`], and a [`RuntimeEvent::RunStatusChanged`]
    /// event is published. The ask result is returned so the caller can pause
    /// further execution.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be resolved or the approval request
    /// cannot be persisted.
    pub async fn evaluate_permission_for_run(
        &self,
        request: PermissionRequest,
    ) -> Result<PermissionResult, AppError> {
        let result = self.evaluate_permission(request.clone());
        let PermissionResult::Ask { request: approval } = result else {
            return Ok(result);
        };

        let Some(run_id) = request.run_id else {
            return Ok(PermissionResult::Ask { request: approval });
        };

        let run = self.storage.get_run(run_id).await?;
        let approval = ApprovalRequest {
            id: approval.id,
            run_id,
            workspace_id: run.workspace_id,
            thread_id: run.thread_id,
            action: approval.action,
            resource: approval.resource,
            status: ApprovalRequestStatus::Pending,
            created_at: chrono::Utc::now(),
            resolved_at: None,
            resolution_reason: None,
        };
        let approval = self.storage.create_approval_request(&approval).await?;

        self.storage.update_run_status(run_id, AgentRunStatus::WaitingApproval).await?;
        self.event_bus
            .publish(RuntimeEvent::RunStatusChanged {
                run_id,
                thread_id: run.thread_id,
                status: AgentRunStatus::WaitingApproval,
                timestamp: chrono::Utc::now(),
            })
            .await?;
        self.event_bus
            .publish(RuntimeEvent::ToolApprovalRequired {
                run_id,
                thread_id: run.thread_id,
                request_id: approval.id.0,
                action: approval.action.clone(),
                resource: approval.resource.clone(),
                timestamp: chrono::Utc::now(),
            })
            .await?;

        Ok(PermissionResult::Ask { request: approval })
    }

    /// Fetch an approval request by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the request is missing.
    pub async fn get_approval_request(
        &self,
        id: ApprovalRequestId,
    ) -> Result<ApprovalRequest, AppError> {
        self.storage.get_approval_request(id).await
    }

    /// List pending approval requests, optionally filtered by run.
    ///
    /// # Errors
    ///
    /// Returns an error if requests cannot be read.
    pub async fn list_pending_approval_requests(
        &self,
        run_id: Option<AgentRunId>,
    ) -> Result<Vec<ApprovalRequest>, AppError> {
        self.storage.list_pending_approval_requests(run_id).await
    }

    /// Approve a pending approval request and resume its run.
    ///
    /// # Errors
    ///
    /// Returns an error if the request is missing, not pending, or the run
    /// cannot be resumed.
    pub async fn approve_request(&self, id: ApprovalRequestId) -> Result<(), AppError> {
        let approval = self.storage.get_approval_request(id).await?;
        if approval.status != ApprovalRequestStatus::Pending {
            return Err(AppError::Internal {
                message: format!("approval request {id:?} is not pending"),
            });
        }

        self.storage
            .resolve_approval_request(id, ApprovalRequestStatus::Approved, None)
            .await?;
        self.resume_run(approval.run_id).await?;
        Ok(())
    }

    /// Deny a pending approval request and fail its run.
    ///
    /// # Errors
    ///
    /// Returns an error if the request is missing, not pending, or the run
    /// cannot be updated.
    pub async fn deny_request(
        &self,
        id: ApprovalRequestId,
        reason: Option<String>,
    ) -> Result<(), AppError> {
        let approval = self.storage.get_approval_request(id).await?;
        if approval.status != ApprovalRequestStatus::Pending {
            return Err(AppError::Internal {
                message: format!("approval request {id:?} is not pending"),
            });
        }

        let reason_str = reason.as_deref().unwrap_or("denied by user");
        self.storage
            .resolve_approval_request(id, ApprovalRequestStatus::Denied, Some(reason_str))
            .await?;

        let run = self.storage.get_run(approval.run_id).await?;
        if !run.status.is_terminal() {
            self.storage.update_run_status(approval.run_id, AgentRunStatus::Failed).await?;
            self.event_bus
                .publish(RuntimeEvent::RunStatusChanged {
                    run_id: approval.run_id,
                    thread_id: run.thread_id,
                    status: AgentRunStatus::Failed,
                    timestamp: chrono::Utc::now(),
                })
                .await?;
            self.event_bus
                .publish(RuntimeEvent::RunFailed {
                    run_id: approval.run_id,
                    thread_id: run.thread_id,
                    error: reason_str.to_owned(),
                    timestamp: chrono::Utc::now(),
                })
                .await?;
        }

        Ok(())
    }

    /// List persisted events for a run.
    ///
    /// # Errors
    ///
    /// Returns an error if events cannot be read.
    pub async fn list_run_events(&self, run_id: AgentRunId) -> Result<Vec<RunEvent>, AppError> {
        self.storage.list_run_events(run_id).await
    }

    /// Subscribe to runtime events for a run.
    ///
    /// # Errors
    ///
    /// Returns an error if the event bus cannot create a subscription.
    pub fn subscribe_events(&self, run_id: AgentRunId) -> Result<Box<dyn EventStream>, AppError> {
        self.event_bus.subscribe(run_id)
    }

    /// Subscribe to all runtime events.
    ///
    /// # Errors
    ///
    /// Returns an error if the event bus cannot create a subscription.
    pub fn subscribe_all_events(&self) -> Result<Box<dyn EventStream>, AppError> {
        self.event_bus.subscribe_all()
    }

    /// List persisted audit log entries, optionally filtered.
    ///
    /// # Errors
    ///
    /// Returns an error if the audit log cannot be read.
    pub async fn list_audit_log(
        &self,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        run_id: Option<AgentRunId>,
    ) -> Result<Vec<crate::AuditLogEntry>, AppError> {
        self.storage.list_audit_log(workspace_id, thread_id, run_id).await
    }

    /// Create a notification in the notification center.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification cannot be persisted.
    pub async fn create_notification(
        &self,
        workspace_id: WorkspaceId,
        thread_id: Option<ThreadId>,
        run_id: Option<AgentRunId>,
        title: impl Into<String>,
        body: impl Into<String>,
        category: NotificationCategory,
    ) -> Result<Notification, AppError> {
        self.notification_center
            .create(workspace_id, thread_id, run_id, title, body, category)
            .await
    }

    /// List notifications in a workspace, or across all workspaces when `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if notifications cannot be read.
    pub async fn list_notifications(
        &self,
        workspace_id: Option<WorkspaceId>,
        unread_only: bool,
        limit: i64,
    ) -> Result<Vec<Notification>, AppError> {
        self.notification_center.list(workspace_id, unread_only, limit).await
    }

    /// Fetch a notification by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification is missing.
    pub async fn get_notification(&self, id: NotificationId) -> Result<Notification, AppError> {
        self.notification_center.get(id).await
    }

    /// Mark a notification as read or unread.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification is missing.
    pub async fn mark_notification_read(
        &self,
        id: NotificationId,
        read: bool,
    ) -> Result<(), AppError> {
        self.notification_center.mark_read(id, read).await
    }

    /// Dismiss (delete) a notification.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification is missing.
    pub async fn dismiss_notification(&self, id: NotificationId) -> Result<(), AppError> {
        self.notification_center.dismiss(id).await
    }

    /// Enqueue a background task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task cannot be persisted.
    #[allow(clippy::too_many_arguments)]
    pub async fn enqueue_background_task(
        &self,
        workspace_id: WorkspaceId,
        thread_id: Option<ThreadId>,
        run_id: Option<AgentRunId>,
        task_kind: TaskKind,
        payload: serde_json::Value,
        priority: i64,
        scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
        max_attempts: Option<u32>,
    ) -> Result<BackgroundTask, AppError> {
        self.task_queue
            .enqueue(
                workspace_id,
                thread_id,
                run_id,
                task_kind,
                payload,
                priority,
                scheduled_at,
                max_attempts,
            )
            .await
    }

    /// List background tasks, optionally filtered by workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if tasks cannot be read.
    pub async fn list_background_tasks(
        &self,
        workspace_id: Option<WorkspaceId>,
        status: Option<BackgroundTaskStatus>,
        limit: i64,
    ) -> Result<Vec<BackgroundTask>, AppError> {
        self.storage.list_background_tasks(workspace_id, status, limit).await
    }

    /// Fetch a background task by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the task is missing.
    pub async fn get_background_task(
        &self,
        id: BackgroundTaskId,
    ) -> Result<BackgroundTask, AppError> {
        self.storage.get_background_task(id).await
    }
}
