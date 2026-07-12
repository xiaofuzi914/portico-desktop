//! Agent executor abstraction and built-in mock executor.

use crate::{
    events::{EventBus, RuntimeEvent},
    storage::Storage,
};
use app_models::{AgentRunId, AppError, RunModelSnapshot, ThreadId, WorkspaceId};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

/// Product-level outcome of one executor pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentExecutionOutcome {
    /// The model completed the run with final assistant text.
    Completed(String),
    /// A durable tool invocation is waiting for human approval.
    WaitingApproval,
}

/// Trait implemented by agent backends that execute a single run turn.
///
/// The runtime calls the executor after transitioning the run to
/// [`AgentRunStatus::Running`]. Implementations are responsible for
/// checking cancellation, publishing runtime events, and persisting any
/// events they choose via [`Storage`].
#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait AgentExecutor: Send + Sync {
    /// Execute one agent turn/run for the given message.
    ///
    /// The executor is responsible for:
    /// - Checking `token.is_cancelled()` regularly and stopping early if cancelled.
    /// - Persisting its own events via `storage.append_event(...)` if desired.
    /// - Publishing runtime-facing events via `event_bus.publish(...)`.
    /// - Returning the final assistant text when the run completes naturally.
    async fn execute(
        &self,
        run_id: AgentRunId,
        thread_id: ThreadId,
        workspace_id: WorkspaceId,
        message: &str,
        storage: Arc<dyn Storage>,
        event_bus: Arc<dyn EventBus>,
        token: CancellationToken,
    ) -> Result<AgentExecutionOutcome, AppError>;
}

/// Resolves the executor snapshot to use for a newly-started run.
pub struct ResolvedAgentExecutor {
    /// Executor built from the selected immutable configuration.
    pub executor: Arc<dyn AgentExecutor>,
    /// Provider/model metadata that must be persisted before network execution.
    pub snapshot: RunModelSnapshot,
}

/// Resolves the executor snapshot to use for a newly-started run.
#[async_trait]
pub trait AgentExecutorResolver: Send + Sync {
    /// Build or select an executor from the latest provider configuration.
    async fn resolve(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
        run_id: AgentRunId,
    ) -> Result<ResolvedAgentExecutor, AppError>;
}

/// Mock executor that simulates a streaming response.
///
/// Used as a fallback when no real agent backend is configured.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockAgentExecutor;

impl MockAgentExecutor {
    /// Create a new mock executor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AgentExecutor for MockAgentExecutor {
    #[allow(clippy::too_many_lines)]
    async fn execute(
        &self,
        run_id: AgentRunId,
        thread_id: ThreadId,
        _workspace_id: WorkspaceId,
        message: &str,
        storage: Arc<dyn Storage>,
        event_bus: Arc<dyn EventBus>,
        token: CancellationToken,
    ) -> Result<AgentExecutionOutcome, AppError> {
        const DELTAS: &[&str] = &[
            "Thinking... ",
            "analyzing ",
            "your ",
            "message. ",
            "Here ",
            "is ",
            "a ",
            "mock ",
            "streaming ",
            "response ",
            "that ",
            "is ",
            "delivered ",
            "in ",
            "small ",
            "chunks ",
            "over ",
            "about ",
            "two ",
            "seconds.",
        ];

        for (sequence, delta) in DELTAS.iter().enumerate() {
            if token.is_cancelled() {
                break;
            }

            let payload = serde_json::json!({ "delta": *delta });
            let _ = storage
                .append_event(
                    run_id,
                    thread_id,
                    i64::try_from(sequence).unwrap_or(0),
                    "message_delta",
                    payload,
                )
                .await;

            event_bus
                .publish(RuntimeEvent::MessageDelta {
                    run_id,
                    thread_id,
                    content: (*delta).to_owned(),
                    timestamp: chrono::Utc::now(),
                })
                .await?;

            sleep(Duration::from_millis(100)).await;
        }

        if token.is_cancelled() {
            return Ok(AgentExecutionOutcome::Completed(String::new()));
        }

        let full_response = format!("Mock response to: {message}");
        let _ = storage
            .append_event(
                run_id,
                thread_id,
                i64::try_from(DELTAS.len()).unwrap_or(0),
                "message_completed",
                serde_json::json!({ "content": &full_response }),
            )
            .await;

        event_bus
            .publish(RuntimeEvent::MessageCompleted {
                run_id,
                thread_id,
                content: full_response.clone(),
                timestamp: chrono::Utc::now(),
            })
            .await?;

        Ok(AgentExecutionOutcome::Completed(full_response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::MemoryEventBus;
    use crate::storage::SqliteStorage;
    use std::time::Duration;

    async fn setup() -> (
        Arc<SqliteStorage>,
        Arc<MemoryEventBus>,
        AgentRunId,
        ThreadId,
        WorkspaceId,
    ) {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        (
            storage,
            event_bus,
            AgentRunId::new(),
            ThreadId::new(),
            WorkspaceId::new(),
        )
    }

    #[tokio::test]
    async fn mock_executor_publishes_message_delta_and_completed() {
        let (storage, event_bus, run_id, thread_id, workspace_id) = setup().await;
        let mut stream = event_bus.subscribe(run_id).expect("subscribe");
        let token = CancellationToken::new();

        MockAgentExecutor::new()
            .execute(
                run_id,
                thread_id,
                workspace_id,
                "hello",
                storage,
                event_bus,
                token,
            )
            .await
            .expect("execute");

        let mut found_delta = false;
        let mut found_completed = false;
        while let Ok(Some(event)) = stream.next().await {
            match event {
                RuntimeEvent::MessageDelta { .. } => found_delta = true,
                RuntimeEvent::MessageCompleted { content, .. } => {
                    assert!(content.contains("hello"));
                    found_completed = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(found_delta);
        assert!(found_completed);
    }

    #[tokio::test]
    async fn mock_executor_respects_cancellation() {
        let (storage, event_bus, run_id, thread_id, workspace_id) = setup().await;
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let handle = tokio::spawn(async move {
            MockAgentExecutor::new()
                .execute(
                    run_id,
                    thread_id,
                    workspace_id,
                    "hello",
                    storage,
                    event_bus,
                    token_clone,
                )
                .await
        });

        tokio::time::sleep(Duration::from_millis(150)).await;
        token.cancel();

        let result = handle.await.expect("join");
        assert!(result.is_ok());
    }
}
