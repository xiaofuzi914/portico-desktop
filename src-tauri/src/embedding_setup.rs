//! Embedding provider selection and construction for the Tauri backend.

use app_memory::{
    EmbeddingProvider, HashEmbeddingProvider, OllamaEmbeddingProvider, OpenAiCompatEmbeddingProvider,
};
use app_models::{ProviderConfig, ProviderKind};
use app_runtime::ModelProviderRegistry;
use std::sync::Arc;

/// Known expected embedding dimensions for common models.
fn known_dimension(provider_kind: ProviderKind, model: &str) -> usize {
    match provider_kind {
        ProviderKind::OpenAI => {
            if model == "text-embedding-3-large" {
                3072
            } else {
                1536
            }
        }
        ProviderKind::Ollama => {
            if model == "mxbai-embed-large" || model == "snowflake-arctic-embed" {
                1024
            } else {
                768
            }
        }
        _ => 1536,
    }
}

/// Build the embedding provider to use for RAG.
///
/// Selection order:
/// 1. Enabled `Ollama` provider.
/// 2. Any enabled `OpenAI`-compatible provider (`OpenAI`, `Moonshot`, `DeepSeek`, etc.).
/// 3. Hash-based deterministic fallback.
///
/// # Errors
///
/// Returns an error only if a configured provider fails to initialize; missing
/// secrets result in a fallback to the hash provider with a warning.
pub async fn build_embedding_provider(
    registry: Arc<dyn ModelProviderRegistry>,
    secret_store: &Arc<dyn app_security::SecretStore>,
) -> Result<Arc<dyn EmbeddingProvider>, app_models::AppError> {
    let providers = registry.list_providers().await?;
    let enabled: Vec<&ProviderConfig> = providers.iter().filter(|p| p.enabled).collect();

    // 1. Prefer Ollama if configured and enabled.
    if let Some(ollama) = enabled.iter().find(|p| p.kind == ProviderKind::Ollama) {
        let model = embedding_model_for_provider(registry.clone(), ollama, "nomic-embed-text")
            .await
            .unwrap_or_else(|| "nomic-embed-text".to_owned());
        let dimension = known_dimension(ProviderKind::Ollama, &model);
        let base_url = ollama
            .base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_owned());

        match OllamaEmbeddingProvider::new("ollama", base_url, model, dimension) {
            Ok(provider) => return Ok(Arc::new(provider)),
            Err(err) => {
                tracing::warn!(error = %err, "failed to create Ollama embedding provider, falling back");
            }
        }
    }

    // 2. Fall back to any enabled OpenAI-compatible provider.
    let openai_compat_kinds = [
        ProviderKind::OpenAI,
        ProviderKind::Moonshot,
        ProviderKind::DeepSeek,
        ProviderKind::Groq,
        ProviderKind::OpenRouter,
        ProviderKind::AzureOpenAI,
        ProviderKind::Custom,
    ];

    if let Some(provider) = enabled.iter().find(|p| openai_compat_kinds.contains(&p.kind)) {
        let (model, dimension) = if provider.kind == ProviderKind::OpenAI {
            ("text-embedding-3-small".to_owned(), 1536usize)
        } else {
            let model = embedding_model_for_provider(
                registry.clone(),
                provider,
                "text-embedding-3-small",
            )
            .await;
            if let Some(model) = model {
                let dimension = known_dimension(provider.kind, &model);
                (model, dimension)
            } else {
                tracing::warn!(
                    provider_id = ?provider.id,
                    "no embedding-capable model found for OpenAI-compatible provider; falling back to hash embedding"
                );
                return Ok(Arc::new(HashEmbeddingProvider::new()));
            }
        };

        let Some(api_key) = resolve_api_key(secret_store, &provider.api_key_reference) else {
            tracing::warn!(
                provider_id = ?provider.id,
                api_key_reference = %provider.api_key_reference,
                "no API key found for OpenAI-compatible provider; falling back to hash embedding"
            );
            return Ok(Arc::new(HashEmbeddingProvider::new()));
        };

        let base_url = provider
            .base_url
            .clone()
            .unwrap_or_else(|| default_openai_base_url(provider.kind));
        let provider_id = format!("{}-{}", provider.kind.as_str(), provider.id.0);

        match OpenAiCompatEmbeddingProvider::new(provider_id, base_url, model, api_key, dimension)
        {
            Ok(openai) => return Ok(Arc::new(openai)),
            Err(err) => {
                tracing::warn!(error = %err, "failed to create OpenAI-compatible embedding provider, falling back");
            }
        }
    }

    // 3. Final fallback.
    Ok(Arc::new(HashEmbeddingProvider::new()))
}

async fn embedding_model_for_provider(
    registry: Arc<dyn ModelProviderRegistry>,
    provider: &ProviderConfig,
    fallback: &str,
) -> Option<String> {
    match registry.list_models(Some(provider.id)).await {
        Ok(models) => models
            .into_iter()
            .find(|m| m.capabilities.supports_embeddings)
            .map(|m| m.model_name),
        Err(err) => {
            tracing::warn!(error = %err, provider_id = ?provider.id, "failed to list models for embedding provider");
            Some(fallback.to_owned())
        }
    }
}

fn resolve_api_key(
    secret_store: &Arc<dyn app_security::SecretStore>,
    api_key_reference: &str,
) -> Option<String> {
    match secret_store.get(api_key_reference) {
        Ok(Some(key)) => Some(key),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(error = %err, api_key_reference, "failed to read API key from secret store");
            None
        }
    }
}

fn default_openai_base_url(kind: ProviderKind) -> String {
    match kind {
        ProviderKind::Moonshot => "https://api.moonshot.cn/v1".to_owned(),
        ProviderKind::DeepSeek => "https://api.deepseek.com/v1".to_owned(),
        ProviderKind::Groq => "https://api.groq.com/openai/v1".to_owned(),
        ProviderKind::OpenRouter => "https://openrouter.ai/api/v1".to_owned(),
        _ => "https://api.openai.com/v1".to_owned(),
    }
}
