//! Background task Tauri commands.

use app_models::{BackgroundTask, BackgroundTaskId, WorkspaceId};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// List background tasks, optionally filtered by workspace.
///
/// # Errors
///
/// Returns an error response if tasks cannot be listed.
#[tauri::command]
pub async fn list_background_tasks(
    state: State<'_, AppState>,
    workspace_id: Option<WorkspaceId>,
) -> Result<ApiResponse<Vec<BackgroundTask>>, String> {
    Ok(
        match state.runtime.list_background_tasks(workspace_id, None, 100).await {
            Ok(tasks) => ApiResponse::ok(tasks),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Cancel a background task.
///
/// # Errors
///
/// Returns an error response if the task cannot be cancelled.
#[tauri::command]
pub async fn cancel_background_task(
    state: State<'_, AppState>,
    id: BackgroundTaskId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.task_queue().cancel(id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}
