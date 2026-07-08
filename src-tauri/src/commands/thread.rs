//! Thread-related Tauri commands.

use app_models::{Thread, ThreadId, WorkspaceId};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Create a new thread in a workspace.
///
/// # Errors
///
/// Returns an error response if the thread cannot be created.
#[tauri::command]
pub async fn create_thread(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    title: String,
) -> Result<ApiResponse<Thread>, String> {
    Ok(
        match state.runtime.create_thread(workspace_id, &title).await {
            Ok(thread) => ApiResponse::ok(thread),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List threads in a workspace.
///
/// # Errors
///
/// Returns an error response if threads cannot be listed.
#[tauri::command]
pub async fn list_threads(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<Vec<Thread>>, String> {
    Ok(match state.runtime.list_threads(workspace_id).await {
        Ok(threads) => ApiResponse::ok(threads),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Fetch a thread by id.
///
/// # Errors
///
/// Returns an error response if the thread is not found or cannot be read.
#[tauri::command]
pub async fn get_thread(
    state: State<'_, AppState>,
    id: ThreadId,
) -> Result<ApiResponse<Thread>, String> {
    Ok(match state.runtime.get_thread(id).await {
        Ok(thread) => ApiResponse::ok(thread),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}
