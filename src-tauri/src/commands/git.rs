//! Git-related Tauri commands.

use app_models::WorkspaceId;
use app_tools::git::GitOperations;
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

fn git_tool_error() -> String {
    "git tool not available".to_owned()
}

/// Run `git status` in a repository.
///
/// # Errors
///
/// Returns an error response if the command is denied or fails.
#[tauri::command]
pub async fn git_status(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    repo_path: String,
) -> Result<ApiResponse<String>, String> {
    let git = state.runtime.git_tool().ok_or_else(git_tool_error)?;
    Ok(match git.status(workspace_id, &repo_path).await {
        Ok(output) => ApiResponse::ok(output),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Run `git diff` in a repository.
///
/// # Errors
///
/// Returns an error response if the command is denied or fails.
#[tauri::command]
pub async fn git_diff(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    repo_path: String,
) -> Result<ApiResponse<String>, String> {
    let git = state.runtime.git_tool().ok_or_else(git_tool_error)?;
    Ok(match git.diff(workspace_id, &repo_path).await {
        Ok(output) => ApiResponse::ok(output),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Stage paths in a repository.
///
/// # Errors
///
/// Returns an error response if the command is denied or fails.
#[tauri::command]
pub async fn git_stage(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    repo_path: String,
    paths: Vec<String>,
) -> Result<ApiResponse<()>, String> {
    let git = state.runtime.git_tool().ok_or_else(git_tool_error)?;
    Ok(match git.stage(workspace_id, &repo_path, &paths).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Unstage paths in a repository.
///
/// # Errors
///
/// Returns an error response if the command is denied or fails.
#[tauri::command]
pub async fn git_unstage(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    repo_path: String,
    paths: Vec<String>,
) -> Result<ApiResponse<()>, String> {
    let git = state.runtime.git_tool().ok_or_else(git_tool_error)?;
    Ok(match git.unstage(workspace_id, &repo_path, &paths).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Commit staged changes in a repository.
///
/// # Errors
///
/// Returns an error response if the command is denied or fails.
#[tauri::command]
pub async fn git_commit(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    repo_path: String,
    message: String,
) -> Result<ApiResponse<String>, String> {
    let git = state.runtime.git_tool().ok_or_else(git_tool_error)?;
    Ok(match git.commit(workspace_id, &repo_path, &message).await {
        Ok(output) => ApiResponse::ok(output),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Show the current branch or create a new one.
///
/// # Errors
///
/// Returns an error response if the command is denied or fails.
#[tauri::command]
pub async fn git_branch(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    repo_path: String,
    name: Option<String>,
) -> Result<ApiResponse<String>, String> {
    let git = state.runtime.git_tool().ok_or_else(git_tool_error)?;
    let name_ref = name.as_deref();
    Ok(match git.branch(workspace_id, &repo_path, name_ref).await {
        Ok(output) => ApiResponse::ok(output),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Push to a remote.
///
/// # Errors
///
/// Returns an error response if the command is denied or fails.
#[tauri::command]
pub async fn git_push(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    repo_path: String,
    remote: Option<String>,
) -> Result<ApiResponse<String>, String> {
    let git = state.runtime.git_tool().ok_or_else(git_tool_error)?;
    let remote_ref = remote.as_deref();
    Ok(match git.push(workspace_id, &repo_path, remote_ref).await {
        Ok(output) => ApiResponse::ok(output),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}
