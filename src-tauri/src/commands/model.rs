//! Model provider registry Tauri commands.

use app_models::{
    ActiveModelSelection, AgentRunId, ModelCapability, ModelId, ModelInfo, ModelSelectionScope,
    ProviderConfig, ProviderHealth, ProviderId, ProviderKind, RunModelSnapshot, ThreadId,
    WorkspaceId,
};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// List all provider configurations.
///
/// # Errors
///
/// Returns an error response if providers cannot be listed.
#[tauri::command]
pub async fn list_providers(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<ProviderConfig>>, String> {
    Ok(match state.runtime.registry().list_providers().await {
        Ok(providers) => ApiResponse::ok(providers),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Create a new provider configuration.
///
/// # Errors
///
/// Returns an error response if the provider cannot be created.
#[tauri::command]
pub async fn create_provider(
    state: State<'_, AppState>,
    kind: ProviderKind,
    display_name: String,
    base_url: Option<String>,
    api_key_reference: String,
) -> Result<ApiResponse<ProviderConfig>, String> {
    Ok(
        match state
            .runtime
            .registry()
            .create_provider(kind, &display_name, base_url.as_deref(), &api_key_reference)
            .await
        {
            Ok(provider) => ApiResponse::ok(provider),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Update an existing provider configuration.
///
/// # Errors
///
/// Returns an error response if the provider cannot be updated.
#[tauri::command]
pub async fn update_provider(
    state: State<'_, AppState>,
    config: ProviderConfig,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state.runtime.registry().update_provider(config).await {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Delete a provider and its registered models.
///
/// # Errors
///
/// Returns an error response if the provider cannot be deleted.
#[tauri::command]
pub async fn delete_provider(
    state: State<'_, AppState>,
    id: ProviderId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.registry().delete_provider(id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// List models, optionally filtered to a provider.
///
/// # Errors
///
/// Returns an error response if models cannot be listed.
#[tauri::command]
pub async fn list_models(
    state: State<'_, AppState>,
    provider_id: Option<ProviderId>,
) -> Result<ApiResponse<Vec<ModelInfo>>, String> {
    Ok(
        match state.runtime.registry().list_models(provider_id).await {
            Ok(models) => ApiResponse::ok(models),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Register a model under a provider.
///
/// # Errors
///
/// Returns an error response if the model cannot be registered.
#[tauri::command]
pub async fn create_model(
    state: State<'_, AppState>,
    provider_id: ProviderId,
    model_name: String,
    display_name: String,
    capabilities: ModelCapability,
) -> Result<ApiResponse<ModelInfo>, String> {
    Ok(
        match state
            .runtime
            .registry()
            .add_model(provider_id, &model_name, &display_name, capabilities)
            .await
        {
            Ok(model) => ApiResponse::ok(model),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Delete a model by id.
///
/// # Errors
///
/// Returns an error response if the model cannot be deleted.
#[tauri::command]
pub async fn delete_model(
    state: State<'_, AppState>,
    id: ModelId,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.registry().delete_model(id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Set an active provider/model selection at a global, workspace, or thread scope.
///
/// # Errors
///
/// Returns a transport error if the response cannot be constructed.
#[tauri::command]
pub async fn set_active_model(
    state: State<'_, AppState>,
    scope: ModelSelectionScope,
    workspace_id: Option<WorkspaceId>,
    thread_id: Option<ThreadId>,
    provider_id: ProviderId,
    model_id: ModelId,
) -> Result<ApiResponse<ActiveModelSelection>, String> {
    Ok(
        match state
            .runtime
            .registry()
            .set_active_model(scope, workspace_id, thread_id, provider_id, model_id)
            .await
        {
            Ok(selection) => ApiResponse::ok(selection),
            Err(error) => ApiResponse::err(error.to_string()),
        },
    )
}

/// Load one exact active provider/model selection.
///
/// # Errors
///
/// Returns a transport error if the response cannot be constructed.
#[tauri::command]
pub async fn get_active_model(
    state: State<'_, AppState>,
    scope: ModelSelectionScope,
    workspace_id: Option<WorkspaceId>,
    thread_id: Option<ThreadId>,
) -> Result<ApiResponse<Option<ActiveModelSelection>>, String> {
    Ok(
        match state.runtime.registry().get_active_model(scope, workspace_id, thread_id).await {
            Ok(selection) => ApiResponse::ok(selection),
            Err(error) => ApiResponse::err(error.to_string()),
        },
    )
}

/// Resolve the effective thread/workspace/global provider/model selection.
///
/// # Errors
///
/// Returns a transport error if the response cannot be constructed.
#[tauri::command]
pub async fn resolve_active_model(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
) -> Result<ApiResponse<ActiveModelSelection>, String> {
    Ok(
        match state.runtime.registry().resolve_active_model(workspace_id, thread_id).await {
            Ok(selection) => ApiResponse::ok(selection),
            Err(error) => ApiResponse::err(error.to_string()),
        },
    )
}

/// Perform a bounded provider/model connection check and persist a safe summary.
///
/// # Errors
///
/// Returns a transport error if the response cannot be constructed.
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, AppState>,
    provider_id: ProviderId,
    model_id: ModelId,
) -> Result<ApiResponse<ProviderHealth>, String> {
    Ok(
        match autoagents_adapter::check_provider_health(
            state.runtime.registry().clone(),
            state.secret_store.clone(),
            provider_id,
            model_id,
        )
        .await
        {
            Ok(health) => ApiResponse::ok(health),
            Err(error) => ApiResponse::err(error.to_string()),
        },
    )
}

/// Load the last provider health result.
///
/// # Errors
///
/// Returns a transport error if the response cannot be constructed.
#[tauri::command]
pub async fn get_provider_health(
    state: State<'_, AppState>,
    provider_id: ProviderId,
    model_id: ModelId,
) -> Result<ApiResponse<Option<ProviderHealth>>, String> {
    Ok(
        match state.runtime.registry().get_provider_health(provider_id, model_id).await {
            Ok(health) => ApiResponse::ok(health),
            Err(error) => ApiResponse::err(error.to_string()),
        },
    )
}

/// Load the immutable provider/model snapshot captured for a run.
///
/// # Errors
///
/// Returns a transport error if the response cannot be constructed.
#[tauri::command]
pub async fn get_run_model_snapshot(
    state: State<'_, AppState>,
    run_id: AgentRunId,
) -> Result<ApiResponse<Option<RunModelSnapshot>>, String> {
    Ok(
        match state.runtime.registry().get_run_model_snapshot(run_id).await {
            Ok(snapshot) => ApiResponse::ok(snapshot),
            Err(error) => ApiResponse::err(error.to_string()),
        },
    )
}
