//! Provider secret management commands.
//!
//! Secrets are stored in the local AES-GCM vault under the app data directory
//! (with process-memory caching). Legacy Keychain entries are imported once and
//! then removed so macOS does not repeatedly prompt for keychain access.

use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Store a provider API key in the local encrypted secret vault.
///
/// # Errors
///
/// Returns an error response if the vault cannot be written.
#[tauri::command]
pub async fn set_provider_secret(
    state: State<'_, AppState>,
    api_key_reference: String,
    api_key: String,
) -> Result<ApiResponse<()>, String> {
    if let Err(reason) = validate_provider_secret(&api_key_reference, &api_key) {
        return Ok(ApiResponse::err(reason));
    }
    if let Err(err) = invalidate_provider_health(&state, &api_key_reference).await {
        return Ok(ApiResponse::err(err));
    }

    Ok(match state.secret_store.set(&api_key_reference, &api_key) {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Delete a provider API key from the local vault (and legacy Keychain if present).
///
/// # Errors
///
/// Returns an error response if the vault entry cannot be removed.
#[tauri::command]
pub async fn delete_provider_secret(
    state: State<'_, AppState>,
    api_key_reference: String,
) -> Result<ApiResponse<()>, String> {
    if let Err(reason) = validate_provider_secret_reference(&api_key_reference) {
        return Ok(ApiResponse::err(reason));
    }
    if let Err(err) = invalidate_provider_health(&state, &api_key_reference).await {
        return Ok(ApiResponse::err(err));
    }

    Ok(match state.secret_store.delete(&api_key_reference) {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

fn validate_provider_secret(reference: &str, value: &str) -> Result<(), String> {
    validate_provider_secret_reference(reference)?;
    if value.trim().is_empty() {
        return Err("provider secret must not be empty".to_owned());
    }
    if value.len() > 65_536 {
        return Err("provider secret exceeds the 65536-byte limit".to_owned());
    }
    if value.chars().any(char::is_control) {
        return Err("provider secret must not contain control characters".to_owned());
    }
    Ok(())
}

fn validate_provider_secret_reference(reference: &str) -> Result<(), String> {
    if reference.is_empty() || reference.len() > 256 {
        return Err("provider secret reference must contain 1 to 256 bytes".to_owned());
    }
    if !reference.chars().all(|character| character.is_ascii_graphic()) {
        return Err(
            "provider secret reference must contain printable ASCII without spaces".to_owned(),
        );
    }
    Ok(())
}

async fn invalidate_provider_health(
    state: &State<'_, AppState>,
    api_key_reference: &str,
) -> Result<(), String> {
    let providers =
        state.runtime.registry().list_providers().await.map_err(|err| err.to_string())?;

    for provider in providers
        .into_iter()
        .filter(|provider| provider.api_key_reference == api_key_reference)
    {
        state
            .runtime
            .registry()
            .invalidate_provider_health(provider.id)
            .await
            .map_err(|err| err.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_secret_boundary_accepts_opaque_non_empty_values() {
        assert!(validate_provider_secret("provider:openai-key", "sk-test_123").is_ok());
    }

    #[test]
    fn provider_secret_boundary_rejects_empty_multiline_and_oversized_values() {
        assert!(validate_provider_secret("", "key").is_err());
        assert!(validate_provider_secret("provider key", "key").is_err());
        assert!(validate_provider_secret("provider-key", "  ").is_err());
        assert!(validate_provider_secret("provider-key", "line1\nline2").is_err());
        assert!(validate_provider_secret("provider-key", &"x".repeat(65_537)).is_err());
    }
}
