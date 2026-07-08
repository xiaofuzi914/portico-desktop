//! Model provider registry Tauri commands.

use app_models::{
    AgentRunId, ModelCapability, ModelId, ModelInfo, ProviderConfig, ProviderId, ProviderKind,
};
use serde::Serialize;
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Summary of today's usage against the app-level budget.
#[derive(Debug, Serialize)]
pub struct UsageSummary {
    /// Total USD spent across all providers today.
    pub daily_usage_usd: f64,
    /// Per-run budget in USD, if configured.
    pub per_run_budget_usd: Option<f64>,
    /// Daily budget in USD, if configured.
    pub daily_budget_usd: Option<f64>,
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

/// Return today's usage summary against the app-level budget.
///
/// # Errors
///
/// Returns an error response if usage cannot be read.
#[tauri::command]
pub async fn get_usage_summary(
    state: State<'_, AppState>,
) -> Result<ApiResponse<UsageSummary>, String> {
    let registry = state.runtime.registry();
    let budget = registry.budget();

    match registry.get_total_daily_usage().await {
        Ok(daily_usage_usd) => Ok(ApiResponse::ok(UsageSummary {
            daily_usage_usd,
            per_run_budget_usd: budget.per_run_usd,
            daily_budget_usd: budget.daily_usd,
        })),
        Err(err) => Ok(ApiResponse::err(err.to_string())),
    }
}

/// Check whether an estimated cost is within budget for a run.
///
/// # Errors
///
/// Returns an error response if the budget check fails.
#[tauri::command]
pub async fn check_budget(
    state: State<'_, AppState>,
    provider_id: ProviderId,
    run_id: AgentRunId,
    estimated_cost_usd: f64,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state
            .runtime
            .registry()
            .check_budget(provider_id, run_id, estimated_cost_usd)
            .await
        {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}
