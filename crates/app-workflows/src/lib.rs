//! Workflow and orchestration abstractions for Portico.
//!
//! **Product note:** default UX is single-agent chat + tools
//! (`docs/AGENT-PRODUCT-PATH.md`). Multi-agent orchestration here is a
//! **production** path for real multi-role tasks (composer secondary action),
//! not a peer dual-mode UI.

pub mod agent_registry;
pub mod memory_plan;
pub mod orchestration_service;
pub mod orchestrator;
pub mod ports;
pub mod scheduler;

pub use agent_registry::AgentRegistry;
pub use orchestration_service::OrchestrationService;
pub use orchestrator::Orchestrator;
pub use ports::{
    NoopPatternSink, NoopPatternSource, PatternRecallQuery, PatternSink, PatternSource,
};
pub use scheduler::AutomationScheduler;

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
