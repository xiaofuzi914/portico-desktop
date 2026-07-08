//! Automation scheduler Tauri commands.

use app_models::{Automation, AutomationId, AutomationTrigger, WorkspaceId};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// List automations in a workspace.
///
/// # Errors
///
/// Returns an error response if automations cannot be listed.
#[tauri::command]
pub async fn list_automations(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<Vec<Automation>>, String> {
    Ok(match state.scheduler.list_automations(workspace_id).await {
        Ok(automations) => ApiResponse::ok(automations),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Create a new automation.
///
/// # Errors
///
/// Returns an error response if the automation cannot be created.
#[tauri::command]
pub async fn create_automation(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    name: String,
    description: String,
    trigger: AutomationTrigger,
    cron_expr: Option<String>,
    enabled: bool,
) -> Result<ApiResponse<Automation>, String> {
    Ok(
        match state
            .scheduler
            .create_automation(
                workspace_id,
                name,
                description,
                trigger,
                cron_expr,
                enabled,
                serde_json::Value::Null,
            )
            .await
        {
            Ok(automation) => ApiResponse::ok(automation),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Update an existing automation.
///
/// # Errors
///
/// Returns an error response if the automation cannot be updated.
#[tauri::command]
pub async fn update_automation(
    state: State<'_, AppState>,
    automation: Automation,
) -> Result<ApiResponse<Automation>, String> {
    if let Err(err) = state.scheduler.update_automation(automation.clone()).await {
        return Ok(ApiResponse::err(err.to_string()));
    }

    Ok(match state.scheduler.get_automation(automation.id).await {
        Ok(updated) => ApiResponse::ok(updated),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Delete an automation.
///
/// # Errors
///
/// Returns an error response if the automation cannot be deleted.
#[tauri::command]
pub async fn delete_automation(
    state: State<'_, AppState>,
    id: AutomationId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.scheduler.delete_automation(id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Run an automation immediately.
///
/// # Errors
///
/// Returns an error response if the automation cannot be started.
#[tauri::command]
pub async fn run_automation_now(
    state: State<'_, AppState>,
    id: AutomationId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.scheduler.run_now(id, &state.runtime).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}
