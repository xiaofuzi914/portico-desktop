//! Worktree-related Tauri commands.

use app_models::{ThreadId, WorkspaceId, Worktree, WorktreeId};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Create a new worktree in a workspace/thread.
///
/// # Errors
///
/// Returns an error response if the worktree cannot be created.
#[tauri::command]
pub async fn create_worktree(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
    name: String,
) -> Result<ApiResponse<Worktree>, String> {
    Ok(
        match state
            .runtime
            .worktree_manager()
            .create_worktree(workspace_id, thread_id, &name)
            .await
        {
            Ok(worktree) => ApiResponse::ok(worktree),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Delete a worktree by id.
///
/// # Errors
///
/// Returns an error response if the worktree cannot be deleted.
#[tauri::command]
pub async fn delete_worktree(
    state: State<'_, AppState>,
    id: WorktreeId,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state.runtime.worktree_manager().delete_worktree(id).await {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List worktrees in a workspace.
///
/// # Errors
///
/// Returns an error response if worktrees cannot be listed.
#[tauri::command]
pub async fn list_worktrees(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<Vec<Worktree>>, String> {
    Ok(
        match state.runtime.worktree_manager().list_worktrees(workspace_id).await {
            Ok(worktrees) => ApiResponse::ok(worktrees),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}
