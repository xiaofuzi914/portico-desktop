//! Keychain-backed secret management commands.
//!
//! These commands let the frontend store and remove provider API keys in the
//! OS keychain. Secrets are never written to `SQLite` or logged.

use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Store a provider API key in the OS keychain under `api_key_reference`.
///
/// # Errors
///
/// Returns an error response if the keychain cannot be written.
#[tauri::command]
pub async fn set_provider_secret(
    state: State<'_, AppState>,
    api_key_reference: String,
    api_key: String,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state
            .secret_store
            .set(&api_key_reference, &api_key)
        {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Delete a provider API key from the OS keychain.
///
/// # Errors
///
/// Returns an error response if the keychain entry cannot be removed.
#[tauri::command]
pub async fn delete_provider_secret(
    state: State<'_, AppState>,
    api_key_reference: String,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state
            .secret_store
            .delete(&api_key_reference)
        {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}
