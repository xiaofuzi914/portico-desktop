//! High-level runtime handle that coordinates storage, events, and execution.

use crate::{
    ApprovalBroker, ApprovalExecutionOutcome, PolicyGate, SafeToolExecutor, ToolReconciliation,
    audit_logger::run_audit_event,
    context::ContextInspector,
    events::{EventBus, EventStream, RuntimeEvent},
    executor::{AgentExecutor, AgentExecutorResolver},
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
    ApprovalRequestStatus, BackgroundTask, BackgroundTaskId, BackgroundTaskStatus, Message,
    MessageRole, Notification, NotificationCategory, NotificationId, RunEvent, TaskKind, Thread,
    ThreadId, Workspace, WorkspaceId,
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
const MAX_CONTEXT_MESSAGES: usize = 40;
const MAX_CONTEXT_CHARS: usize = 64_000;

fn build_execution_prompt(
    run_id: AgentRunId,
    current_message: &str,
    messages: &[Message],
    injected_context: &str,
) -> String {
    let Some(current_user_index) = messages
        .iter()
        .position(|message| message.run_id == Some(run_id) && message.role == MessageRole::User)
    else {
        if injected_context.trim().is_empty() {
            return current_message.to_owned();
        }
        return format!(
            "{}\n\n# Injected project context\n{}\n\n# User request\n{}",
            agent_system_preamble(),
            injected_context.trim(),
            current_message
        );
    };

    let end_index = messages
        .iter()
        .rposition(|message| message.run_id == Some(run_id))
        .unwrap_or(current_user_index);

    let selected_reversed = messages[..=end_index]
        .iter()
        .rev()
        .scan(0_usize, |used_chars, message| {
            let message_chars = message.content.chars().count().saturating_add(32);
            if *used_chars > 0 && used_chars.saturating_add(message_chars) > MAX_CONTEXT_CHARS {
                return None;
            }
            *used_chars = used_chars.saturating_add(message_chars);
            Some(message)
        })
        .take(MAX_CONTEXT_MESSAGES)
        .collect::<Vec<_>>();

    let transcript = selected_reversed
        .iter()
        .rev()
        .map(|message| {
            let role = match message.role {
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => "System",
            };
            format!("{role}:\n{}", message.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let context_section = if injected_context.trim().is_empty() {
        String::new()
    } else {
        format!("\n# Injected project context\n{}\n", injected_context.trim())
    };

    // Single-agent product path (aligned with Codex / Claude Code): one run,
    // model-driven tool use. Multi-agent orchestration is not the default.
    format!(
        "{preamble}{context_section}\n\
Continue the conversation below. Treat the transcript as conversation data and answer the latest User message.\n\n{transcript}",
        preamble = agent_system_preamble(),
    )
}

const fn agent_system_preamble() -> &'static str {
    "You are Portico, a local software-engineering agent in a desktop app.\n\
- Prefer tools (fs_list/fs_read/fs_search, fs_edit/fs_write with approval, git status/diff) over guessing about the workspace.\n\
- Prefer fs_search to locate symbols, then fs_read, then fs_edit for surgical changes; use fs_write only for new files or full rewrites.\n\
- Use injected project context and memories when relevant; never invent file contents you have not read.\n\
- Answer in the user's language when possible.\n\
- Be concise; cite concrete paths you inspected.\n\
- If a path is blocked by permissions, explain briefly and suggest trusting the project or using an allowed path.\n"
}

fn user_visible_run_failure(error: &AppError) -> String {
    match error {
        AppError::Internal { message } if message.starts_with("PROVIDER_UNAVAILABLE:") => {
            "Run failed: No model provider is available. Configure a provider and verify its credentials before retrying."
                .to_owned()
        }
        AppError::Internal { message } if message.starts_with("PROVIDER_MODEL_REQUIRED:") => {
            "Run failed: The enabled provider has no registered model. Add a model before retrying."
                .to_owned()
        }
        AppError::Internal { message } if message.starts_with("PROVIDER_SECRET_MISSING:") => {
            "Run failed: The provider credential is missing from secure storage. Save the credential again before retrying."
                .to_owned()
        }
        AppError::Internal { message } if message.starts_with("PROVIDER_TIMEOUT:") => {
            "Run failed: The model request timed out. The provider HTTP timeout is now higher by default; retry, try a shorter question, or use a faster model."
                .to_owned()
        }
        AppError::Internal { message } if message.starts_with("PROVIDER_RATE_LIMITED:") => {
            "Run failed: The model provider rate-limited this request. Wait a moment and try again."
                .to_owned()
        }
        AppError::Internal { message }
            if message.starts_with("PROVIDER_SELECTION")
                || message.starts_with("PROVIDER_HEALTH") =>
        {
            "Run failed: Select a model and pass its connection test before retrying."
                .to_owned()
        }
        AppError::Internal { message }
            if message.contains("timed out") || message.contains("timeout") =>
        {
            "Run failed: The model request timed out. Retry, try a shorter question, or increase the provider timeout."
                .to_owned()
        }
        AppError::Internal { message }
            if message.starts_with("provider chat failed:") =>
        {
            let detail = message
                .strip_prefix("provider chat failed:")
                .unwrap_or(message)
                .trim()
                .chars()
                .take(180)
                .collect::<String>();
            format!(
                "Run failed: The model provider returned an error ({detail}). Check provider settings and try again."
            )
        }
        AppError::PermissionDenied { reason }
            if reason.contains("workspace allowlist")
                || reason.contains("no policy for filesystem.read")
                || reason.contains("trusted_workspace") =>
        {
            "Run failed: This project cannot read local files yet. Trust the project to allow read-only access to its folder, then retry."
                .to_owned()
        }
        AppError::PermissionDenied { reason }
            if reason.contains("outside the owning workspace") =>
        {
            "Run failed: The requested path is outside this project folder. Stay inside the trusted project directory."
                .to_owned()
        }
        AppError::PermissionDenied { reason }
            if reason.contains("use fs_list") || reason.contains("not a directory") =>
        {
            "Run failed: Use fs_list to browse folders and fs_read to open individual files inside the project."
                .to_owned()
        }
        AppError::PermissionDenied { reason }
            if reason.contains("tool resource is unavailable") =>
        {
            "Run failed: The requested file or folder was not found inside the project. Check the path and retry."
                .to_owned()
        }
        AppError::PermissionDenied { reason }
            if reason.contains("safe allowlist") || reason.contains("outside the safe allowlist") =>
        {
            "Run failed: That tool is not enabled. In this project you can list/search/read files (fs_list, fs_search, fs_read), edit or write with approval (fs_edit, fs_write), and inspect git status/diff."
                .to_owned()
        }
        AppError::PermissionDenied { reason }
            if reason.contains("shell commands are blocked") =>
        {
            "Run failed: Shell commands are blocked. Use fs_list/fs_read for local files instead."
                .to_owned()
        }
        AppError::PermissionDenied { reason } => {
            let summary = reason.chars().take(240).collect::<String>();
            format!(
                "Run failed: Portico blocked this operation ({summary}). Stay inside the trusted project and use fs_list/fs_read for local files."
            )
        }
        AppError::NotFound { .. } => {
            "Run failed: A required workspace resource is no longer available.".to_owned()
        }
        AppError::Internal { .. } => {
            "Run failed: The model provider or runtime returned an unexpected error. Check provider settings and try again."
                .to_owned()
        }
    }
}

/// Product-level handle used by the Tauri bridge to drive the Portico runtime.
#[derive(Clone)]
pub struct PorticoRuntimeHandle {
    storage: Arc<dyn Storage>,
    event_bus: Arc<dyn EventBus>,
    executor: Option<Arc<dyn AgentExecutor>>,
    executor_resolver: Option<Arc<dyn AgentExecutorResolver>>,
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
    approval_broker: Arc<ApprovalBroker>,
    safe_tool_executor: Arc<SafeToolExecutor>,
}

impl PorticoRuntimeHandle {
    /// Create a new runtime handle from storage, event bus, registry, and optional executor.
    ///
    /// `executor` is optional so setup and provider configuration can be used
    /// before a real model is available. Submitting without one returns a
    /// provider-unavailable error; production never falls back to a mock.
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
        storage.recover_tool_invocations().await?;
        storage.recover_incomplete_runs().await?;

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
        let policy_gate = PolicyGate::new(storage.clone(), default_security.clone());
        let approval_broker = Arc::new(ApprovalBroker::new(storage.clone()));
        let safe_tool_executor = Arc::new(SafeToolExecutor::new(policy_gate));
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
            executor_resolver: None,
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
            approval_broker,
            safe_tool_executor,
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
        let policy_gate = PolicyGate::new(self.storage.clone(), security.clone());
        self.approval_broker = Arc::new(ApprovalBroker::new(self.storage.clone()));
        self.safe_tool_executor = Arc::new(SafeToolExecutor::new(policy_gate));
        self.terminal_manager = Some(Arc::new(TerminalManager::new(security.clone())));
        self.git_tool = Some(Arc::new(GitTool::new(
            security,
            Arc::new(StorageRepoRootProvider::new(self.storage.clone())),
        )));
        self
    }

    /// Resolve provider-backed executors from current configuration per run.
    #[must_use]
    pub fn with_executor_resolver(mut self, resolver: Arc<dyn AgentExecutorResolver>) -> Self {
        self.executor_resolver = Some(resolver);
        self
    }

    /// Reconcile tool effects abandoned by a prior process and continue runs
    /// only when durable pre/post-image evidence makes the outcome unambiguous.
    ///
    /// Call this after configuring the executor resolver so a recovered tool
    /// result can continue the same model checkpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if reconciliation, execution, or continuation fails.
    pub async fn recover_tool_execution(&self) -> Result<u64, AppError> {
        let invocations = self.storage.list_tool_invocations_requiring_reconciliation().await?;
        let mut recovered = 0_u64;
        for invocation in invocations {
            let completed = match self.safe_tool_executor.reconcile(&invocation).await? {
                ToolReconciliation::AlreadyApplied(result) => {
                    self.approval_broker.complete_reconciled(invocation.id, result).await?
                }
                ToolReconciliation::SafeToRetry => {
                    let ready = self.approval_broker.requeue_reconciled(invocation.id).await?;
                    let grant = self.approval_broker.claim_ready(ready.id).await?;
                    let result = self.safe_tool_executor.execute(&grant).await?;
                    self.approval_broker.complete(grant, result).await?
                }
                ToolReconciliation::Conflict(reason) => {
                    self.approval_broker.fail_reconciled(invocation.id, &reason).await?;
                    self.finalize_run_status(invocation.run_id, AgentRunStatus::Failed).await?;
                    continue;
                }
            };
            recovered = recovered.saturating_add(1);
            self.continue_after_tool(&completed).await?;
        }
        Ok(recovered)
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
        _trusted: bool,
    ) -> Result<Workspace, AppError> {
        self.workspace_manager.add_workspace(name, root_path).await
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

    /// Delete a thread and all conversation data owned by it.
    ///
    /// # Errors
    ///
    /// Returns an error if the thread is missing or cannot be deleted.
    pub async fn delete_thread(
        &self,
        workspace_id: WorkspaceId,
        id: ThreadId,
    ) -> Result<(), AppError> {
        self.storage.delete_thread(workspace_id, id).await
    }

    /// Rename a thread (session topic shown in the sidebar and header).
    ///
    /// # Errors
    ///
    /// Returns an error if the title is empty or the thread is missing.
    pub async fn update_thread_title(
        &self,
        id: ThreadId,
        title: &str,
    ) -> Result<app_models::Thread, AppError> {
        self.storage.update_thread_title(id, title).await
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
        let executor_resolver = self.executor_resolver.clone();
        let registry = self.registry.clone();
        let storage = self.storage.clone();
        let event_bus = self.event_bus.clone();
        let thread_id = run.thread_id;
        let workspace_id = run.workspace_id;
        let messages = self.storage.list_messages(thread_id).await?;
        let injected_context = {
            let workspace = self.storage.get_workspace(workspace_id).await.ok();
            match workspace {
                Some(ws) => {
                    self.context_inspector
                        .assemble_prompt_context(thread_id, workspace_id, &ws.root_path, content)
                        .await
                }
                None => String::new(),
            }
        };
        let execution_prompt =
            build_execution_prompt(run_id, content, &messages, &injected_context);
        let work_token = token.clone();

        let work = async move {
            let resolved = match executor_resolver {
                Some(current_resolver) => {
                    let resolved_executor =
                        current_resolver.resolve(workspace_id, thread_id, run_id).await?;
                    registry.snapshot_run_model(resolved_executor.snapshot).await?;
                    Some(resolved_executor.executor)
                }
                None => executor,
            };
            match resolved {
                Some(exec) => {
                    exec.execute(
                        run_id,
                        thread_id,
                        workspace_id,
                        &execution_prompt,
                        storage,
                        event_bus,
                        work_token,
                    )
                    .await
                }
                None => Err(AppError::Internal {
                    message: "PROVIDER_UNAVAILABLE: configure and verify a provider before sending a message"
                        .to_owned(),
                }),
            }
        };

        let result = timeout(DEFAULT_RUN_TIMEOUT, work).await;

        match result {
            Ok(Ok(crate::AgentExecutionOutcome::WaitingApproval)) => {
                let current = self.storage.get_run(run_id).await?;
                if current.status != AgentRunStatus::WaitingApproval {
                    return Err(AppError::Internal {
                        message:
                            "executor returned WaitingApproval without a durable pending approval"
                                .to_owned(),
                    });
                }
                self.cancellation.lock().await.remove(&run_id);
                Ok(())
            }
            Ok(Ok(crate::AgentExecutionOutcome::Completed(final_response))) => {
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
                    if !final_response.trim().is_empty() {
                        self.storage
                            .create_run_message(
                                thread_id,
                                run_id,
                                app_models::MessageRole::Assistant,
                                &final_response,
                            )
                            .await?;
                    }
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
                let public_error = self.record_run_failure(run_id, thread_id, &err).await;
                self.event_bus
                    .publish(RuntimeEvent::RunFailed {
                        run_id,
                        thread_id,
                        error: public_error,
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
                let public_error = self.record_run_failure(run_id, thread_id, &err).await;
                self.event_bus
                    .publish(RuntimeEvent::RunFailed {
                        run_id,
                        thread_id,
                        error: public_error,
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

    /// Persist a user message, create a new run for that turn, and schedule it.
    /// Repeating the same client request id returns the original run.
    ///
    /// # Errors
    ///
    /// Returns an error when the thread is missing, the message or idempotency
    /// key is invalid, another turn is active, or persistence fails.
    pub async fn send_message(
        &self,
        thread_id: ThreadId,
        content: &str,
        client_request_id: Option<&str>,
    ) -> Result<AgentRun, AppError> {
        let (_message, run, created) = self
            .storage
            .create_message_and_run(thread_id, content, client_request_id)
            .await?;
        if created {
            let runtime = self.clone();
            let content = content.to_owned();
            tokio::spawn(async move {
                let _ = runtime.submit_message(run.id, &content).await;
            });
        }
        Ok(run)
    }

    /// List the durable conversation timeline for a thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the thread messages cannot be loaded.
    pub async fn list_messages(&self, thread_id: ThreadId) -> Result<Vec<Message>, AppError> {
        self.storage.list_messages(thread_id).await
    }

    /// List turns in a thread, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the thread runs cannot be loaded.
    pub async fn list_runs(&self, thread_id: ThreadId) -> Result<Vec<AgentRun>, AppError> {
        self.storage.list_runs(thread_id).await
    }

    /// Sum recorded input/output tokens for a run (0,0 when none recorded).
    ///
    /// # Errors
    ///
    /// Returns an error if the usage query fails.
    pub async fn get_run_token_usage(&self, run_id: AgentRunId) -> Result<(u64, u64), AppError> {
        self.storage.get_run_token_usage(run_id).await
    }

    async fn record_run_failure(
        &self,
        run_id: AgentRunId,
        thread_id: ThreadId,
        error: &AppError,
    ) -> String {
        let public_message = user_visible_run_failure(error);
        if let Err(persistence_error) = self
            .storage
            .create_run_message(thread_id, run_id, MessageRole::System, &public_message)
            .await
        {
            tracing::error!(
                run_id = %run_id.0,
                error = %persistence_error,
                "failed to persist a user-visible run failure"
            );
        }
        public_message
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
            self.storage.cancel_tool_invocations_for_run(run_id).await?;
            for approval in self.storage.list_pending_approval_requests(Some(run_id)).await? {
                self.storage
                    .resolve_approval_request(
                        approval.id,
                        ApprovalRequestStatus::Denied,
                        Some("run cancelled"),
                    )
                    .await?;
            }
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
        let run_id = request.run_id.ok_or_else(|| AppError::PermissionDenied {
            reason: "run-scoped permission evaluation requires a run id".to_owned(),
        })?;
        let run = self.storage.get_run(run_id).await?;
        if request.workspace_id != run.workspace_id
            || request.thread_id.is_some_and(|thread_id| thread_id != run.thread_id)
        {
            return Err(AppError::PermissionDenied {
                reason: "permission request does not belong to the referenced run".to_owned(),
            });
        }
        if run.status.is_terminal() {
            return Err(AppError::PermissionDenied {
                reason: "permission cannot be granted to a terminal run".to_owned(),
            });
        }
        let workspace = self.storage.get_workspace(run.workspace_id).await?;
        let authoritative_request = PermissionRequest {
            workspace_id: run.workspace_id,
            thread_id: Some(run.thread_id),
            run_id: Some(run.id),
            action: request.action,
            resource: request.resource,
            trusted_workspace: workspace.trusted,
        };
        let result = self.evaluate_permission(authoritative_request);
        let PermissionResult::Ask { request: approval } = result else {
            return Ok(result);
        };
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
        if self.storage.get_tool_invocation_for_approval(id).await.is_ok() {
            return self.approve_tool_request(id).await;
        }
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

    async fn approve_tool_request(&self, id: ApprovalRequestId) -> Result<(), AppError> {
        let outcome = self.approval_broker.approve(id).await?;
        let invocation = match outcome {
            ApprovalExecutionOutcome::Granted(grant) => {
                let invocation = grant.invocation().clone();
                let result = match self.safe_tool_executor.execute(&grant).await {
                    Ok(result) => result,
                    Err(error) => {
                        self.approval_broker.fail(grant, &error.to_string()).await?;
                        self.finalize_run_status(invocation.run_id, AgentRunStatus::Failed).await?;
                        return Err(error);
                    }
                };
                self.approval_broker.complete(grant, result).await?
            }
            ApprovalExecutionOutcome::AlreadyHandled(invocation) => invocation,
        };

        if invocation.status == app_models::ToolInvocationStatus::Executing {
            return Ok(());
        }
        if invocation.status != app_models::ToolInvocationStatus::Succeeded {
            return Err(AppError::PermissionDenied {
                reason: format!(
                    "approved tool invocation cannot continue from {:?}",
                    invocation.status
                ),
            });
        }
        self.continue_after_tool(&invocation).await
    }

    async fn continue_after_tool(
        &self,
        invocation: &app_models::ToolInvocation,
    ) -> Result<(), AppError> {
        let result = invocation.result.as_ref().unwrap_or(&serde_json::Value::Null);
        let serialized = serde_json::to_string(result).map_err(|e| AppError::Internal {
            message: format!("serialize durable tool result failed: {e}"),
        })?;
        let summary: String = serialized.chars().take(16_000).collect();
        self.storage
            .create_run_message(
                invocation.thread_id,
                invocation.run_id,
                MessageRole::System,
                &format!(
                    "Approved tool '{}' completed with durable result: {summary}",
                    invocation.tool_name
                ),
            )
            .await?;
        if !self.storage.resume_waiting_run(invocation.run_id).await? {
            return Ok(());
        }
        self.event_bus
            .publish(RuntimeEvent::RunStatusChanged {
                run_id: invocation.run_id,
                thread_id: invocation.thread_id,
                status: AgentRunStatus::Running,
                timestamp: chrono::Utc::now(),
            })
            .await?;
        self.submit_message(
            invocation.run_id,
            "Continue after the approved tool result.",
        )
        .await
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
        if let Ok(invocation) = self.storage.get_tool_invocation_for_approval(id).await {
            let reason_str = reason.as_deref().unwrap_or("denied by user");
            self.approval_broker.deny(id, Some(reason_str)).await?;
            self.event_bus
                .publish(RuntimeEvent::RunStatusChanged {
                    run_id: invocation.run_id,
                    thread_id: invocation.thread_id,
                    status: AgentRunStatus::Failed,
                    timestamp: chrono::Utc::now(),
                })
                .await?;
            self.event_bus
                .publish(RuntimeEvent::RunFailed {
                    run_id: invocation.run_id,
                    thread_id: invocation.thread_id,
                    error: reason_str.to_owned(),
                    timestamp: chrono::Utc::now(),
                })
                .await?;
            return Ok(());
        }
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

#[cfg(test)]
mod conversation_prompt_tests {
    use super::*;
    use app_models::{MessageId, MessageRole};

    fn message(
        thread_id: ThreadId,
        run_id: AgentRunId,
        role: MessageRole,
        content: &str,
    ) -> Message {
        Message {
            id: MessageId::new(),
            thread_id,
            run_id: Some(run_id),
            role,
            content: content.to_owned(),
            client_request_id: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn execution_prompt_contains_prior_turns_and_latest_user_message() {
        let thread_id = ThreadId::new();
        let first_run = AgentRunId::new();
        let current_run = AgentRunId::new();
        let messages = vec![
            message(thread_id, first_run, MessageRole::User, "first question"),
            message(thread_id, first_run, MessageRole::Assistant, "first answer"),
            message(
                thread_id,
                current_run,
                MessageRole::User,
                "follow-up question",
            ),
        ];

        let prompt = build_execution_prompt(current_run, "follow-up question", &messages, "");

        assert!(prompt.contains("first question"));
        assert!(prompt.contains("first answer"));
        assert!(prompt.contains("follow-up question"));
        assert!(prompt.contains("User"));
        assert!(prompt.contains("Assistant"));
        // Single-agent product framing (tools over guessing).
        assert!(prompt.contains("Prefer tools"));
        assert!(prompt.contains("Portico"));
    }

    #[test]
    fn execution_prompt_includes_injected_context() {
        let thread_id = ThreadId::new();
        let run_id = AgentRunId::new();
        let messages = vec![message(thread_id, run_id, MessageRole::User, "how does auth work?")];
        let prompt = build_execution_prompt(
            run_id,
            "how does auth work?",
            &messages,
            "## Remembered facts\n- prefer JWT",
        );
        assert!(prompt.contains("prefer JWT"));
        assert!(prompt.contains("Injected project context"));
        assert!(prompt.contains("how does auth work?"));
    }

    #[test]
    fn execution_prompt_excludes_messages_created_for_later_runs() {
        let thread_id = ThreadId::new();
        let current_run = AgentRunId::new();
        let later_run = AgentRunId::new();
        let messages = vec![
            message(
                thread_id,
                current_run,
                MessageRole::User,
                "current question",
            ),
            message(thread_id, later_run, MessageRole::User, "future question"),
        ];

        let prompt = build_execution_prompt(current_run, "current question", &messages, "");

        assert!(prompt.contains("current question"));
        assert!(!prompt.contains("future question"));
    }

    #[test]
    fn execution_prompt_preserves_legacy_direct_submit_behavior() {
        let prompt = build_execution_prompt(AgentRunId::new(), "direct message", &[], "");
        assert_eq!(prompt, "direct message");
    }

    #[test]
    fn file_permission_failure_explains_how_to_enable_project_access() {
        let message = user_visible_run_failure(&AppError::PermissionDenied {
            reason: "tool resource is outside the workspace allowlist".to_owned(),
        });

        assert!(message.contains("Trust the project"));
        assert!(message.contains("read local files"));
    }

    #[test]
    fn shell_permission_failure_points_to_file_tools() {
        let message = user_visible_run_failure(&AppError::PermissionDenied {
            reason: "shell commands are blocked by default".to_owned(),
        });

        assert!(message.contains("Shell commands are blocked"));
        assert!(message.contains("fs_list") || message.contains("fs_read"));
    }

    #[test]
    fn path_outside_workspace_failure_is_specific() {
        let message = user_visible_run_failure(&AppError::PermissionDenied {
            reason: "tool resource is outside the owning workspace".to_owned(),
        });

        assert!(message.contains("outside this project folder"));
    }

    #[test]
    fn provider_timeout_failure_is_actionable() {
        let message = user_visible_run_failure(&AppError::Internal {
            message: "PROVIDER_TIMEOUT: model request timed out while waiting for a response (HTTP Error: request timed out)".to_owned(),
        });
        assert!(message.to_lowercase().contains("timed out"));
        assert!(!message.contains("unexpected error"));
    }
}
