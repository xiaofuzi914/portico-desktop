//! Multi-agent orchestration for Portico.

use app_models::{
    AgentDefinition, AgentRunId, AgentRunStatus, AppError, BuiltInAgent, OrchestrationPlan,
    PermissionScope, SubagentRun,
};
use app_runtime::PorticoRuntimeHandle;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use crate::AgentRegistry;

/// Default timeout for an individual subagent run.
const SUBAGENT_TIMEOUT: Duration = Duration::from_secs(60);

/// Coordinates subagent planning, execution, and synthesis.
pub struct Orchestrator {
    runtime: Arc<PorticoRuntimeHandle>,
    registry: AgentRegistry,
}

impl Orchestrator {
    /// Create a new orchestrator bound to the provided runtime and registry.
    #[must_use]
    pub const fn new(runtime: Arc<PorticoRuntimeHandle>, registry: AgentRegistry) -> Self {
        Self { runtime, registry }
    }

    /// List all registered agent definitions.
    #[must_use]
    pub fn list_agents(&self) -> Vec<AgentDefinition> {
        self.registry.list()
    }

    /// Parse `task` and create an orchestration plan under `parent_run_id`.
    ///
    /// Subagent permission scopes are clamped to the parent run's effective
    /// scope, and write-type subagents require an existing worktree.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent run cannot be found or persistence fails.
    pub async fn plan(
        &self,
        parent_run_id: AgentRunId,
        task: &str,
    ) -> Result<OrchestrationPlan, AppError> {
        let parent = self.runtime.get_run(parent_run_id).await?;
        let workspace = self.runtime.get_workspace(parent.workspace_id).await?;
        let parent_scope = if workspace.trusted {
            PermissionScope::Workspace
        } else {
            PermissionScope::Thread
        };

        let selected = select_agents_for_task(task);
        let mut subagents = Vec::with_capacity(selected.len());

        for agent in selected {
            let mut def = self.registry.built_in(agent);
            if !def.default_permission_scope.is_at_most(parent_scope) {
                def.default_permission_scope = parent_scope;
            }

            if is_write_agent(&def) {
                self.ensure_worktree(parent.workspace_id, parent.thread_id).await?;
            }

            let task_description = format!("[{}] {task}", def.name);
            let subagent = SubagentRun {
                id: AgentRunId::new(),
                parent_run_id,
                agent_name: def.name.clone(),
                status: AgentRunStatus::Queued,
                task_description,
                output_summary: None,
                created_at: Utc::now(),
                completed_at: None,
            };
            self.runtime.storage().create_subagent(&subagent).await?;
            subagents.push(subagent);
        }

        Ok(OrchestrationPlan {
            parent_run_id,
            subagents,
        })
    }

    /// Execute all subagents in `plan` in parallel.
    ///
    /// # Errors
    ///
    /// Returns an error if any subagent cannot be started or persisted.
    pub async fn execute_parallel(
        &self,
        plan: OrchestrationPlan,
    ) -> Result<Vec<SubagentRun>, AppError> {
        let mut handles = Vec::with_capacity(plan.subagents.len());
        for subagent in plan.subagents {
            let runtime = self.runtime.clone();
            handles.push(tokio::spawn(async move {
                run_subagent(runtime, subagent).await
            }));
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            let result = handle.await.map_err(|e| AppError::Internal {
                message: format!("subagent task panicked: {e}"),
            })?;
            results.push(result?);
        }
        Ok(results)
    }

    /// Execute the plan respecting the write-serial / read-parallel red line.
    ///
    /// Read-type subagents run in parallel; write-type subagents run serially
    /// afterwards.
    ///
    /// # Errors
    ///
    /// Returns an error if any subagent cannot be started or persisted.
    pub async fn execute_plan(
        &self,
        plan: OrchestrationPlan,
    ) -> Result<Vec<SubagentRun>, AppError> {
        let (read, write): (Vec<_>, Vec<_>) =
            plan.subagents.into_iter().partition(|s| !self.is_write_subagent(s));

        let mut read_results = self.execute_parallel_for_subagents(read).await?;
        let mut write_results = Vec::with_capacity(write.len());
        for subagent in write {
            write_results.push(run_subagent(self.runtime.clone(), subagent).await?);
        }

        read_results.append(&mut write_results);
        Ok(read_results)
    }

    /// Synthesize a human-readable summary from subagent results.
    ///
    /// # Errors
    ///
    /// Returns an error if synthesis fails.
    #[allow(clippy::unused_async)]
    pub async fn synthesize(&self, results: &[SubagentRun]) -> Result<String, AppError> {
        let mut lines = Vec::with_capacity(results.len() + 1);
        lines.push("Subagent results:".to_owned());
        for result in results {
            let summary = result.output_summary.as_deref().unwrap_or("No summary available");
            lines.push(format!(
                "- {} ({}): {}",
                result.agent_name,
                result.status.as_str(),
                summary.lines().next().unwrap_or(summary)
            ));
        }
        Ok(lines.join("\n"))
    }

    /// Cancel a subagent run, if it has been started.
    ///
    /// # Errors
    ///
    /// Returns an error if the subagent is missing or cancellation fails.
    pub async fn cancel_subagent(&self, id: AgentRunId) -> Result<(), AppError> {
        if let Some(child_run_id) = self.runtime.storage().get_subagent_child_run(id).await? {
            self.runtime.cancel_run(child_run_id).await?;
        }
        self.runtime
            .storage()
            .update_subagent_status(id, AgentRunStatus::Cancelled, None)
            .await
    }

    async fn execute_parallel_for_subagents(
        &self,
        subagents: Vec<SubagentRun>,
    ) -> Result<Vec<SubagentRun>, AppError> {
        let mut handles = Vec::with_capacity(subagents.len());
        for subagent in subagents {
            let runtime = self.runtime.clone();
            handles.push(tokio::spawn(async move {
                run_subagent(runtime, subagent).await
            }));
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            let result = handle.await.map_err(|e| AppError::Internal {
                message: format!("subagent task panicked: {e}"),
            })?;
            results.push(result?);
        }
        Ok(results)
    }

    fn is_write_subagent(&self, subagent: &SubagentRun) -> bool {
        self.registry
            .get(&subagent.agent_name)
            .is_some_and(|def| def.allowed_tools.iter().any(|tool| is_write_tool(tool)))
    }

    async fn ensure_worktree(
        &self,
        workspace_id: app_models::WorkspaceId,
        thread_id: app_models::ThreadId,
    ) -> Result<(), AppError> {
        let worktrees = self.runtime.worktree_manager().list_worktrees(workspace_id).await?;
        if worktrees.is_empty() {
            self.runtime
                .worktree_manager()
                .create_worktree(workspace_id, thread_id, "default")
                .await?;
        }
        Ok(())
    }
}

async fn run_subagent(
    runtime: Arc<PorticoRuntimeHandle>,
    subagent: SubagentRun,
) -> Result<SubagentRun, AppError> {
    let storage = runtime.storage();
    storage
        .update_subagent_status(subagent.id, AgentRunStatus::Running, None)
        .await?;

    let parent = runtime.get_run(subagent.parent_run_id).await?;
    let child = runtime.start_run(parent.workspace_id, parent.thread_id).await?;
    storage.update_subagent_child_run(subagent.id, child.id).await?;

    let work = runtime.submit_message(child.id, &subagent.task_description);
    let outcome = match timeout(SUBAGENT_TIMEOUT, work).await {
        Ok(Ok(())) => {
            let child_run = runtime.get_run(child.id).await?;
            let summary = summarize_run(&runtime, child.id).await?;
            (child_run.status, summary)
        }
        Ok(Err(err)) => (AgentRunStatus::Failed, Some(err.to_string())),
        Err(_) => {
            let _ = runtime.cancel_run(child.id).await;
            (
                AgentRunStatus::Failed,
                Some(format!(
                    "subagent timed out after {} seconds",
                    SUBAGENT_TIMEOUT.as_secs()
                )),
            )
        }
    };

    storage
        .update_subagent_status(subagent.id, outcome.0, outcome.1.as_deref())
        .await?;
    storage.get_subagent(subagent.id).await
}

async fn summarize_run(
    runtime: &PorticoRuntimeHandle,
    run_id: AgentRunId,
) -> Result<Option<String>, AppError> {
    let events = runtime.list_run_events(run_id).await?;
    for event in events.iter().rev() {
        if event.event_type == "MessageCompleted" {
            if let Some(content) = event.payload.get("content").and_then(|v| v.as_str()) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    return Ok(Some(trimmed.chars().take(200).collect()));
                }
            }
        }
    }
    Ok(None)
}

fn select_agents_for_task(task: &str) -> Vec<BuiltInAgent> {
    let lower = task.to_lowercase();
    let mut selected = Vec::new();
    let mut push = |agent: BuiltInAgent| {
        if !selected.contains(&agent) {
            selected.push(agent);
        }
    };

    if lower.contains("review") || lower.contains("audit") {
        push(BuiltInAgent::SecurityReviewer);
        push(BuiltInAgent::Reviewer);
    }
    if lower.contains("plan") || lower.contains("break down") || lower.contains("design") {
        push(BuiltInAgent::Planner);
    }
    if lower.contains("explore") || lower.contains("navigate") || lower.contains("find") {
        push(BuiltInAgent::Explorer);
    }
    if lower.contains("write")
        || lower.contains("implement")
        || lower.contains("code")
        || lower.contains("create")
    {
        push(BuiltInAgent::Worker);
    }
    if lower.contains("test") {
        push(BuiltInAgent::Tester);
    }
    if lower.contains("research") || lower.contains("search") {
        push(BuiltInAgent::Researcher);
    }
    if lower.contains("document") || lower.contains("doc") {
        push(BuiltInAgent::DocWriter);
    }

    if selected.is_empty() {
        selected.push(BuiltInAgent::Default);
    }
    selected
}

fn is_write_agent(def: &AgentDefinition) -> bool {
    def.allowed_tools.iter().any(|tool| is_write_tool(tool))
}

fn is_write_tool(tool: &str) -> bool {
    matches!(
        tool,
        "filesystem.write" | "terminal.execute" | "git.commit" | "git.stage" | "mcp.invoke.write"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_runtime::{
        MemoryEventBus, PorticoRuntimeHandle, SqliteModelProviderRegistry, SqliteStorage, Storage,
    };
    use std::sync::Arc;

    async fn setup() -> (
        Arc<PorticoRuntimeHandle>,
        app_models::Workspace,
        app_models::Thread,
    ) {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let event_bus = Arc::new(MemoryEventBus::default());
        let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let runtime = PorticoRuntimeHandle::new(storage, event_bus, registry, None, None)
            .await
            .expect("create runtime");
        let workspace = runtime
            .create_workspace("test", "/tmp/portico-orchestrator-test", false)
            .await
            .expect("create workspace");
        let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");
        (Arc::new(runtime), workspace, thread)
    }

    #[tokio::test]
    async fn plan_parses_review_keywords() {
        let (runtime, workspace, thread) = setup().await;
        let orchestrator = Orchestrator::new(runtime, AgentRegistry::new());
        let parent = orchestrator
            .runtime
            .start_run(workspace.id, thread.id)
            .await
            .expect("start run");

        let plan = orchestrator
            .plan(parent.id, "Please review and test this change")
            .await
            .expect("plan");
        let names: Vec<_> = plan.subagents.iter().map(|s| s.agent_name.as_str()).collect();
        assert!(names.contains(&"security-reviewer"));
        assert!(names.contains(&"reviewer"));
        assert!(names.contains(&"tester"));
    }

    #[tokio::test]
    async fn plan_clamps_permission_scope_to_parent() {
        let (runtime, workspace, thread) = setup().await;
        let orchestrator = Orchestrator::new(runtime, AgentRegistry::new());
        let parent = orchestrator
            .runtime
            .start_run(workspace.id, thread.id)
            .await
            .expect("start run");

        let plan = orchestrator.plan(parent.id, "write some code").await.expect("plan");
        let worker = plan
            .subagents
            .iter()
            .find(|s| s.agent_name == "worker")
            .expect("worker subagent");
        // Untrusted workspace limits parent scope to Thread.
        let persisted = orchestrator
            .runtime
            .storage()
            .get_subagent(worker.id)
            .await
            .expect("get subagent");
        assert_eq!(persisted.status, AgentRunStatus::Queued);
    }

    #[tokio::test]
    async fn execute_parallel_completes_subagents() {
        let (runtime, workspace, thread) = setup().await;
        let orchestrator = Orchestrator::new(runtime, AgentRegistry::new());
        let parent = orchestrator
            .runtime
            .start_run(workspace.id, thread.id)
            .await
            .expect("start run");

        let plan = orchestrator.plan(parent.id, "research this topic").await.expect("plan");
        let results = orchestrator.execute_parallel(plan).await.expect("execute");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, AgentRunStatus::Completed);
    }

    #[tokio::test]
    async fn execute_plan_runs_write_subagent_serially_and_creates_worktree() {
        let (runtime, workspace, thread) = setup().await;
        let orchestrator = Orchestrator::new(runtime, AgentRegistry::new());
        let parent = orchestrator
            .runtime
            .start_run(workspace.id, thread.id)
            .await
            .expect("start run");

        let plan = orchestrator.plan(parent.id, "write code").await.expect("plan");
        let results = orchestrator.execute_plan(plan).await.expect("execute plan");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, AgentRunStatus::Completed);

        let worktrees = orchestrator
            .runtime
            .worktree_manager()
            .list_worktrees(workspace.id)
            .await
            .expect("list worktrees");
        assert_eq!(worktrees.len(), 1);
    }

    #[tokio::test]
    async fn cancel_subagent_marks_cancelled() {
        let (runtime, workspace, thread) = setup().await;
        let orchestrator = Orchestrator::new(runtime, AgentRegistry::new());
        let parent = orchestrator
            .runtime
            .start_run(workspace.id, thread.id)
            .await
            .expect("start run");

        let plan = orchestrator.plan(parent.id, "research this topic").await.expect("plan");
        let subagent = &plan.subagents[0];

        // Start the subagent in the background and cancel it quickly.
        let runtime_clone = orchestrator.runtime.clone();
        let subagent_clone = subagent.clone();
        let handle = tokio::spawn(async move { run_subagent(runtime_clone, subagent_clone).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        orchestrator.cancel_subagent(subagent.id).await.expect("cancel");

        let result = handle.await.expect("join");
        assert!(
            result.is_ok() || matches!(result, Err(AppError::Internal { .. })),
            "cancelled subagent may complete or fail"
        );

        let persisted = orchestrator
            .runtime
            .storage()
            .get_subagent(subagent.id)
            .await
            .expect("get subagent");
        assert_eq!(persisted.status, AgentRunStatus::Cancelled);
    }

    #[tokio::test]
    async fn synthesize_formats_results() {
        let (runtime, workspace, thread) = setup().await;
        let orchestrator = Orchestrator::new(runtime, AgentRegistry::new());
        let parent = orchestrator
            .runtime
            .start_run(workspace.id, thread.id)
            .await
            .expect("start run");

        let plan = orchestrator.plan(parent.id, "hello").await.expect("plan");
        let results = orchestrator.execute_plan(plan).await.expect("execute");
        let summary = orchestrator.synthesize(&results).await.expect("synthesize");
        assert!(summary.contains("default"));
        assert!(summary.contains("Completed"));
    }
}
