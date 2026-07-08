//! Notification center Tauri commands.

use app_models::{Notification, NotificationCategory, NotificationId, WorkspaceId};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// List notifications in a workspace, or across all workspaces when `None`.
///
/// # Errors
///
/// Returns an error response if notifications cannot be listed.
#[tauri::command]
pub async fn list_notifications(
    state: State<'_, AppState>,
    workspace_id: Option<WorkspaceId>,
    unread_only: bool,
) -> Result<ApiResponse<Vec<Notification>>, String> {
    Ok(
        match state.runtime.list_notifications(workspace_id, unread_only, 100).await {
            Ok(notifications) => ApiResponse::ok(notifications),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Mark a notification as read and return the updated notification.
///
/// # Errors
///
/// Returns an error response if the notification cannot be updated.
#[tauri::command]
pub async fn mark_notification_read(
    state: State<'_, AppState>,
    id: NotificationId,
    read: Option<bool>,
) -> Result<ApiResponse<Notification>, String> {
    let read = read.unwrap_or(true);
    if let Err(err) = state.runtime.mark_notification_read(id, read).await {
        return Ok(ApiResponse::err(err.to_string()));
    }

    Ok(match state.runtime.get_notification(id).await {
        Ok(notification) => ApiResponse::ok(notification),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Dismiss (delete) a notification.
///
/// # Errors
///
/// Returns an error response if the notification cannot be deleted.
#[tauri::command]
pub async fn dismiss_notification(
    state: State<'_, AppState>,
    id: NotificationId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.dismiss_notification(id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Create a test notification.
///
/// # Errors
///
/// Returns an error response if no workspace is selected or the notification
/// cannot be created.
#[tauri::command]
pub async fn send_test_notification(
    state: State<'_, AppState>,
    workspace_id: Option<WorkspaceId>,
    title: String,
    body: String,
    category: Option<NotificationCategory>,
) -> Result<ApiResponse<Notification>, String> {
    let Some(workspace_id) = workspace_id else {
        return Ok(ApiResponse::err(
            "Select a workspace to send a test notification".to_owned(),
        ));
    };

    let category = category.unwrap_or(NotificationCategory::System);

    Ok(
        match state
            .notification_center
            .create(workspace_id, None, None, title, body, category)
            .await
        {
            Ok(notification) => ApiResponse::ok(notification),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}
