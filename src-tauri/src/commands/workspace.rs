//! Workspace-related Tauri commands.

use app_models::{Workspace, WorkspaceId};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Create a new workspace.
///
/// # Errors
///
/// Returns an error response if the workspace cannot be created.
#[tauri::command]
pub async fn create_workspace(
    state: State<'_, AppState>,
    name: String,
    root_path: String,
    trusted: bool,
) -> Result<ApiResponse<Workspace>, String> {
    Ok(
        match state.runtime.create_workspace(&name, &root_path, trusted).await {
            Ok(workspace) => ApiResponse::ok(workspace),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List all workspaces.
///
/// # Errors
///
/// Returns an error response if workspaces cannot be listed.
#[tauri::command]
pub async fn list_workspaces(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<Workspace>>, String> {
    Ok(match state.runtime.list_workspaces().await {
        Ok(workspaces) => ApiResponse::ok(workspaces),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Fetch a workspace by id.
///
/// # Errors
///
/// Returns an error response if the workspace is not found or cannot be read.
#[tauri::command]
pub async fn get_workspace(
    state: State<'_, AppState>,
    id: WorkspaceId,
) -> Result<ApiResponse<Workspace>, String> {
    Ok(match state.runtime.get_workspace(id).await {
        Ok(workspace) => ApiResponse::ok(workspace),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Update whether a workspace is trusted for sensitive operations.
///
/// # Errors
///
/// Returns an error response if the workspace cannot be updated.
#[tauri::command]
pub async fn trust_workspace(
    state: State<'_, AppState>,
    id: WorkspaceId,
    trusted: bool,
) -> Result<ApiResponse<Workspace>, String> {
    Ok(
        match state.runtime.workspace_manager().trust_workspace(id, trusted).await {
            Ok(workspace) => ApiResponse::ok(workspace),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Replace the allowed read/write paths for a workspace.
///
/// # Errors
///
/// Returns an error response if the paths cannot be persisted.
#[tauri::command]
pub async fn set_workspace_paths(
    state: State<'_, AppState>,
    id: WorkspaceId,
    read_paths: Vec<String>,
    write_paths: Vec<String>,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state
            .runtime
            .workspace_manager()
            .set_allowed_paths(id, read_paths, write_paths)
            .await
        {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}
