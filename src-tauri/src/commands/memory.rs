//! Memory, instruction, context, RAG, and learning-loop commands.

use app_models::{
    CandidateStatus, ContextSummary, FeatureCapabilities, InstructionFile, LearningDataExport,
    LearningOverview, LearningQueueStatus, MemoryCandidate, MemoryCandidateId, MemoryId,
    MemoryItem, MemoryScope, PrivacySettings, RagChunk, RagIndexStatus, RagRefreshResult,
    RunContextSnapshot, RunFeedback, RunFeedbackRating, RunLearningSummary, ThreadId, WorkspaceId,
};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Backend-authoritative capability probe for the UI.
///
/// # Errors
///
/// Always succeeds with the current product capability matrix.
#[tauri::command]
pub fn get_feature_capabilities() -> Result<ApiResponse<FeatureCapabilities>, String> {
    Ok(ApiResponse::ok(FeatureCapabilities::default()))
}

/// List memories matching the given scope filters.
///
/// # Errors
///
/// Returns an error response if memories cannot be listed.
#[tauri::command]
pub async fn list_memories(
    state: State<'_, AppState>,
    scope: MemoryScope,
    workspace_id: Option<WorkspaceId>,
    thread_id: Option<ThreadId>,
) -> Result<ApiResponse<Vec<MemoryItem>>, String> {
    Ok(
        match state
            .runtime
            .memory_manager()
            .list_memories(scope, workspace_id, thread_id)
            .await
        {
            Ok(memories) => ApiResponse::ok(memories),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Create a new memory.
///
/// # Errors
///
/// Returns an error response if the memory cannot be created.
#[tauri::command]
pub async fn create_memory(
    state: State<'_, AppState>,
    scope: MemoryScope,
    workspace_id: Option<WorkspaceId>,
    thread_id: Option<ThreadId>,
    key: String,
    value: String,
    sensitive: bool,
) -> Result<ApiResponse<MemoryItem>, String> {
    Ok(
        match state
            .runtime
            .memory_manager()
            .create_memory(scope, workspace_id, thread_id, &key, &value, sensitive)
            .await
        {
            Ok(memory) => ApiResponse::ok(memory),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Update an existing memory's value.
///
/// # Errors
///
/// Returns an error response if the memory is missing or cannot be updated.
#[tauri::command]
pub async fn update_memory(
    state: State<'_, AppState>,
    id: MemoryId,
    value: String,
) -> Result<ApiResponse<MemoryItem>, String> {
    Ok(
        match state.runtime.memory_manager().update_memory(id, &value).await {
            Ok(memory) => ApiResponse::ok(memory),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Delete a memory by id.
///
/// # Errors
///
/// Returns an error response if the memory is missing or cannot be deleted.
#[tauri::command]
pub async fn delete_memory(
    state: State<'_, AppState>,
    id: MemoryId,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state.runtime.memory_manager().delete_memory(id).await {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Load AGENTS.md instructions for a workspace (root resolved from workspace id).
///
/// # Errors
///
/// Returns an error response if the workspace is missing or instructions cannot be loaded.
#[tauri::command]
pub async fn load_instructions(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<Vec<InstructionFile>>, String> {
    use app_memory::InstructionLoader;
    use std::path::Path;

    let workspace = match state.runtime.get_workspace(workspace_id).await {
        Ok(ws) => ws,
        Err(err) => return Ok(ApiResponse::err(err.to_string())),
    };
    let mut instructions = Vec::new();
    let root = Path::new(&workspace.root_path);
    let global_dir = dirs::config_dir().unwrap_or_else(|| root.to_path_buf());
    instructions.extend(InstructionLoader::load_global(&global_dir));
    instructions.extend(InstructionLoader::load_workspace(root));
    Ok(ApiResponse::ok(instructions))
}

/// Inspect the full context for a run (workspace root resolved server-side).
///
/// # Errors
///
/// Returns an error response if the context cannot be assembled.
#[tauri::command]
pub async fn inspect_context(
    state: State<'_, AppState>,
    run_id: app_models::AgentRunId,
    thread_id: ThreadId,
    workspace_id: WorkspaceId,
    query: String,
) -> Result<ApiResponse<ContextSummary>, String> {
    let workspace = match state.runtime.get_workspace(workspace_id).await {
        Ok(ws) => ws,
        Err(err) => return Ok(ApiResponse::err(err.to_string())),
    };
    Ok(
        match state
            .runtime
            .context_inspector()
            .summarize_context(
                run_id,
                thread_id,
                workspace_id,
                &workspace.root_path,
                &query,
            )
            .await
        {
            Ok(summary) => ApiResponse::ok(summary),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Search the workspace RAG index.
///
/// # Errors
///
/// This command always succeeds and returns an empty list if no chunks match.
#[tauri::command]
pub async fn search_rag(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    query: String,
    top_n: usize,
) -> Result<ApiResponse<Vec<RagChunk>>, String> {
    Ok(ApiResponse::ok(
        state.runtime.context_inspector().search_rag(workspace_id, &query, top_n).await,
    ))
}

/// Rebuild the RAG index for a workspace by scanning the project tree from disk.
///
/// # Errors
///
/// Returns an error response if the rebuild fails.
#[tauri::command]
pub async fn rebuild_rag_index(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<usize>, String> {
    let workspace = match state.runtime.get_workspace(workspace_id).await {
        Ok(ws) => ws,
        Err(err) => return Ok(ApiResponse::err(err.to_string())),
    };
    Ok(
        match state
            .runtime
            .context_inspector()
            .rebuild_workspace(workspace_id, Some(&workspace.root_path))
            .await
        {
            Ok(count) => ApiResponse::ok(count),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List memory candidates awaiting review (or filtered by status).
#[tauri::command]
pub async fn list_memory_candidates(
    state: State<'_, AppState>,
    status: Option<CandidateStatus>,
    workspace_id: Option<WorkspaceId>,
) -> Result<ApiResponse<Vec<MemoryCandidate>>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(
        match learning.candidates().list(status, workspace_id, 100).await {
            Ok(items) => ApiResponse::ok(items),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Accept a memory candidate into long-term memory (optionally after edit).
#[tauri::command]
pub async fn accept_memory_candidate(
    state: State<'_, AppState>,
    candidate_id: MemoryCandidateId,
    edited_value: Option<String>,
    scope: Option<MemoryScope>,
    sensitive: Option<bool>,
) -> Result<ApiResponse<MemoryItem>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(
        match learning
            .accept_memory_candidate(
                candidate_id,
                edited_value,
                scope,
                sensitive,
                state.runtime.memory_manager().as_ref(),
            )
            .await
        {
            Ok((_candidate, memory)) => ApiResponse::ok(memory),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Reject a memory candidate (fingerprint suppressed for re-proposal).
#[tauri::command]
pub async fn reject_memory_candidate(
    state: State<'_, AppState>,
    candidate_id: MemoryCandidateId,
) -> Result<ApiResponse<()>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(match learning.candidates().reject(candidate_id).await {
        Ok(_) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Expire a memory candidate without accepting it.
#[tauri::command]
pub async fn expire_memory_candidate(
    state: State<'_, AppState>,
    candidate_id: MemoryCandidateId,
) -> Result<ApiResponse<()>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(match learning.candidates().expire(candidate_id).await {
        Ok(_) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Submit thumbs-up / thumbs-down feedback for a finished run.
#[tauri::command]
pub async fn submit_run_feedback(
    state: State<'_, AppState>,
    run_id: app_models::AgentRunId,
    rating: RunFeedbackRating,
    comment: Option<String>,
) -> Result<ApiResponse<RunFeedback>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(
        match learning.submit_run_feedback(run_id, rating, comment).await {
            Ok(fb) => ApiResponse::ok(fb),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Learning summary for a run (experience, candidates, policy snapshot).
#[tauri::command]
pub async fn get_run_learning_summary(
    state: State<'_, AppState>,
    run_id: app_models::AgentRunId,
) -> Result<ApiResponse<RunLearningSummary>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(match learning.get_run_learning_summary(run_id).await {
        Ok(summary) => ApiResponse::ok(summary),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Learning background queue depth (diagnostics).
#[tauri::command]
pub async fn get_learning_queue_status(
    state: State<'_, AppState>,
) -> Result<ApiResponse<LearningQueueStatus>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::ok(LearningQueueStatus {
            queued: 0,
            running: 0,
            failed: 0,
            completed_recent: 0,
        }));
    };
    Ok(match learning.get_learning_queue_status().await {
        Ok(status) => ApiResponse::ok(status),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Memory Center overview stats.
#[tauri::command]
pub async fn get_learning_overview(
    state: State<'_, AppState>,
) -> Result<ApiResponse<LearningOverview>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    // Production always injects cipher; e2e may not.
    let encryption_enabled = cfg!(not(feature = "desktop-e2e"));
    Ok(
        match learning.get_learning_overview(encryption_enabled).await {
            Ok(overview) => ApiResponse::ok(overview),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Load privacy / learning settings.
#[tauri::command]
pub async fn get_privacy_settings(
    state: State<'_, AppState>,
) -> Result<ApiResponse<PrivacySettings>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::ok(PrivacySettings::default()));
    };
    Ok(match learning.get_privacy_settings().await {
        Ok(s) => ApiResponse::ok(s),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Update privacy / learning settings.
#[tauri::command]
pub async fn update_privacy_settings(
    state: State<'_, AppState>,
    settings: PrivacySettings,
) -> Result<ApiResponse<PrivacySettings>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(match learning.update_privacy_settings(settings).await {
        Ok(s) => ApiResponse::ok(s),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Authoritative frozen context snapshot for a finished (or in-progress) run.
#[tauri::command]
pub async fn get_run_context_snapshot(
    state: State<'_, AppState>,
    run_id: app_models::AgentRunId,
) -> Result<ApiResponse<RunContextSnapshot>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(
        match learning
            .get_run_context_snapshot(run_id, state.runtime.memory_manager().as_ref())
            .await
        {
            Ok(snap) => ApiResponse::ok(snap),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Export all local learning data as JSON-serializable payload.
#[tauri::command]
pub async fn export_learning_data(
    state: State<'_, AppState>,
) -> Result<ApiResponse<LearningDataExport>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(
        match learning
            .export_learning_data(state.runtime.memory_manager().as_ref())
            .await
        {
            Ok(data) => ApiResponse::ok(data),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Clear learning data (candidates / memories / patterns / RAG) — not source files.
#[tauri::command]
pub async fn clear_learning_data(
    state: State<'_, AppState>,
    clear_candidates: bool,
    clear_memories: bool,
    clear_patterns: bool,
    clear_rag: bool,
) -> Result<ApiResponse<()>, String> {
    let Some(learning) = state.runtime.learning() else {
        return Ok(ApiResponse::err(
            "learning coordinator is not available".to_owned(),
        ));
    };
    Ok(
        match learning
            .clear_learning_data(clear_candidates, clear_memories, clear_patterns, clear_rag)
            .await
        {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Incremental RAG index status for a workspace.
#[tauri::command]
pub async fn get_rag_index_status(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<RagIndexStatus>, String> {
    Ok(
        match state
            .runtime
            .context_inspector()
            .get_rag_index_status(workspace_id)
            .await
        {
            Ok(status) => ApiResponse::ok(status),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Refresh only changed / deleted RAG documents for a workspace.
#[tauri::command]
pub async fn refresh_changed_rag_documents(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<RagRefreshResult>, String> {
    let workspace = match state.runtime.get_workspace(workspace_id).await {
        Ok(ws) => ws,
        Err(err) => return Ok(ApiResponse::err(err.to_string())),
    };
    Ok(
        match state
            .runtime
            .context_inspector()
            .refresh_changed_documents(workspace_id, &workspace.root_path)
            .await
        {
            Ok(result) => ApiResponse::ok(result),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}
