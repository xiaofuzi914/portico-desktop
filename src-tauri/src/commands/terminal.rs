//! Terminal-related Tauri commands.

use app_models::{TerminalId, TerminalOutput, ThreadId};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

fn terminal_error() -> String {
    "terminal not available".to_owned()
}

/// Create a new terminal session for a thread.
///
/// # Errors
///
/// Returns an error response if the terminal manager is unavailable.
#[tauri::command]
pub async fn create_terminal(
    state: State<'_, AppState>,
    thread_id: ThreadId,
) -> Result<ApiResponse<TerminalId>, String> {
    let terminal = state.runtime.terminal_manager().ok_or_else(terminal_error)?;
    Ok(ApiResponse::ok(terminal.create_session(thread_id).await))
}

/// Execute a command in a terminal session.
///
/// # Errors
///
/// Returns an error response if the command cannot be executed.
#[tauri::command]
pub async fn execute_terminal_command(
    state: State<'_, AppState>,
    id: TerminalId,
    command: String,
    cwd: String,
) -> Result<ApiResponse<TerminalOutput>, String> {
    let terminal = state.runtime.terminal_manager().ok_or_else(terminal_error)?;
    Ok(match terminal.execute(id, &command, &cwd).await {
        Ok(output) => ApiResponse::ok(output),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Read command history for a terminal session.
///
/// # Errors
///
/// Returns an error response if the terminal manager is unavailable.
#[tauri::command]
pub async fn read_terminal_history(
    state: State<'_, AppState>,
    id: TerminalId,
) -> Result<ApiResponse<Vec<String>>, String> {
    let terminal = state.runtime.terminal_manager().ok_or_else(terminal_error)?;
    Ok(ApiResponse::ok(terminal.read_history(id).await))
}
