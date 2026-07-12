//! Multi-agent orchestration commands (memory-conditioned).

use app_models::{
    AgentDefinition, AgentRunId, Orchestration, OrchestrationId, OrchestrationPlan, PatternHint,
    ThreadId, WorkspaceId,
};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// List registered agent role definitions.
///
/// # Errors
///
/// Returns an error response if state cannot be accessed.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn list_agents(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<AgentDefinition>>, String> {
    Ok(ApiResponse::ok(state.orchestration.list_agents()))
}

/// Recall workflow patterns for a task (memory layer, loosely coupled).
///
/// # Errors
///
/// Returns an error response if recall fails.
#[tauri::command]
pub async fn recall_workflow_patterns(
    state: State<'_, AppState>,
    task: String,
    workspace_id: Option<WorkspaceId>,
) -> Result<ApiResponse<Vec<PatternHint>>, String> {
    Ok(
        match state.orchestration.recall_patterns(&task, workspace_id).await {
            Ok(hints) => ApiResponse::ok(hints),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Preview a memory-conditioned multi-agent plan for a parent run.
///
/// # Errors
///
/// Returns an error response if planning fails.
#[tauri::command]
pub async fn preview_orchestration_plan(
    state: State<'_, AppState>,
    parent_run_id: AgentRunId,
    task: String,
) -> Result<ApiResponse<OrchestrationPlan>, String> {
    Ok(
        match state.orchestration.preview_plan(parent_run_id, &task).await {
            Ok(plan) => ApiResponse::ok(plan),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Start a full multi-agent orchestration closed loop.
///
/// # Errors
///
/// Returns an error response if orchestration fails.
#[tauri::command]
pub async fn start_orchestration(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
    task: String,
) -> Result<ApiResponse<Orchestration>, String> {
    Ok(
        match state.orchestration.start_orchestration(workspace_id, thread_id, &task).await {
            Ok(session) => ApiResponse::ok(session),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Get an orchestration session by id.
///
/// # Errors
///
/// Returns an error response if the session is missing.
#[tauri::command]
pub async fn get_orchestration(
    state: State<'_, AppState>,
    orchestration_id: OrchestrationId,
) -> Result<ApiResponse<Orchestration>, String> {
    Ok(
        match state.orchestration.get_orchestration(orchestration_id).await {
            Ok(session) => ApiResponse::ok(session),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List orchestration sessions for a thread.
///
/// # Errors
///
/// Returns an error response if state cannot be accessed.
#[tauri::command]
pub async fn list_thread_orchestrations(
    state: State<'_, AppState>,
    thread_id: ThreadId,
) -> Result<ApiResponse<Vec<Orchestration>>, String> {
    Ok(ApiResponse::ok(
        state.orchestration.list_for_thread(thread_id).await,
    ))
}

/// Cancel an orchestration and its child runs (best effort).
///
/// # Errors
///
/// Returns an error response if the session is missing.
#[tauri::command]
pub async fn cancel_orchestration(
    state: State<'_, AppState>,
    orchestration_id: OrchestrationId,
) -> Result<ApiResponse<Orchestration>, String> {
    Ok(match state.orchestration.cancel(orchestration_id).await {
        Ok(session) => ApiResponse::ok(session),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// List workflow patterns for a memory scope (and optional workspace).
///
/// # Errors
///
/// Returns an error response if the pattern store is unavailable or listing fails.
#[tauri::command]
pub async fn list_workflow_patterns(
    state: State<'_, AppState>,
    scope: app_models::MemoryScope,
    workspace_id: Option<WorkspaceId>,
) -> Result<ApiResponse<Vec<app_models::WorkflowPattern>>, String> {
    let Some(store) = state.pattern_store.as_ref() else {
        return Ok(ApiResponse::err(
            "pattern store is not available in this build".to_owned(),
        ));
    };
    Ok(match store.list_patterns(scope, workspace_id).await {
        Ok(patterns) => ApiResponse::ok(patterns),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Mute a workflow pattern so it no longer influences planning.
///
/// # Errors
///
/// Returns an error response if the pattern store rejects the update.
#[tauri::command]
pub async fn mute_workflow_pattern(
    state: State<'_, AppState>,
    pattern_id: app_models::WorkflowPatternId,
) -> Result<ApiResponse<()>, String> {
    let Some(store) = state.pattern_store.as_ref() else {
        return Ok(ApiResponse::err(
            "pattern store is not available in this build".to_owned(),
        ));
    };
    Ok(match store.mute_pattern(pattern_id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}
