//! Adapter between `AutoAgents` internals and Portico product APIs.
//!
//! This crate is the only place in the workspace that is allowed to depend on
//! `AutoAgents` internal crates. It translates product-level types into
//! `AutoAgents` primitives and back again, keeping UI-facing crates free of
//! framework-specific types such as `Arc<dyn LLMProvider>` or `Box<dyn ToolT>`.

pub mod event_mapping;
pub mod executor;
pub mod mock_llm;
pub mod mcp_bridge;
pub mod provider_factory;
pub mod tool_adapter;

pub use event_mapping::map_autoagents_event;
pub use executor::AutoAgentsExecutor;
pub use mock_llm::MockLlmProvider;
pub use mcp_bridge::McpToolAdapter;
pub use provider_factory::{build_default_executor, build_llm_provider};
pub use tool_adapter::{FilesystemToolAdapter, GitToolAdapter, PorticoToolRegistry, TerminalToolAdapter};

use app_models::{
    AgentRun, AgentRunId, AgentRunStatus, AppError, RunEvent, Thread, ThreadId, Workspace,
    WorkspaceId,
};
use app_runtime::{AgentExecutor, EventBus, EventStream, PorticoRuntime, Storage};
use app_security::PermissionEngine;
use async_trait::async_trait;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Product-facing runtime implemented on top of `AutoAgents`.
#[async_trait]
pub trait ProductAgentRuntime: Send + Sync {
    /// Start an agent run in the given workspace and thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be started.
    async fn start_run(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<AgentRunId, AppError>;
}

/// Adapter that wraps `AutoAgents` primitives and exposes [`ProductAgentRuntime`].
pub struct AutoAgentsRuntimeAdapter {
    /// Permission engine used to gate `AutoAgents` operations.
    permissions: Box<dyn PermissionEngine>,
    /// Product-level tool registry.
    #[allow(dead_code)]
    tools: Arc<PorticoToolRegistry>,
    /// Optional `AutoAgents`-backed executor. When set, callers can inject it into
    /// [`app_runtime::PorticoRuntimeHandle`] so that `submit_message` is backed by
    /// `AutoAgents` rather than the built-in mock executor.
    executor: Option<AutoAgentsExecutor>,
    /// Optional storage backend for persisting runs and events.
    storage: Option<Arc<dyn Storage>>,
    /// Optional event bus for publishing run lifecycle events.
    event_bus: Option<Arc<dyn EventBus>>,
}

impl AutoAgentsRuntimeAdapter {
    /// Create a new adapter.
    #[must_use]
    pub fn new(permissions: Box<dyn PermissionEngine>, tools: Arc<PorticoToolRegistry>) -> Self {
        Self {
            permissions,
            tools,
            executor: None,
            storage: None,
            event_bus: None,
        }
    }

    /// Attach an [`AutoAgentsExecutor`] to the adapter.
    #[must_use]
    pub fn with_executor(mut self, executor: AutoAgentsExecutor) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Attach a storage backend.
    #[must_use]
    pub fn with_storage(mut self, storage: Arc<dyn Storage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Attach an event bus.
    #[must_use]
    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Return the configured executor, if any.
    #[must_use]
    pub const fn executor(&self) -> Option<&AutoAgentsExecutor> {
        self.executor.as_ref()
    }
}

#[async_trait]
impl ProductAgentRuntime for AutoAgentsRuntimeAdapter {
    async fn start_run(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<AgentRunId, AppError> {
        let _ = &self.permissions;
        let storage = self.storage.as_ref().ok_or_else(|| AppError::Internal {
            message: "AutoAgents adapter storage not configured".to_owned(),
        })?;
        let event_bus = self.event_bus.as_ref().ok_or_else(|| AppError::Internal {
            message: "AutoAgents adapter event bus not configured".to_owned(),
        })?;
        let executor = self.executor.as_ref().ok_or_else(|| AppError::Internal {
            message: "AutoAgents adapter executor not configured".to_owned(),
        })?;

        let run = storage.create_run(workspace_id, thread_id).await?;
        storage.update_run_status(run.id, AgentRunStatus::Running).await?;

        let run_id = run.id;
        let executor = executor.clone();
        let storage = Arc::clone(storage);
        let event_bus = Arc::clone(event_bus);
        let token = CancellationToken::new();

        tokio::spawn(async move {
            let _ = executor
                .execute(
                    run_id,
                    thread_id,
                    workspace_id,
                    "Autostart",
                    storage,
                    event_bus,
                    token,
                )
                .await;
        });

        Ok(run_id)
    }
}

#[async_trait]
impl PorticoRuntime for AutoAgentsRuntimeAdapter {
    async fn create_workspace(
        &self,
        _name: &str,
        _root_path: &str,
        _trusted: bool,
    ) -> Result<Workspace, AppError> {
        Err(AppError::Internal {
            message: "AutoAgents adapter does not manage workspaces".to_owned(),
        })
    }

    async fn list_workspaces(&self) -> Result<Vec<Workspace>, AppError> {
        Err(AppError::Internal {
            message: "AutoAgents adapter does not manage workspaces".to_owned(),
        })
    }

    async fn create_thread(
        &self,
        _workspace_id: WorkspaceId,
        _title: &str,
    ) -> Result<Thread, AppError> {
        Err(AppError::Internal {
            message: "AutoAgents adapter does not manage threads".to_owned(),
        })
    }

    async fn list_threads(&self, _workspace_id: WorkspaceId) -> Result<Vec<Thread>, AppError> {
        Err(AppError::Internal {
            message: "AutoAgents adapter does not manage threads".to_owned(),
        })
    }

    async fn start_run(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<AgentRun, AppError> {
        let run_id = ProductAgentRuntime::start_run(self, workspace_id, thread_id).await?;
        let storage = self.storage.as_ref().ok_or_else(|| AppError::Internal {
            message: "AutoAgents adapter storage not configured".to_owned(),
        })?;
        storage.get_run(run_id).await
    }

    async fn submit_message(&self, run_id: AgentRunId, content: &str) -> Result<(), AppError> {
        let storage = self.storage.as_ref().ok_or_else(|| AppError::Internal {
            message: "AutoAgents adapter storage not configured".to_owned(),
        })?;
        let event_bus = self.event_bus.as_ref().ok_or_else(|| AppError::Internal {
            message: "AutoAgents adapter event bus not configured".to_owned(),
        })?;
        let executor = self.executor.as_ref().ok_or_else(|| AppError::Internal {
            message: "AutoAgents adapter executor not configured".to_owned(),
        })?;

        let run = storage.get_run(run_id).await?;
        executor
            .execute(
                run_id,
                run.thread_id,
                run.workspace_id,
                content,
                Arc::clone(storage),
                Arc::clone(event_bus),
                CancellationToken::new(),
            )
            .await
    }

    async fn cancel_run(&self, _run_id: AgentRunId) -> Result<(), AppError> {
        Err(AppError::Internal {
            message: "AutoAgents adapter does not support cancellation".to_owned(),
        })
    }

    async fn pause_run(&self, _run_id: AgentRunId) -> Result<(), AppError> {
        Err(AppError::Internal {
            message: "AutoAgents adapter does not support pause".to_owned(),
        })
    }

    async fn resume_run(&self, _run_id: AgentRunId) -> Result<(), AppError> {
        Err(AppError::Internal {
            message: "AutoAgents adapter does not support resume".to_owned(),
        })
    }

    async fn list_run_events(&self, _run_id: AgentRunId) -> Result<Vec<RunEvent>, AppError> {
        Err(AppError::Internal {
            message: "AutoAgents adapter does not persist events".to_owned(),
        })
    }

    fn subscribe_events(&self, _run_id: AgentRunId) -> Result<Box<dyn EventStream>, AppError> {
        Err(AppError::NotFound {
            resource: "event stream".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_llm::MockLlmProvider;
    use app_runtime::{MemoryEventBus, SqliteStorage};

    struct DummyPermissions;

    impl PermissionEngine for DummyPermissions {
        fn evaluate(
            &self,
            _request: app_security::PermissionRequest,
        ) -> app_security::PermissionResult {
            app_security::PermissionResult::Allowed
        }
    }

    #[tokio::test]
    async fn adapter_starts_run() {
        use autoagents_llm::LLMProvider;

        let storage: Arc<dyn Storage> =
            Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus: Arc<dyn EventBus> = Arc::new(MemoryEventBus::default());
        let tools = Arc::new(PorticoToolRegistry::new());
        let llm: Arc<dyn LLMProvider> = Arc::new(MockLlmProvider::new());
        let executor = AutoAgentsExecutor::new(llm, Arc::clone(&tools));

        let adapter = AutoAgentsRuntimeAdapter::new(Box::new(DummyPermissions), tools)
            .with_storage(Arc::clone(&storage))
            .with_event_bus(Arc::clone(&event_bus))
            .with_executor(executor);

        let workspace = storage
            .create_workspace("test", "/tmp/test", false)
            .await
            .expect("create workspace");
        let thread = storage.create_thread(workspace.id, "thread").await.expect("create thread");

        let run_id = ProductAgentRuntime::start_run(&adapter, workspace.id, thread.id)
            .await
            .expect("should start run");
        assert_ne!(run_id, AgentRunId::new());

        // Give the spawned executor a moment to persist events.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let run = storage.get_run(run_id).await.expect("run exists");
        assert_eq!(run.status, AgentRunStatus::Running);
    }
}
