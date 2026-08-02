//! Workflow and orchestration abstractions for Portico.
//!
//! **Product note:** default UX is single-agent chat + tools
//! (`docs/AGENT-PRODUCT-PATH.md`). Multi-agent orchestration here is a
//! **production** path for real multi-role tasks (composer secondary action),
//! not a peer dual-mode UI.

pub mod agent_registry;
pub mod canvas_extract;
pub mod canvas_goal;
pub mod canvas_layout;
pub mod execution_spec;
pub mod memory_plan;
pub mod orchestration_service;
pub mod orchestrator;
pub mod ports;
pub mod progress;
pub mod scheduler;
pub mod stage_graph;

pub use agent_registry::AgentRegistry;
pub use execution_spec::{
    deepseek_model_candidates, moonshot_model_candidates, needs_worktree, openai_model_candidates,
    resolve_thinking, spec_for_agent, spec_for_role_name, spec_from_definition,
    thinking_prompt_directive,
};
pub use canvas_extract::{
    ExtractedInsight, ExtractedThreadCluster, SessionCard, branch_title_from_focus,
    build_branch_context_seed_with_focus, extract_session_cards,
    extract_thread_clusters, summarize_session_messages,
};
pub use canvas_goal::{
    DecomposedStage, StageLaunchMode, compose_stage_launch_prompt, decompose_goal_template,
};
pub use canvas_layout::{
    LayoutPos, column_pitch, goal_column_x, goal_column_x_after_conversation,
    goal_column_x_for_session, layout_cluster_forest, layout_goal_spine, layout_project_forest,
    layout_session_narrative, layout_session_tree, next_cluster_column, project_forest_right_x,
    row_pitch, session_narrative_right_x, session_tree_right_x,
};
pub use orchestration_service::OrchestrationService;
pub use orchestrator::Orchestrator;
pub use progress::build_orchestration_progress;
pub use ports::{
    NoopPatternSink, NoopPatternSource, PatternRecallQuery, PatternSink, PatternSource,
};
pub use scheduler::AutomationScheduler;
pub use stage_graph::{
    BundledWorkflowMeta, apply_stage_edit, assemble_reduce_upstream, clamp_loop_max_iterations,
    coalesce_orchestration_status, collect_upstream_payloads, expand_foreach_tasks,
    extract_foreach_items, list_bundled_workflows, loop_should_stop, merge_foreach_outputs,
    merge_status_for_store, plan_adaptive_stage_graph, plan_bundled_workflow, plan_from_stages,
    plan_has_stages, primary_upstream_control_payload, render_stage_prompt, topological_stage_order,
    validate_stage_dag,
};

use app_models::{AgentRunId, AppError, ThreadId, WorkspaceId};
use app_runtime::PorticoRuntime;
use app_security::PermissionEngine;
use async_trait::async_trait;

/// A declarative workflow that can be executed by the runtime.
#[async_trait]
pub trait Workflow: Send + Sync {
    /// Unique name of the workflow.
    fn name(&self) -> &str;

    /// Execute the workflow within the provided runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if the workflow fails to execute.
    async fn execute(&self, runtime: &dyn PorticoRuntime) -> Result<WorkflowResult, AppError>;
}

/// Outcome of a workflow execution.
#[derive(Debug, Clone)]
pub struct WorkflowResult {
    /// Identifier of the run that executed the workflow.
    pub run_id: AgentRunId,
    /// Whether the workflow completed successfully.
    pub success: bool,
}

/// Builder for constructing permission-aware workflows.
pub trait WorkflowBuilder: Send + Sync {
    /// Create a new workflow instance scoped to a workspace and thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the workflow cannot be built.
    fn build(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
        permissions: &dyn PermissionEngine,
    ) -> Result<Box<dyn Workflow>, AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_result_stores_success() {
        let result = WorkflowResult {
            run_id: AgentRunId::new(),
            success: true,
        };
        assert!(result.success);
    }
}
