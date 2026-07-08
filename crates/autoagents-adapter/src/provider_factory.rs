//! Build `AutoAgents` LLM providers from Portico [`ProviderConfig`]s.

use crate::{AutoAgentsExecutor, tool_adapter::PorticoToolRegistry};
use app_models::{AppError, ProviderConfig, ProviderKind};
use app_runtime::ModelProviderRegistry;
use autoagents_llm::{LLMProvider, builder::LLMBuilder};
use std::sync::Arc;

const fn default_model_for_kind(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAI => "gpt-4.1-nano",
        ProviderKind::Anthropic => "claude-3-sonnet-20240229",
        // Defaults for other providers are best-effort; callers should register
        // explicit models for production use.
        ProviderKind::DeepSeek => "deepseek-chat",
        ProviderKind::Google => "gemini-1.5-flash",
        ProviderKind::Groq => "llama3-70b-8192",
        ProviderKind::OpenRouter => "openai/gpt-4o-mini",
        ProviderKind::AzureOpenAI => "gpt-4o",
        ProviderKind::Ollama => "llama3",
        ProviderKind::Custom => "custom-model",
    }
}

/// Build an [`LLMProvider`] from a Portico provider configuration.
///
/// # Errors
///
/// Returns an error if the provider kind is unsupported or the backend builder
/// rejects the configuration (e.g. a missing API key).
pub fn build_llm_provider(
    config: &ProviderConfig,
    model_name: Option<&str>,
) -> Result<Arc<dyn LLMProvider>, AppError> {
    let timeout_seconds = config.timeout_ms / 1000;

    match config.kind {
        ProviderKind::OpenAI => {
            let mut builder = LLMBuilder::<autoagents_llm::backends::openai::OpenAI>::new()
                .api_key(config.api_key_reference.clone())
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
                message: format!("failed to build OpenAI provider: {e}"),
            })?;
            Ok(provider)
        }
        ProviderKind::Anthropic => {
            let mut builder = LLMBuilder::<autoagents_llm::backends::anthropic::Anthropic>::new()
                .api_key(config.api_key_reference.clone())
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
        _ => Err(AppError::Internal {
            message: format!(
                "AutoAgents adapter does not support provider kind {:?}",
                config.kind
            ),
        }),
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

    let llm = build_llm_provider(&provider_config, model_name)?;
    Ok(Some(AutoAgentsExecutor::new(llm, tools)))
}
