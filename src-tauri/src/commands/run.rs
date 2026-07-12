//! Run-related Tauri commands.

use app_models::{AgentRun, AgentRunId, Message, RunEvent, ThreadId, WorkspaceId};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Start a new agent run.
///
/// # Errors
///
/// Returns an error response if the run cannot be started.
#[tauri::command]
pub async fn start_run(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
) -> Result<ApiResponse<AgentRun>, String> {
    Ok(
        match state.runtime.start_run(workspace_id, thread_id).await {
            Ok(run) => ApiResponse::ok(run),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Submit a user message to a run.
///
/// # Errors
///
/// Returns an error response if the message cannot be submitted.
#[tauri::command]
pub async fn submit_message(
    state: State<'_, AppState>,
    run_id: AgentRunId,
    content: String,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.submit_message(run_id, &content).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Persist a user turn, create its run, and schedule execution.
///
/// # Errors
///
/// Returns a transport error if the Tauri response cannot be constructed.
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    thread_id: ThreadId,
    content: String,
    client_request_id: Option<String>,
) -> Result<ApiResponse<AgentRun>, String> {
    Ok(
        match state
            .runtime
            .send_message(thread_id, &content, client_request_id.as_deref())
            .await
        {
            Ok(run) => ApiResponse::ok(run),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List durable messages in a thread.
///
/// # Errors
///
/// Returns a transport error if the Tauri response cannot be constructed.
#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    thread_id: ThreadId,
) -> Result<ApiResponse<Vec<Message>>, String> {
    Ok(match state.runtime.list_messages(thread_id).await {
        Ok(messages) => ApiResponse::ok(messages),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// List runs in a thread, newest first.
///
/// # Errors
///
/// Returns a transport error if the Tauri response cannot be constructed.
#[tauri::command]
pub async fn list_runs(
    state: State<'_, AppState>,
    thread_id: ThreadId,
) -> Result<ApiResponse<Vec<AgentRun>>, String> {
    Ok(match state.runtime.list_runs(thread_id).await {
        Ok(runs) => ApiResponse::ok(runs),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Cancel a run.
///
/// # Errors
///
/// Returns an error response if the run cannot be cancelled.
#[tauri::command]
pub async fn cancel_run(
    state: State<'_, AppState>,
    run_id: AgentRunId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.cancel_run(run_id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Pause a run.
///
/// # Errors
///
/// Returns an error response if the run cannot be paused.
#[tauri::command]
pub async fn pause_run(
    state: State<'_, AppState>,
    run_id: AgentRunId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.pause_run(run_id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Resume a paused run.
///
/// # Errors
///
/// Returns an error response if the run cannot be resumed.
#[tauri::command]
pub async fn resume_run(
    state: State<'_, AppState>,
    run_id: AgentRunId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.resume_run(run_id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// List persisted events for a run.
///
/// # Errors
///
/// Returns an error response if events cannot be listed.
#[tauri::command]
pub async fn list_run_events(
    state: State<'_, AppState>,
    run_id: AgentRunId,
) -> Result<ApiResponse<Vec<RunEvent>>, String> {
    Ok(match state.runtime.list_run_events(run_id).await {
        Ok(events) => ApiResponse::ok(events),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Token usage totals for a run (sums all recorded LLM turns).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunTokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// Fetch recorded token usage for a run.
///
/// # Errors
///
/// Returns an error response if usage cannot be read.
#[tauri::command]
pub async fn get_run_token_usage(
    state: State<'_, AppState>,
    run_id: AgentRunId,
) -> Result<ApiResponse<RunTokenUsage>, String> {
    Ok(
        match state.runtime.get_run_token_usage(run_id).await {
            Ok((input_tokens, output_tokens)) => ApiResponse::ok(RunTokenUsage {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens.saturating_add(output_tokens),
            }),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}
