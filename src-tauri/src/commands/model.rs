//! Model provider registry Tauri commands.

use app_models::{
    ActiveModelSelection, AgentRunId, ModelCapability, ModelId, ModelInfo, ModelSelectionScope,
    ProviderConfig, ProviderHealth, ProviderId, ProviderKind, RunModelSnapshot, ThreadId,
    WorkspaceId,
};
use tauri::State;

use crate::cli_auth_import::{
    import_cli_auth_source, list_cli_auth_sources, CliAuthImport, CliAuthSource,
};
use crate::AppState;
use crate::error::ApiResponse;

/// Scan local Codex / Kimi / Grok CLI login state (no secrets in the response).
#[tauri::command]
pub async fn list_cli_auth_sources_cmd() -> Result<ApiResponse<Vec<CliAuthSource>>, String> {
    Ok(ApiResponse::ok(list_cli_auth_sources()))
}

/// Import a CLI login into Portico: store the session token and create a provider + models.
#[tauri::command]
pub async fn import_cli_auth_source_cmd(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<ApiResponse<ProviderConfig>, String> {
    Ok(
        match import_cli_auth_and_configure(&state, &source_id).await {
            Ok(provider) => ApiResponse::ok(provider),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

async fn import_cli_auth_and_configure(
    state: &AppState,
    source_id: &str,
) -> Result<ProviderConfig, app_models::AppError> {
    let imported: CliAuthImport = import_cli_auth_source(source_id)?;

    // Persist secret first so connection probe can resolve it.
    state
        .secret_store
        .set(&imported.key_reference, &imported.secret)
        .map_err(|e| app_models::AppError::Internal {
            message: format!("store CLI auth secret failed: {e}"),
        })?;

    let mut provider = state
        .runtime
        .registry()
        .create_provider(
            imported.kind,
            &imported.display_name,
            imported.base_url.as_deref(),
            &imported.key_reference,
        )
        .await?;

    // Attach ChatGPT-Account-ID (and any other CLI headers) after create.
    if !imported.default_headers.is_empty()
        || imported.base_url.is_some() && provider.base_url != imported.base_url
    {
        provider.default_headers = imported.default_headers.clone();
        if let Some(ref url) = imported.base_url {
            provider.base_url = Some(url.clone());
        }
        provider.updated_at = chrono::Utc::now();
        state
            .runtime
            .registry()
            .update_provider(provider.clone())
            .await?;
    }

    // Prefer CLI-specific model seeds (e.g. Codex ChatGPT models) over generic presets.
    let models: Vec<(String, String)> = if imported.suggested_models.is_empty() {
        default_models_for_kind(imported.kind)
            .into_iter()
            .map(|(n, d)| (n.to_owned(), d.to_owned()))
            .collect()
    } else {
        imported.suggested_models.clone()
    };
    let mut first_model_id = None;
    for (model_name, display_name) in models {
        let caps = default_text_capabilities();
        match state
            .runtime
            .registry()
            .add_model(provider.id, &model_name, &display_name, caps)
            .await
        {
            Ok(m) => {
                if first_model_id.is_none() {
                    first_model_id = Some(m.id);
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, model_name, "seed model after CLI import failed");
            }
        }
    }

    if let Some(model_id) = first_model_id {
        // Best-effort health probe; ChatGPT Codex sessions use a special gateway.
        let _ = autoagents_adapter::check_provider_health(
            state.runtime.registry().clone(),
            state.secret_store.clone(),
            provider.id,
            model_id,
        )
        .await;
        let _ = state
            .runtime
            .registry()
            .set_active_model(
                ModelSelectionScope::Global,
                None,
                None,
                provider.id,
                model_id,
            )
            .await;
    }

    // Re-load so the UI gets persisted headers / base_url.
    state.runtime.registry().get_provider(provider.id).await
}

fn default_text_capabilities() -> ModelCapability {
    ModelCapability {
        supports_streaming: true,
        supports_tools: true,
        supports_json_schema: false,
        supports_vision: false,
        supports_pdf: false,
        supports_system_prompt: true,
        supports_embeddings: false,
        max_context_tokens: None,
        input_price_per_1k: None,
        output_price_per_1k: None,
    }
}

fn default_models_for_kind(kind: ProviderKind) -> Vec<(&'static str, &'static str)> {
    match kind {
        ProviderKind::OpenAI => vec![
            ("gpt-4.1", "GPT-4.1"),
            ("gpt-4.1-mini", "GPT-4.1 mini"),
        ],
        ProviderKind::Moonshot => vec![
            ("kimi-k2-turbo-preview", "Kimi K2 Turbo"),
            ("kimi-k2-0711-preview", "Kimi K2"),
        ],
        ProviderKind::Xai => vec![("grok-3", "Grok 3"), ("grok-3-mini", "Grok 3 Mini")],
        ProviderKind::DeepSeek => vec![
            ("deepseek-v4-pro", "DeepSeek V4 Pro"),
            ("deepseek-v4-flash", "DeepSeek V4 Flash"),
        ],
        ProviderKind::Anthropic => vec![("claude-sonnet-4-5", "Claude Sonnet 4.5")],
        _ => vec![("default", "Default")],
    }
}

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
