//! Build `AutoAgents` LLM providers from Portico [`ProviderConfig`]s.

use crate::{AutoAgentsExecutor, tool_adapter::PorticoToolRegistry};
use app_models::{AppError, ProviderConfig, ProviderKind};
use app_runtime::ModelProviderRegistry;
use app_security::SecretStore;
use autoagents_llm::{LLMProvider, backends::openai, builder::LLMBuilder};
use std::sync::Arc;

const fn default_model_for_kind(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAI => "gpt-4.1-nano",
        ProviderKind::Anthropic => "claude-3-sonnet-20240229",
        // Defaults for other providers are best-effort; callers should register
        // explicit models for production use.
        ProviderKind::Moonshot => "kimi-k2-0711-preview",
        ProviderKind::DeepSeek => "deepseek-chat",
        ProviderKind::Google => "gemini-1.5-flash",
        ProviderKind::Groq => "llama3-70b-8192",
        ProviderKind::OpenRouter => "openai/gpt-4o-mini",
        ProviderKind::AzureOpenAI => "gpt-4o",
        ProviderKind::Ollama => "llama3",
        ProviderKind::Custom => "custom-model",
    }
}

const fn default_base_url_for_kind(kind: ProviderKind) -> Option<&'static str> {
    match kind {
        ProviderKind::Moonshot => Some("https://api.moonshot.cn/v1"),
        ProviderKind::DeepSeek => Some("https://api.deepseek.com/v1"),
        ProviderKind::Groq => Some("https://api.groq.com/openai/v1"),
        ProviderKind::OpenRouter => Some("https://openrouter.ai/api/v1"),
        ProviderKind::Ollama => Some("http://localhost:11434/v1"),
        // OpenAI uses the AutoAgents `OpenAI` backend's own default; Custom must
        // supply a base URL explicitly. Unsupported kinds are rejected before
        // this helper is consulted.
        ProviderKind::OpenAI
        | ProviderKind::Custom
        | ProviderKind::Anthropic
        | ProviderKind::Google
        | ProviderKind::AzureOpenAI => None,
    }
}

const fn is_openai_compatible(kind: ProviderKind) -> bool {
    matches!(
        kind,
        ProviderKind::OpenAI
            | ProviderKind::Moonshot
            | ProviderKind::DeepSeek
            | ProviderKind::Groq
            | ProviderKind::OpenRouter
            | ProviderKind::Ollama
            | ProviderKind::Custom
    )
}

const fn is_keyless(kind: ProviderKind) -> bool {
    matches!(kind, ProviderKind::Ollama)
}

/// Detects strings that are likely raw API keys rather than opaque references.
///
/// This is a best-effort backward-compatibility heuristic: users who previously
/// stored plaintext keys in `api_key_reference` will keep working, but we warn
/// them to migrate to the keychain.
fn looks_like_plaintext_key(reference: &str) -> bool {
    reference.starts_with("sk-")
        || reference.starts_with("gsk_")
        || reference.starts_with("sk-or-")
        || reference.starts_with("sk-ant-")
}

/// Resolve the real API key for a provider configuration.
///
/// 1. Look up `api_key_reference` in the configured [`SecretStore`].
/// 2. If not found and the reference looks like a plaintext key, use it
///    directly and warn.
/// 3. If not found and the provider does not require a key, return a harmless
///    placeholder (the `OpenAI` backend requires a non-empty key even though
///    Ollama ignores it).
fn resolve_api_key(
    config: &ProviderConfig,
    secret_store: &dyn SecretStore,
) -> Result<String, AppError> {
    let reference = &config.api_key_reference;

    if let Some(secret) = secret_store.get(reference)? {
        return Ok(secret);
    }

    if looks_like_plaintext_key(reference) {
        tracing::warn!(
            provider_kind = ?config.kind,
            api_key_reference = reference,
            "api_key_reference looks like a plaintext API key; using it directly. Store it in the keychain instead."
        );
        return Ok(reference.clone());
    }

    if is_keyless(config.kind) {
        return Ok("not-used".to_owned());
    }

    Err(AppError::Internal {
        message: format!(
            "no API key found for provider {:?} reference '{}'",
            config.kind, reference
        ),
    })
}

fn build_openai_compatible_provider(
    config: &ProviderConfig,
    model_name: Option<&str>,
    api_key: &str,
) -> Result<Arc<dyn LLMProvider>, AppError> {
    let timeout_seconds = config.timeout_ms / 1000;
    let base_url = config
        .base_url
        .clone()
        .or_else(|| default_base_url_for_kind(config.kind).map(std::borrow::ToOwned::to_owned));

    if config.kind == ProviderKind::Custom && base_url.is_none() {
        return Err(AppError::Internal {
            message: "Custom provider requires a base_url".to_owned(),
        });
    }

    let mut builder = LLMBuilder::<openai::OpenAI>::new()
        .api_key(api_key)
        .timeout_seconds(timeout_seconds);

    if let Some(model) = model_name {
        builder = builder.model(model);
    } else {
        builder = builder.model(default_model_for_kind(config.kind));
    }

    if let Some(url) = base_url {
        builder = builder.base_url(url);
    }

    let provider = builder.build().map_err(|e| AppError::Internal {
        message: format!("failed to build OpenAI-compatible provider: {e}"),
    })?;

    Ok(provider)
}

/// Build an [`LLMProvider`] from a Portico provider configuration.
///
/// # Errors
///
/// Returns an error if the provider kind is unsupported, a required API key or
/// base URL is missing, or the backend builder rejects the configuration.
pub fn build_llm_provider(
    config: &ProviderConfig,
    model_name: Option<&str>,
    secret_store: &dyn SecretStore,
) -> Result<Arc<dyn LLMProvider>, AppError> {
    if is_openai_compatible(config.kind) {
        let api_key = resolve_api_key(config, secret_store)?;
        return build_openai_compatible_provider(config, model_name, &api_key);
    }

    let timeout_seconds = config.timeout_ms / 1000;

    match config.kind {
        ProviderKind::Anthropic => {
            let api_key = resolve_api_key(config, secret_store)?;
            let mut builder = LLMBuilder::<autoagents_llm::backends::anthropic::Anthropic>::new()
                .api_key(api_key)
                .timeout_seconds(timeout_seconds);

            if let Some(model) = model_name {
                builder = builder.model(model);
            } else {
                builder = builder.model(default_model_for_kind(config.kind));
            }

            if let Some(base_url) = config.base_url.clone() {
                builder = builder.base_url(base_url);
            }

            let provider = builder.build().map_err(|e| AppError::Internal {
                message: format!("failed to build Anthropic provider: {e}"),
            })?;
            Ok(provider)
        }
        ProviderKind::Google => Err(AppError::Internal {
            message: "Google provider is not supported by the AutoAgents adapter yet. \
                Use a Custom provider with a Gemini-compatible base URL or route through OpenRouter."
                .to_owned(),
        }),
        ProviderKind::AzureOpenAI => Err(AppError::Internal {
            message: "AzureOpenAI provider is not supported by the AutoAgents adapter yet. \
                Use a Custom provider with an Azure OpenAI-compatible base URL or route through OpenRouter."
                .to_owned(),
        }),
        // OpenAI-compatible kinds are handled above.
        ProviderKind::OpenAI
        | ProviderKind::Moonshot
        | ProviderKind::DeepSeek
        | ProviderKind::Groq
        | ProviderKind::OpenRouter
        | ProviderKind::Ollama
        | ProviderKind::Custom => unreachable!("openai-compatible providers are handled above"),
    }
}

/// Build a real [`AutoAgentsExecutor`] from the first enabled provider in the
/// registry.
///
/// Returns `Ok(None)` when no providers are configured, allowing the runtime to
/// fall back to its mock executor until the user adds a provider.
///
/// # Errors
///
/// Returns an error if provider or model lookup fails, or if the selected
/// provider cannot be constructed.
pub async fn build_default_executor(
    registry: Arc<dyn ModelProviderRegistry>,
    tools: Arc<PorticoToolRegistry>,
    secret_store: Arc<dyn SecretStore>,
) -> Result<Option<AutoAgentsExecutor>, AppError> {
    let providers = registry.list_providers().await?;
    let provider_config = providers
        .iter()
        .find(|provider| provider.enabled)
        .or_else(|| providers.first())
        .cloned();

    let Some(provider_config) = provider_config else {
        return Ok(None);
    };

    let models = registry.list_models(Some(provider_config.id)).await?;
    let model_name = models.first().map(|model| model.model_name.as_str());

    let llm = build_llm_provider(&provider_config, model_name, secret_store.as_ref())?;
    Ok(Some(AutoAgentsExecutor::new(llm, tools)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_security::InMemorySecretStore;

    fn test_config(kind: ProviderKind, reference: &str, base_url: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            id: app_models::ProviderId::new(),
            kind,
            display_name: "Test".to_owned(),
            base_url: base_url.map(std::borrow::ToOwned::to_owned),
            api_key_reference: reference.to_owned(),
            organization_id: None,
            project_id: None,
            default_headers: std::collections::HashMap::new(),
            timeout_ms: 30_000,
            retry_policy: app_models::RetryPolicy::default(),
            fallback_provider_ids: Vec::new(),
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn openai_provider_resolves_key_from_secret_store() {
        let store = InMemorySecretStore::new();
        store.set("openai-ref", "sk-test").unwrap();

        let config = test_config(ProviderKind::OpenAI, "openai-ref", None);
        let provider = build_llm_provider(&config, None, &store).unwrap();

        assert_eq!(provider.model(), "gpt-4.1-nano");
    }

    #[test]
    fn openai_provider_falls_back_to_plaintext_key_with_warning() {
        let store = InMemorySecretStore::new();
        let config = test_config(ProviderKind::OpenAI, "sk-plaintext", None);

        let provider = build_llm_provider(&config, None, &store).unwrap();
        assert_eq!(provider.model(), "gpt-4.1-nano");
    }

    #[test]
    fn openai_provider_errors_when_key_missing_and_reference_is_opaque() {
        let store = InMemorySecretStore::new();
        let config = test_config(ProviderKind::OpenAI, "my-openai-key", None);

        let err = match build_llm_provider(&config, None, &store) {
            Err(err) => err,
            Ok(_) => panic!("expected provider construction to fail"),
        };
        assert!(
            err.to_string().contains("no API key found"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn moonshot_provider_uses_default_base_url_and_model() {
        let store = InMemorySecretStore::new();
        store.set("moonshot-ref", "sk-moon").unwrap();

        let config = test_config(ProviderKind::Moonshot, "moonshot-ref", None);
        let provider = build_llm_provider(&config, None, &store).unwrap();

        assert_eq!(provider.model(), "kimi-k2-0711-preview");
    }

    #[test]
    fn deepseek_provider_uses_default_base_url_and_model() {
        let store = InMemorySecretStore::new();
        store.set("deepseek-ref", "sk-ds").unwrap();

        let config = test_config(ProviderKind::DeepSeek, "deepseek-ref", None);
        let provider = build_llm_provider(&config, None, &store).unwrap();

        assert_eq!(provider.model(), "deepseek-chat");
    }

    #[test]
    fn ollama_provider_builds_without_key() {
        let store = InMemorySecretStore::new();
        let config = test_config(ProviderKind::Ollama, "", None);

        let provider = build_llm_provider(&config, None, &store).unwrap();
        assert_eq!(provider.model(), "llama3");
    }

    #[test]
    fn custom_provider_requires_base_url() {
        let store = InMemorySecretStore::new();
        let config = test_config(ProviderKind::Custom, "sk-custom", None);

        let err = match build_llm_provider(&config, None, &store) {
            Err(err) => err,
            Ok(_) => panic!("expected provider construction to fail"),
        };
        assert!(
            err.to_string().contains("Custom provider requires a base_url"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn custom_provider_builds_with_base_url() {
        let store = InMemorySecretStore::new();
        let config = test_config(
            ProviderKind::Custom,
            "sk-custom",
            Some("http://localhost:1234/v1"),
        );

        let provider = build_llm_provider(&config, None, &store).unwrap();
        assert_eq!(provider.model(), "custom-model");
    }

    #[test]
    fn google_and_azure_openai_are_unsupported() {
        let store = InMemorySecretStore::new();

        let google = test_config(ProviderKind::Google, "ref", None);
        let google_err = match build_llm_provider(&google, None, &store) {
            Err(err) => err,
            Ok(_) => panic!("expected Google provider to be unsupported"),
        };
        assert!(
            google_err.to_string().contains("not supported"),
            "unexpected error: {google_err}"
        );

        let azure = test_config(ProviderKind::AzureOpenAI, "ref", None);
        let azure_err = match build_llm_provider(&azure, None, &store) {
            Err(err) => err,
            Ok(_) => panic!("expected AzureOpenAI provider to be unsupported"),
        };
        assert!(
            azure_err.to_string().contains("not supported"),
            "unexpected error: {azure_err}"
        );
    }
}
