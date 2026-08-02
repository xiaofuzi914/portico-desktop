//! Thread-related Tauri commands.

use app_models::{MessageRole, Thread, ThreadId, WorkspaceId};
use app_workflows::canvas_extract::{
    branch_title_from_focus, build_branch_context_seed_with_focus, summarize_session_messages,
};
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

/// Branch a new session from an existing one, carrying parent context.
///
/// Creates a child thread linked via `parent_thread_id` and seeds a system
/// message with a summary of the parent conversation so the new session can
/// diverge while still knowing the prior context. When `focus_text` is set
/// (划词发散), the seed prioritizes that selection and the child title is
/// derived from it — this is the primary path for mind-map closed loop.
///
/// # Errors
///
/// Returns an error response if the parent is missing or persistence fails.
#[tauri::command]
pub async fn branch_thread_from_context(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    parent_thread_id: ThreadId,
    title: Option<String>,
    focus_text: Option<String>,
) -> Result<ApiResponse<Thread>, String> {
    Ok(
        match branch_thread_from_context_inner(
            &state,
            workspace_id,
            parent_thread_id,
            title.as_deref(),
            focus_text.as_deref(),
        )
        .await
        {
            Ok(thread) => ApiResponse::ok(thread),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

async fn branch_thread_from_context_inner(
    state: &AppState,
    workspace_id: WorkspaceId,
    parent_thread_id: ThreadId,
    title: Option<&str>,
    focus_text: Option<&str>,
) -> Result<Thread, app_models::AppError> {
    let storage = state.runtime.storage();
    let parent = storage.get_thread(parent_thread_id).await?;
    if parent.workspace_id != workspace_id {
        return Err(app_models::AppError::PermissionDenied {
            reason: "parent thread belongs to a different workspace".to_owned(),
        });
    }

    let messages = storage.list_messages(parent_thread_id).await?;
    let summary = summarize_session_messages(&messages, 300);
    let focus = focus_text.map(str::trim).filter(|s| !s.is_empty());
    let context_seed =
        build_branch_context_seed_with_focus(&parent.title, &summary, &messages, focus);

    let child_title = title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(80).collect::<String>())
        .unwrap_or_else(|| {
            if let Some(f) = focus {
                branch_title_from_focus(f, &parent.title)
            } else {
                let base = parent.title.trim();
                if base.is_empty() {
                    "发散会话".to_owned()
                } else {
                    format!("{} · 发散", base.chars().take(60).collect::<String>())
                }
            }
        });

    let child = state
        .runtime
        .create_thread_with_parent(workspace_id, &child_title, Some(parent_thread_id))
        .await?;

    // Inherit the parent's effective model so the first child send does not race
    // an empty Thread selection (parent-only model → NotFound / provider errors).
    if let Ok(selection) = state
        .runtime
        .registry()
        .resolve_active_model(workspace_id, parent_thread_id)
        .await
    {
        let _ = state
            .runtime
            .registry()
            .set_active_model(
                app_models::ModelSelectionScope::Thread,
                Some(workspace_id),
                Some(child.id),
                selection.provider_id,
                selection.model_id,
            )
            .await;
    }

    if !context_seed.trim().is_empty() {
        // Prefix is recognized by the UI as context (not a run failure).
        let seeded = if context_seed.trim_start().starts_with('【') {
            context_seed
        } else {
            format!("【会话上下文】\n{context_seed}")
        };
        let _ = storage
            .create_standalone_message(child.id, MessageRole::System, &seeded)
            .await?;
    }

    Ok(child)
}

/// List **active** (non-archived) threads in a workspace.
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

/// List archived threads for a workspace (newest archive first).
#[tauri::command]
pub async fn list_archived_threads(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<Vec<Thread>>, String> {
    Ok(
        match state.runtime.list_archived_threads(workspace_id).await {
            Ok(threads) => ApiResponse::ok(threads),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
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

/// Soft-delete: move a session into the archive (30-day retention before purge).
#[tauri::command]
pub async fn archive_thread(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    id: ThreadId,
) -> Result<ApiResponse<Thread>, String> {
    Ok(match state.runtime.archive_thread(workspace_id, id).await {
        Ok(thread) => ApiResponse::ok(thread),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Restore a session from the archive to the active list.
#[tauri::command]
pub async fn restore_thread(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    id: ThreadId,
) -> Result<ApiResponse<Thread>, String> {
    Ok(match state.runtime.restore_thread(workspace_id, id).await {
        Ok(thread) => ApiResponse::ok(thread),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Permanently delete a thread and its conversation history.
///
/// Prefer [`archive_thread`] for the normal product “delete” action.
///
/// # Errors
///
/// Returns an error response if the thread cannot be deleted.
#[tauri::command]
pub async fn delete_thread(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    id: ThreadId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.delete_thread(workspace_id, id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Update a thread's display title (session topic).
///
/// # Errors
///
/// Returns an error response if the title is empty or the thread is missing.
#[tauri::command]
pub async fn update_thread_title(
    state: State<'_, AppState>,
    id: ThreadId,
    title: String,
) -> Result<ApiResponse<Thread>, String> {
    Ok(match state.runtime.update_thread_title(id, &title).await {
        Ok(thread) => ApiResponse::ok(thread),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}
