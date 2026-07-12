//! Security-related Tauri commands.

use app_models::{AgentRunId, ApprovalRequest, ApprovalRequestId, ThreadId, WorkspaceId};
use app_runtime::AuditLogEntry;
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// List audit log entries with optional filters.
///
/// # Errors
///
/// Returns a string error response if the audit log cannot be read.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn list_audit_log(
    state: State<'_, AppState>,
    workspace_id: Option<WorkspaceId>,
    thread_id: Option<ThreadId>,
    run_id: Option<AgentRunId>,
) -> Result<ApiResponse<Vec<AuditLogEntry>>, String> {
    Ok(
        match state.runtime.list_audit_log(workspace_id, thread_id, run_id).await {
            Ok(entries) => ApiResponse::ok(entries),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List durable pending approvals, optionally filtered to one run.
///
/// # Errors
///
/// Returns a string error response if approvals cannot be read.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn list_pending_approvals(
    state: State<'_, AppState>,
    run_id: Option<AgentRunId>,
) -> Result<ApiResponse<Vec<ApprovalRequest>>, String> {
    Ok(
        match state.runtime.list_pending_approval_requests(run_id).await {
            Ok(approvals) => ApiResponse::ok(approvals),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Approve a pending approval request and resume the associated run.
///
/// # Errors
///
/// Returns a string error response if the request cannot be approved.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn approve_request(
    state: State<'_, AppState>,
    id: ApprovalRequestId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.approve_request(id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Deny a pending approval request and fail the associated run.
///
/// # Errors
///
/// Returns a string error response if the request cannot be denied.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn deny_request(
    state: State<'_, AppState>,
    id: ApprovalRequestId,
    reason: Option<String>,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.deny_request(id, reason).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}
