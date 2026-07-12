//! Runtime and event bus abstractions for Portico.

pub mod approval_broker;
pub mod audit_logger;
pub mod context;
pub mod events;
pub mod executor;
pub mod notification_center;
pub mod provider_registry;
pub mod repo_root_provider;
pub mod runner;
pub mod safe_tools;
pub mod storage;
pub mod task_queue;
pub mod tool_execution;
pub mod workspace;
pub mod worktree;

// Re-export memory/embedding abstractions used by the Tauri bridge.
pub use app_memory::{
    EmbeddingProvider, HashEmbeddingProvider, OllamaEmbeddingProvider,
    OpenAiCompatEmbeddingProvider, SqliteRagStore,
};

// Re-export product types from app-models so consumers only depend on app-runtime.
pub use app_models::{
    ActiveModelSelection, AgentDefinition, AgentRun, AgentRunId, AgentRunStatus, AppError,
    ApprovalRequest, ApprovalRequestId, ApprovalRequestStatus, Artifact, Automation, AutomationId,
    AutomationTrigger, BackgroundTask, BackgroundTaskId, BackgroundTaskStatus, BuiltInAgent,
    ContextSummary, ExecutionContext, FeatureCapabilities, InstructionFile, McpServerConfig,
    McpTransport, MemoryId, MemoryItem, MemoryScope, Message, MessageId, MessageRole,
    ModelCapability, ModelId, ModelInfo, ModelSelectionScope, Notification, NotificationCategory,
    NotificationId, Orchestration, OrchestrationId, OrchestrationPlan, OrchestrationStatus,
    PermissionScope, PluginId, PluginManifest, PluginPermissions, ProviderConfig, ProviderHealth,
    ProviderHealthStatus, ProviderId, ProviderKind, RagChunk, RetryPolicy, RunEvent,
    RunModelSnapshot, Skill, SkillId, SubagentRun, TaskKind, Thread, ThreadId, ToolInvocation,
    ToolInvocationId, ToolInvocationStatus, UsageBudget, UsageRecord, WorkflowPatternId, Workspace,
    WorkspaceId, Worktree, WorktreeId,
};

// Re-export runtime modules.
pub use app_plugins::{McpClientManager, PluginRegistry, SqlitePluginRegistry};
pub use app_security::SecurityContext;
pub use approval_broker::{ApprovalBroker, ApprovalExecutionOutcome, ExecutionGrant};
pub use audit_logger::SqliteAuditLogger;
pub use context::ContextInspector;
pub use events::{EventBus, EventStream, MemoryEventBus, RuntimeEvent};
pub use executor::{
    AgentExecutionOutcome, AgentExecutor, AgentExecutorResolver, MockAgentExecutor,
    ResolvedAgentExecutor,
};
pub use notification_center::NotificationCenter;
pub use provider_registry::{ModelProviderRegistry, SqliteModelProviderRegistry};
pub use repo_root_provider::StorageRepoRootProvider;
pub use runner::PorticoRuntimeHandle;
pub use safe_tools::{SafeToolExecutor, ToolReconciliation};
pub use storage::{AgentCheckpoint, AuditLogEntry, SqliteStorage, Storage};
pub use task_queue::BackgroundTaskQueue;
pub use tool_execution::{PolicyGate, PreparedToolRequest, ToolGateOutcome};
pub use workspace::WorkspaceManager;
pub use worktree::WorktreeManager;

use app_security::PermissionEngine;
use async_trait::async_trait;

/// Product-level runtime trait used by plugins and workflows.
///
/// The canonical implementation is [`PorticoRuntimeHandle`]; this trait remains
/// as a narrow boundary so downstream crates can accept a runtime without
/// depending on its concrete type.
#[async_trait]
pub trait PorticoRuntime: Send + Sync {
    /// Create a new workspace.
    async fn create_workspace(
        &self,
        name: &str,
        root_path: &str,
        trusted: bool,
    ) -> Result<Workspace, AppError>;

    /// List all workspaces.
    async fn list_workspaces(&self) -> Result<Vec<Workspace>, AppError>;

    /// Create a new thread inside a workspace.
    async fn create_thread(
        &self,
        workspace_id: WorkspaceId,
        title: &str,
    ) -> Result<Thread, AppError>;

    /// List threads in a workspace.
    async fn list_threads(&self, workspace_id: WorkspaceId) -> Result<Vec<Thread>, AppError>;

    /// Delete a thread and its conversation history.
    async fn delete_thread(&self, workspace_id: WorkspaceId, id: ThreadId) -> Result<(), AppError>;

    /// Start a new agent run.
    async fn start_run(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<AgentRun, AppError>;

    /// Submit a user message to a run.
    async fn submit_message(&self, run_id: AgentRunId, content: &str) -> Result<(), AppError>;

    /// Cancel a run.
    async fn cancel_run(&self, run_id: AgentRunId) -> Result<(), AppError>;

    /// Pause a run.
    async fn pause_run(&self, run_id: AgentRunId) -> Result<(), AppError>;

    /// Resume a paused run.
    async fn resume_run(&self, run_id: AgentRunId) -> Result<(), AppError>;

    /// List persisted events for a run.
    async fn list_run_events(&self, run_id: AgentRunId) -> Result<Vec<RunEvent>, AppError>;

    /// Subscribe to runtime events for a specific run.
    ///
    /// # Errors
    ///
    /// Returns an error if the event stream cannot be created.
    fn subscribe_events(&self, run_id: AgentRunId) -> Result<Box<dyn EventStream>, AppError>;
}

/// Builder-like handle exposing runtime configuration hooks.
pub struct RuntimeContext {
    /// Permission engine used for runtime decisions.
    pub permissions: Box<dyn PermissionEngine>,
}

impl RuntimeContext {
    /// Create a new runtime context.
    #[must_use]
    pub const fn new(permissions: Box<dyn PermissionEngine>) -> Self {
        Self { permissions }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct DummyEngine;

    fn ensure_test_directory(path: &str) {
        std::fs::create_dir_all(path).expect("create test workspace directory");
    }

    impl PermissionEngine for DummyEngine {
        fn evaluate(
            &self,
            _request: app_security::PermissionRequest,
        ) -> app_security::PermissionResult {
            app_security::PermissionResult::Allowed
        }
    }

    #[test]
    fn runtime_context_holds_engine() {
        let ctx = RuntimeContext::new(Box::new(DummyEngine));
        assert!(matches!(
            ctx.permissions.evaluate(app_security::PermissionRequest {
                workspace_id: WorkspaceId::new(),
                thread_id: None,
                run_id: None,
                action: "test".to_owned(),
                resource: "resource".to_owned(),
                trusted_workspace: false,
            }),
            app_security::PermissionResult::Allowed
        ));
    }

    async fn setup() -> (PorticoRuntimeHandle, Workspace, Thread) {
        ensure_test_directory("/tmp/test");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let runtime = PorticoRuntimeHandle::new(
            storage,
            event_bus,
            registry,
            Some(Arc::new(MockAgentExecutor)),
            None,
        )
        .await
        .expect("create runtime");
        let workspace = runtime
            .create_workspace("test", "/tmp/test", false)
            .await
            .expect("create workspace");
        let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");
        (runtime, workspace, thread)
    }

    #[tokio::test]
    async fn storage_crud_works() {
        let (runtime, workspace, thread) = setup().await;

        let workspaces = runtime.list_workspaces().await.expect("list workspaces");
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0].name, "test");

        let fetched_workspace = runtime.get_workspace(workspace.id).await.expect("get workspace");
        assert_eq!(fetched_workspace.id, workspace.id);

        let threads = runtime.list_threads(workspace.id).await.expect("list threads");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, thread.id);

        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");
        assert_eq!(run.workspace_id, workspace.id);
        assert_eq!(run.thread_id, thread.id);
        assert_eq!(run.status, AgentRunStatus::Running);

        let fetched_run = runtime.get_run(run.id).await.expect("get run");
        assert_eq!(fetched_run.id, run.id);

        let events = runtime.list_run_events(run.id).await.expect("list events");
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn send_message_is_idempotent_and_persists_the_user_turn() {
        let (runtime, workspace, thread) = setup().await;
        let first = runtime
            .send_message(thread.id, "hello", Some("request-1"))
            .await
            .expect("first send");
        let second = runtime
            .send_message(thread.id, "hello", Some("request-1"))
            .await
            .expect("idempotent retry");

        assert_eq!(first.id, second.id);
        let messages = runtime.list_messages(thread.id).await.expect("list messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[0].content, "hello");
        assert_eq!(messages[0].run_id, Some(first.id));
        assert_eq!(messages[0].client_request_id.as_deref(), Some("request-1"));

        let runs = runtime.list_runs(thread.id).await.expect("list runs");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].workspace_id, workspace.id);
    }

    #[tokio::test]
    async fn send_message_validates_content_and_idempotency_key() {
        let (runtime, _workspace, thread) = setup().await;

        assert!(runtime.send_message(thread.id, "   ", Some("request-1")).await.is_err());
        assert!(
            runtime
                .send_message(thread.id, &"x".repeat(32_001), Some("request-2"))
                .await
                .is_err()
        );
        assert!(runtime.send_message(thread.id, "hello", Some("bad key!")).await.is_err());

        assert!(runtime.list_messages(thread.id).await.expect("messages").is_empty());
        assert!(runtime.list_runs(thread.id).await.expect("runs").is_empty());
    }

    #[tokio::test]
    async fn send_message_rejects_a_second_active_turn() {
        let (runtime, _workspace, thread) = setup().await;
        let first = runtime
            .send_message(thread.id, "first", Some("active-1"))
            .await
            .expect("first send");

        let duplicate = runtime
            .send_message(thread.id, "first", Some("active-1"))
            .await
            .expect("idempotent retry");
        assert_eq!(duplicate.id, first.id);

        let second = runtime.send_message(thread.id, "second", Some("active-2")).await;
        assert!(
            matches!(second, Err(AppError::Internal { ref message }) if message.contains("RUN_ALREADY_ACTIVE"))
        );
    }

    #[tokio::test]
    async fn create_run_rejects_a_thread_from_another_workspace() {
        let (runtime, workspace, _thread) = setup().await;
        ensure_test_directory("/tmp/other");
        let other = runtime
            .create_workspace("other", "/tmp/other", false)
            .await
            .expect("create other workspace");
        let other_thread =
            runtime.create_thread(other.id, "other").await.expect("create other thread");

        let err = runtime
            .start_run(workspace.id, other_thread.id)
            .await
            .expect_err("cross-workspace run must fail");
        assert!(matches!(err, AppError::PermissionDenied { .. }));
    }

    #[tokio::test]
    async fn submit_message_streams_and_completes() {
        let (runtime, workspace, thread) = setup().await;
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        let mut stream = runtime.subscribe_events(run.id).expect("subscribe");

        runtime.submit_message(run.id, "hello").await.expect("submit message");

        let completed = runtime.get_run(run.id).await.expect("get run");
        assert_eq!(completed.status, AgentRunStatus::Completed);

        let persisted_events = runtime.list_run_events(run.id).await.expect("list events");
        assert!(!persisted_events.is_empty());

        let messages = runtime.list_messages(thread.id).await.expect("list messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::Assistant);
        assert_eq!(messages[0].run_id, Some(run.id));

        // Consume at least the MessageCompleted event from the bus.
        let mut found_completion = false;
        while let Ok(Some(event)) = stream.next().await {
            if matches!(event, RuntimeEvent::MessageCompleted { .. }) {
                found_completion = true;
                break;
            }
        }
        assert!(found_completion);
    }

    #[tokio::test]
    async fn submit_message_without_executor_fails_without_mock_response() {
        ensure_test_directory("/tmp/test");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let runtime = PorticoRuntimeHandle::new(storage, event_bus, registry, None, None)
            .await
            .expect("create runtime");
        let workspace = runtime
            .create_workspace("test", "/tmp/test", false)
            .await
            .expect("create workspace");
        let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        let err = runtime
            .submit_message(run.id, "hello")
            .await
            .expect_err("a real provider is required");

        assert!(err.to_string().contains("PROVIDER_UNAVAILABLE"));
        let messages = runtime.list_messages(thread.id).await.expect("list messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::System);
        assert!(messages[0].content.contains("No model provider"));
        assert!(runtime.list_run_events(run.id).await.expect("list events").is_empty());
    }

    #[tokio::test]
    async fn cancel_run_stops_streaming() {
        let (runtime, workspace, thread) = setup().await;
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        // Start streaming in the background and cancel it quickly.
        let runtime_clone = runtime.clone();
        let run_id = run.id;
        let handle =
            tokio::spawn(async move { runtime_clone.submit_message(run_id, "hello").await });

        // Give the task a moment to start, then cancel.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        runtime.cancel_run(run.id).await.expect("cancel run");

        // Cancellation should stop the work without an error.
        let result = handle.await.expect("join handle");
        assert!(
            result.is_ok(),
            "cancelled run should complete without error"
        );

        let cancelled = runtime.get_run(run.id).await.expect("get run");
        assert_eq!(cancelled.status, AgentRunStatus::Cancelled);
    }

    #[tokio::test]
    async fn pause_and_resume_run_updates_status() {
        let (runtime, workspace, thread) = setup().await;
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        runtime.pause_run(run.id).await.expect("pause");
        let paused = runtime.get_run(run.id).await.expect("get run");
        assert_eq!(paused.status, AgentRunStatus::Paused);

        runtime.resume_run(run.id).await.expect("resume");
        let resumed = runtime.get_run(run.id).await.expect("get run");
        assert_eq!(resumed.status, AgentRunStatus::Running);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn runtime_evaluates_permissions_and_audits() {
        ensure_test_directory("/tmp/test");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let audit = Arc::new(app_security::MemoryAuditLogger::new());
        let security = Arc::new(SecurityContext::new(
            Arc::new(app_security::PolicyPermissionEngine::default_rules()),
            Arc::new(app_security::DefaultCommandPolicy::new()),
            Arc::new(app_security::DefaultNetworkPolicy::new()),
            audit.clone(),
        ));
        let runtime = PorticoRuntimeHandle::new(storage, event_bus, registry, None, None)
            .await
            .expect("create runtime")
            .with_security(security);

        let workspace = runtime
            .create_workspace("test", "/tmp/test", true)
            .await
            .expect("create workspace");
        let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        let result = runtime.evaluate_permission(app_security::PermissionRequest {
            workspace_id: workspace.id,
            thread_id: Some(thread.id),
            run_id: Some(run.id),
            action: "filesystem.read".to_owned(),
            resource: "/tmp/foo".to_owned(),
            trusted_workspace: true,
        });
        assert!(matches!(result, app_security::PermissionResult::Allowed));

        runtime
            .audit(app_security::AuditEvent {
                workspace_id: workspace.id,
                thread_id: Some(thread.id),
                run_id: Some(run.id),
                action: "test.action".to_owned(),
                resource: "resource".to_owned(),
                outcome: app_security::PermissionResult::Allowed,
            })
            .expect("audit");

        let events = audit.events().expect("read events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "test.action");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn submit_message_emits_audit_events() {
        ensure_test_directory("/tmp/test");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let audit = Arc::new(app_security::MemoryAuditLogger::new());
        let security = Arc::new(SecurityContext::new(
            Arc::new(app_security::PolicyPermissionEngine::default_rules()),
            Arc::new(app_security::DefaultCommandPolicy::new()),
            Arc::new(app_security::DefaultNetworkPolicy::new()),
            audit.clone(),
        ));
        let runtime = PorticoRuntimeHandle::new(
            storage,
            event_bus,
            registry,
            Some(Arc::new(MockAgentExecutor)),
            None,
        )
        .await
        .expect("create runtime")
        .with_security(security);

        let workspace = runtime
            .create_workspace("test", "/tmp/test", true)
            .await
            .expect("create workspace");
        runtime
            .workspace_manager()
            .trust_workspace(workspace.id, true)
            .await
            .expect("trust workspace");
        let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        runtime.submit_message(run.id, "hello").await.expect("submit message");

        let events = audit.events().expect("read events");
        let actions: Vec<_> = events.iter().map(|e| e.action.as_str()).collect();
        assert!(actions.contains(&"run.start"));
        assert!(actions.contains(&"run.complete"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn evaluate_permission_for_run_creates_approval_request() {
        ensure_test_directory("/tmp/test");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let security = Arc::new(SecurityContext::new(
            Arc::new(app_security::PolicyPermissionEngine::default_rules()),
            Arc::new(app_security::DefaultCommandPolicy::new()),
            Arc::new(app_security::DefaultNetworkPolicy::new()),
            Arc::new(app_security::MemoryAuditLogger::new()),
        ));
        let runtime = PorticoRuntimeHandle::new(storage, event_bus, registry, None, None)
            .await
            .expect("create runtime")
            .with_security(security);

        let workspace = runtime
            .create_workspace("test", "/tmp/test", true)
            .await
            .expect("create workspace");
        runtime
            .workspace_manager()
            .trust_workspace(workspace.id, true)
            .await
            .expect("trust workspace");
        let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        let result = runtime
            .evaluate_permission_for_run(app_security::PermissionRequest {
                workspace_id: workspace.id,
                thread_id: Some(thread.id),
                run_id: Some(run.id),
                action: "mcp.invoke.write".to_owned(),
                resource: "test.write".to_owned(),
                trusted_workspace: true,
            })
            .await
            .expect("evaluate permission");

        assert!(
            matches!(result, app_security::PermissionResult::Ask { .. }),
            "expected Ask result, got {result:?}"
        );

        let run = runtime.get_run(run.id).await.expect("get run");
        assert_eq!(run.status, AgentRunStatus::WaitingApproval);

        let pending = runtime
            .list_pending_approval_requests(Some(run.id))
            .await
            .expect("list pending");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].status, ApprovalRequestStatus::Pending);
    }

    #[tokio::test]
    async fn evaluate_permission_for_run_uses_persisted_ownership_and_trust() {
        let (runtime, workspace, thread) = setup().await;
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        let forged_trust = runtime
            .evaluate_permission_for_run(app_security::PermissionRequest {
                workspace_id: workspace.id,
                thread_id: Some(thread.id),
                run_id: Some(run.id),
                action: "filesystem.read".to_owned(),
                resource: "/tmp/test/file.txt".to_owned(),
                trusted_workspace: true,
            })
            .await
            .expect("evaluate forged trust");
        assert!(matches!(
            forged_trust,
            app_security::PermissionResult::Denied { .. }
        ));

        let other_workspace_id = WorkspaceId::new();
        let forged_owner = runtime
            .evaluate_permission_for_run(app_security::PermissionRequest {
                workspace_id: other_workspace_id,
                thread_id: Some(thread.id),
                run_id: Some(run.id),
                action: "filesystem.write".to_owned(),
                resource: "/tmp/test/file.txt".to_owned(),
                trusted_workspace: true,
            })
            .await;
        assert!(matches!(
            forged_owner,
            Err(AppError::PermissionDenied { .. })
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn approve_request_resumes_run() {
        ensure_test_directory("/tmp/test");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let security = Arc::new(SecurityContext::new(
            Arc::new(app_security::PolicyPermissionEngine::default_rules()),
            Arc::new(app_security::DefaultCommandPolicy::new()),
            Arc::new(app_security::DefaultNetworkPolicy::new()),
            Arc::new(app_security::MemoryAuditLogger::new()),
        ));
        let runtime = PorticoRuntimeHandle::new(storage, event_bus, registry, None, None)
            .await
            .expect("create runtime")
            .with_security(security);

        let workspace = runtime
            .create_workspace("test", "/tmp/test", true)
            .await
            .expect("create workspace");
        runtime
            .workspace_manager()
            .trust_workspace(workspace.id, true)
            .await
            .expect("trust workspace");
        let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        let result = runtime
            .evaluate_permission_for_run(app_security::PermissionRequest {
                workspace_id: workspace.id,
                thread_id: Some(thread.id),
                run_id: Some(run.id),
                action: "mcp.invoke.write".to_owned(),
                resource: "test.write".to_owned(),
                trusted_workspace: true,
            })
            .await
            .expect("evaluate permission");

        let app_security::PermissionResult::Ask { request: approval } = result else {
            panic!("expected Ask result, got {result:?}");
        };

        runtime.approve_request(approval.id).await.expect("approve");

        let run = runtime.get_run(run.id).await.expect("get run");
        assert_eq!(run.status, AgentRunStatus::Running);

        let approval = runtime.get_approval_request(approval.id).await.expect("get approval");
        assert_eq!(approval.status, ApprovalRequestStatus::Approved);
    }

    #[tokio::test]
    async fn cancel_waiting_run_closes_pending_approvals() {
        let (runtime, workspace, thread) = setup().await;
        runtime
            .workspace_manager()
            .trust_workspace(workspace.id, true)
            .await
            .expect("trust workspace");
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");
        let result = runtime
            .evaluate_permission_for_run(app_security::PermissionRequest {
                workspace_id: workspace.id,
                thread_id: Some(thread.id),
                run_id: Some(run.id),
                action: "mcp.invoke.write".to_owned(),
                resource: "test.write".to_owned(),
                trusted_workspace: true,
            })
            .await
            .expect("evaluate permission");
        let app_security::PermissionResult::Ask { request } = result else {
            panic!("expected approval request");
        };

        runtime.cancel_run(run.id).await.expect("cancel run");

        assert_eq!(
            runtime.get_run(run.id).await.expect("run").status,
            AgentRunStatus::Cancelled
        );
        let approval = runtime.get_approval_request(request.id).await.expect("approval");
        assert_eq!(approval.status, ApprovalRequestStatus::Denied);
        assert_eq!(approval.resolution_reason.as_deref(), Some("run cancelled"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn deny_request_fails_run() {
        ensure_test_directory("/tmp/test");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let security = Arc::new(SecurityContext::new(
            Arc::new(app_security::PolicyPermissionEngine::default_rules()),
            Arc::new(app_security::DefaultCommandPolicy::new()),
            Arc::new(app_security::DefaultNetworkPolicy::new()),
            Arc::new(app_security::MemoryAuditLogger::new()),
        ));
        let runtime = PorticoRuntimeHandle::new(storage, event_bus, registry, None, None)
            .await
            .expect("create runtime")
            .with_security(security);

        let workspace = runtime
            .create_workspace("test", "/tmp/test", true)
            .await
            .expect("create workspace");
        runtime
            .workspace_manager()
            .trust_workspace(workspace.id, true)
            .await
            .expect("trust workspace");
        let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");
        let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

        let result = runtime
            .evaluate_permission_for_run(app_security::PermissionRequest {
                workspace_id: workspace.id,
                thread_id: Some(thread.id),
                run_id: Some(run.id),
                action: "mcp.invoke.write".to_owned(),
                resource: "test.write".to_owned(),
                trusted_workspace: true,
            })
            .await
            .expect("evaluate permission");

        let app_security::PermissionResult::Ask { request: approval } = result else {
            panic!("expected Ask result, got {result:?}");
        };

        runtime
            .deny_request(approval.id, Some("user denied".to_owned()))
            .await
            .expect("deny");

        let run = runtime.get_run(run.id).await.expect("get run");
        assert_eq!(run.status, AgentRunStatus::Failed);

        let approval = runtime.get_approval_request(approval.id).await.expect("get approval");
        assert_eq!(approval.status, ApprovalRequestStatus::Denied);
        assert_eq!(approval.resolution_reason.as_deref(), Some("user denied"));
    }
}
