//! AutoAgents-backed implementation of [`app_runtime::AgentExecutor`].

use crate::{event_mapping::map_autoagents_event, tool_adapter::PorticoToolRegistry};
use app_models::{AgentRunId, AppError, ThreadId, WorkspaceId};
use app_runtime::{AgentExecutor, EventBus, Storage};
use async_trait::async_trait;
use autoagents_core::agent::{
    AgentBuilder, AgentDeriveT, AgentHooks, DirectAgent, DirectAgentHandle,
    prebuilt::executor::ReActAgent,
};
use autoagents_llm::LLMProvider;
use autoagents_protocol::Task;
use futures::StreamExt;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Executor that drives an `AutoAgents` [`ReActAgent`] and maps its events to
/// Portico [`RuntimeEvent`](app_runtime::RuntimeEvent)s.
#[derive(Clone)]
pub struct AutoAgentsExecutor {
    llm: Arc<dyn LLMProvider>,
    tools: Arc<PorticoToolRegistry>,
}

impl AutoAgentsExecutor {
    /// Create a new executor backed by the given LLM provider and tool registry.
    #[must_use]
    pub fn new(llm: Arc<dyn LLMProvider>, tools: Arc<PorticoToolRegistry>) -> Self {
        Self { llm, tools }
    }
}

#[async_trait]
impl AgentExecutor for AutoAgentsExecutor {
    #[allow(clippy::too_many_lines)]
    async fn execute(
        &self,
        run_id: AgentRunId,
        thread_id: ThreadId,
        workspace_id: WorkspaceId,
        message: &str,
        storage: Arc<dyn Storage>,
        event_bus: Arc<dyn EventBus>,
        token: CancellationToken,
    ) -> Result<(), AppError> {
        if token.is_cancelled() {
            return Ok(());
        }

        let agent = ReActAgent::new(PorticoAgent {
            workspace_id,
            run_id,
            tools: Arc::clone(&self.tools),
        });
        let handle: DirectAgentHandle<ReActAgent<PorticoAgent>> =
            AgentBuilder::<_, DirectAgent>::new(agent)
                .llm(self.llm.clone())
                .stream(true)
                .build()
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("failed to build AutoAgents agent: {e}"),
                })?;

        let task = Task::new(message).with_app_meta(json!({
            "run_id": run_id.0,
            "thread_id": thread_id.0,
            "workspace_id": workspace_id.0,
        }));

        // Subscribe to agent events before starting the run so no events are lost.
        let mut event_stream = handle.rx;
        let mut output_stream =
            handle.agent.run_stream(task).await.map_err(|e| AppError::Internal {
                message: format!("failed to start AutoAgents run: {e}"),
            })?;

        // Forward AutoAgents events to the Portico event bus until the run ends
        // or cancellation is requested.
        let event_bus_clone = event_bus.clone();
        let token_clone = token.clone();
        let event_task = tokio::spawn(async move {
            let mut response_buffer = String::new();
            loop {
                tokio::select! {
                    () = token_clone.cancelled() => break,
                    event = event_stream.next() => {
                        match event {
                            Some(autoagents_event) => {
                                if let Some(runtime_event) = map_autoagents_event(
                                    &autoagents_event,
                                    run_id,
                                    thread_id,
                                    workspace_id,
                                    &mut response_buffer,
                                ) {
                                    let _ = event_bus_clone.publish(runtime_event).await;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        // Drive the output stream to completion, respecting cancellation.
        let mut final_response = String::new();
        let mut stream_error: Option<AppError> = None;
        loop {
            tokio::select! {
                () = token.cancelled() => break,
                item = output_stream.next() => {
                    match item {
                        Some(Ok(text)) => {
                            final_response = text;
                        }
                        Some(Err(e)) => {
                            stream_error = Some(AppError::Internal {
                                message: format!("AutoAgents stream error: {e}"),
                            });
                            break;
                        }
                        None => break,
                    }
                }
            }
        }

        // Give the event forwarder a moment to drain remaining events, then stop it.
        tokio::time::sleep(Duration::from_millis(50)).await;
        event_task.abort();
        let _ = event_task.await;

        if let Some(err) = stream_error {
            return Err(err);
        }

        if !final_response.is_empty() {
            let _ = storage
                .append_event(
                    run_id,
                    thread_id,
                    0,
                    "message_completed",
                    json!({ "content": &final_response }),
                )
                .await;
        }

        Ok(())
    }
}

/// Minimal agent definition used by [`AutoAgentsExecutor`].
#[derive(Debug, Clone)]
struct PorticoAgent {
    workspace_id: WorkspaceId,
    run_id: AgentRunId,
    tools: Arc<PorticoToolRegistry>,
}

#[async_trait]
impl AgentDeriveT for PorticoAgent {
    type Output = String;

    fn name(&self) -> &'static str {
        "portico_agent"
    }

    fn description(&self) -> &'static str {
        "Portico product agent backed by AutoAgents"
    }

    fn output_schema(&self) -> Option<serde_json::Value> {
        None
    }

    fn tools(&self) -> Vec<Box<dyn autoagents_core::tool::ToolT>> {
        self.tools.autoagents_tools(self.workspace_id, self.run_id)
    }
}

impl AgentHooks for PorticoAgent {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock_llm::MockLlmProvider, tool_adapter::PorticoToolRegistry};
    use app_runtime::{MemoryEventBus, RuntimeEvent, SqliteStorage};
    use app_tools::{Tool as PorticoTool, ToolInput, ToolOutput};

    #[derive(Debug, Clone)]
    struct MockPorticoTool;

    #[async_trait::async_trait]
    impl PorticoTool for MockPorticoTool {
        fn name(&self) -> &'static str {
            "mock_tool"
        }

        fn description(&self) -> &'static str {
            "A mock Portico tool for tests"
        }

        fn schema(&self) -> Option<serde_json::Value> {
            Some(serde_json::json!({"type": "object"}))
        }

        async fn invoke(&self, _input: ToolInput) -> Result<ToolOutput, AppError> {
            Ok(ToolOutput {
                result: serde_json::json!({"ok": true}),
            })
        }
    }

    async fn setup() -> (
        AutoAgentsExecutor,
        Arc<SqliteStorage>,
        Arc<MemoryEventBus>,
        AgentRunId,
        ThreadId,
        WorkspaceId,
    ) {
        let llm: Arc<dyn LLMProvider> = Arc::new(MockLlmProvider::new());
        let tools = Arc::new(PorticoToolRegistry::new());
        let executor = AutoAgentsExecutor::new(llm, tools);
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        (
            executor,
            storage,
            event_bus,
            AgentRunId::new(),
            ThreadId::new(),
            WorkspaceId::new(),
        )
    }

    #[tokio::test]
    async fn executor_runs_and_emits_message_completed() {
        let (executor, storage, event_bus, run_id, thread_id, workspace_id) = setup().await;
        let mut stream = event_bus.subscribe(run_id).expect("subscribe");
        let token = CancellationToken::new();

        executor
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

        let mut found_completed = false;
        while let Ok(Some(event)) = stream.next().await {
            if matches!(event, RuntimeEvent::MessageCompleted { .. }) {
                found_completed = true;
                break;
            }
        }
        assert!(found_completed);
    }

    #[tokio::test]
    async fn executor_emits_tool_events_when_asked_for_tool() {
        let (_executor, storage, event_bus, run_id, thread_id, workspace_id) = setup().await;
        let llm: Arc<dyn LLMProvider> = Arc::new(MockLlmProvider::new());
        let tools = PorticoToolRegistry::new();
        tools.register(Arc::new(MockPorticoTool));
        let executor = AutoAgentsExecutor::new(llm, Arc::new(tools));
        let mut stream = event_bus.subscribe(run_id).expect("subscribe");
        let token = CancellationToken::new();

        executor
            .execute(
                run_id,
                thread_id,
                workspace_id,
                "please use a tool",
                storage,
                event_bus,
                token,
            )
            .await
            .expect("execute");

        let mut found_tool_requested = false;
        let mut found_tool_completed = false;
        while let Ok(Some(event)) = stream.next().await {
            match event {
                RuntimeEvent::ToolRequested { tool_name, .. } if tool_name == "mock_tool" => {
                    found_tool_requested = true;
                }
                RuntimeEvent::ToolCompleted { tool_name, .. } if tool_name == "mock_tool" => {
                    found_tool_completed = true;
                }
                RuntimeEvent::MessageCompleted { .. } => break,
                _ => {}
            }
        }
        assert!(found_tool_requested);
        assert!(found_tool_completed);
    }
}
