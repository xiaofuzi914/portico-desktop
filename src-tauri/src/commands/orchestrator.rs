//! Orchestrator-related Tauri commands.

use app_models::{AgentDefinition, AgentRunId, OrchestrationPlan, SubagentRun};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// List all registered agent definitions.
///
/// # Errors
///
/// Returns an error response if state cannot be accessed.
#[tauri::command]
pub async fn list_agents(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<AgentDefinition>>, String> {
    Ok(ApiResponse::ok(state.orchestrator.list_agents()))
}

/// Create an orchestration plan for a parent run.
///
/// # Errors
///
/// Returns an error response if planning fails.
#[tauri::command]
pub async fn plan_subagents(
    state: State<'_, AppState>,
    parent_run_id: AgentRunId,
    task: String,
) -> Result<ApiResponse<OrchestrationPlan>, String> {
    Ok(match state.orchestrator.plan(parent_run_id, &task).await {
        Ok(plan) => ApiResponse::ok(plan),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Execute an orchestration plan.
///
/// # Errors
///
/// Returns an error response if execution fails.
#[tauri::command]
pub async fn execute_subagents(
    state: State<'_, AppState>,
    plan: OrchestrationPlan,
) -> Result<ApiResponse<Vec<SubagentRun>>, String> {
    Ok(match state.orchestrator.execute_plan(plan).await {
        Ok(runs) => ApiResponse::ok(runs),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Cancel a subagent run.
///
/// # Errors
///
/// Returns an error response if cancellation fails.
#[tauri::command]
pub async fn cancel_subagent(
    state: State<'_, AppState>,
    subagent_run_id: AgentRunId,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state.orchestrator.cancel_subagent(subagent_run_id).await {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}
