//! Memory, instruction, context, and RAG commands.

use app_models::{
    ContextSummary, InstructionFile, MemoryId, MemoryItem, MemoryScope, RagChunk, ThreadId,
    WorkspaceId,
};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

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

/// Load AGENTS.md instructions for a workspace.
///
/// # Errors
///
/// Returns an error response if instructions cannot be loaded.
#[tauri::command]
pub async fn load_instructions(
    _state: State<'_, AppState>,
    workspace_root: String,
) -> Result<ApiResponse<Vec<InstructionFile>>, String> {
    use app_memory::InstructionLoader;
    use std::path::Path;

    let mut instructions = Vec::new();
    let root = Path::new(&workspace_root);

    let global_dir = dirs::config_dir().unwrap_or_else(|| root.to_path_buf());
    instructions.extend(InstructionLoader::load_global(&global_dir));
    instructions.extend(InstructionLoader::load_workspace(root));

    Ok(ApiResponse::ok(instructions))
}

/// Inspect the full context for a run.
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
    workspace_root: String,
    query: String,
) -> Result<ApiResponse<ContextSummary>, String> {
    Ok(
        match state
            .runtime
            .context_inspector()
            .summarize_context(run_id, thread_id, workspace_id, &workspace_root, &query)
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
        state
            .runtime
            .context_inspector()
            .search_rag(workspace_id, &query, top_n)
            .await,
    ))
}

/// Rebuild the RAG index for a workspace using the current embedding provider.
///
/// # Errors
///
/// Returns an error response if the rebuild fails.
#[tauri::command]
pub async fn rebuild_rag_index(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<usize>, String> {
    Ok(
        match state
            .runtime
            .context_inspector()
            .rebuild_workspace(workspace_id)
            .await
        {
            Ok(count) => ApiResponse::ok(count),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}
