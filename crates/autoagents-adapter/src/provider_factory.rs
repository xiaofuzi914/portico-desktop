//! Build `AutoAgents` LLM providers from Portico [`ProviderConfig`]s.

use crate::{AutoAgentsExecutor, tool_adapter::PorticoToolRegistry};
use app_models::{
    AgentRunId, AppError, ModelId, ProviderConfig, ProviderHealth, ProviderHealthStatus,
    ProviderId, ProviderKind, RunModelSnapshot, ThreadId, WorkspaceId,
};
use app_runtime::ModelProviderRegistry;
use app_runtime::{AgentExecutor, AgentExecutorResolver, ResolvedAgentExecutor};
use app_security::SecretStore;
use autoagents_llm::{LLMProvider, backends::openai, builder::LLMBuilder};
use std::sync::Arc;
use std::time::Duration;

const PROVIDER_HEALTH_TIMEOUT: Duration = Duration::from_secs(12);

const fn default_model_for_kind(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAI => "gpt-4.1-nano",
        ProviderKind::Anthropic => "claude-3-sonnet-20240229",
        // Defaults for other providers are best-effort; callers should register
        // explicit models for production use.
        ProviderKind::Moonshot => "kimi-k2-0711-preview",
        ProviderKind::DeepSeek => "deepseek-v4-pro",
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
        ProviderKind::DeepSeek => Some("https://api.deepseek.com"),
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

/// Resolve the real API key for a provider configuration.
///
/// 1. Look up `api_key_reference` in the configured [`SecretStore`].
/// 2. If not found and the provider does not require a key, return a harmless
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

    if is_keyless(config.kind) {
        return Ok("not-used".to_owned());
    }

    Err(AppError::Internal {
        message: "PROVIDER_SECRET_MISSING: provider credential was not found in the secure store"
            .to_owned(),
    })
}

fn build_openai_compatible_provider(
    config: &ProviderConfig,
    model_name: Option<&str>,
    api_key: &str,
) -> Result<Arc<dyn LLMProvider>, AppError> {
    // Floor at 60s so legacy 30s configs still work for tool-using agents even
    // before migration runs; prefer the configured value when higher.
    let timeout_seconds = (config.timeout_ms / 1000).max(60);
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

    if config.kind != ProviderKind::OpenAI {
        builder =
            builder.api_mode(autoagents_llm::backends::openai::OpenAIApiMode::ChatCompletions);
    }

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

    let timeout_seconds = (config.timeout_ms / 1000).max(60);

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

async fn probe_llm_provider(
    provider: &dyn LLMProvider,
    model_name: &str,
    timeout_duration: Duration,
) -> Result<(), AppError> {
    let message = autoagents_llm::chat::ChatMessage::user()
        .content(format!(
            "Connection check for model {model_name}. Reply with OK."
        ))
        .build();
    tokio::time::timeout(timeout_duration, provider.chat(&[message], None))
        .await
        .map_err(|_| AppError::Internal {
            message: "PROVIDER_HEALTH_TIMEOUT".to_owned(),
        })?
        .map(|_| ())
        .map_err(|error| AppError::Internal {
            message: format!("PROVIDER_HEALTH_FAILED: {error}"),
        })
}

fn safe_health_failure(
    provider_id: ProviderId,
    model_id: ModelId,
    error: &AppError,
) -> ProviderHealth {
    let lowered = error.to_string().to_ascii_lowercase();
    let (status, error_code, message) = if lowered.contains("credential")
        || lowered.contains("unauthorized")
        || lowered.contains("401")
        || lowered.contains("api key")
    {
        (
            ProviderHealthStatus::InvalidCredentials,
            "INVALID_CREDENTIALS",
            "Provider credentials were rejected or are missing.",
        )
    } else if lowered.contains("not supported") {
        (
            ProviderHealthStatus::Unsupported,
            "UNSUPPORTED_PROVIDER",
            "This provider is not supported by the current runtime.",
        )
    } else if lowered.contains("model_not_found") {
        (
            ProviderHealthStatus::Degraded,
            "MODEL_NOT_FOUND",
            "The selected model was not reported by the provider.",
        )
    } else if lowered.contains("429") || lowered.contains("rate limit") {
        (
            ProviderHealthStatus::Degraded,
            "RATE_LIMITED",
            "The provider rate limit was reached. Retry later.",
        )
    } else if lowered.contains("timeout") {
        (
            ProviderHealthStatus::Degraded,
            "HEALTH_TIMEOUT",
            "The provider health check timed out.",
        )
    } else {
        (
            ProviderHealthStatus::Degraded,
            "CONNECTION_FAILED",
            "The provider connection check failed.",
        )
    };
    ProviderHealth {
        provider_id,
        model_id,
        status,
        error_code: Some(error_code.to_owned()),
        message: Some(message.to_owned()),
        checked_at: chrono::Utc::now(),
    }
}

/// Build and probe one registered provider/model, persisting only a safe health summary.
///
/// # Errors
///
/// Returns an error if the provider/model relationship is invalid, registry
/// access fails, or the safe health summary cannot be persisted.
pub async fn check_provider_health(
    registry: Arc<dyn ModelProviderRegistry>,
    secret_store: Arc<dyn SecretStore>,
    provider_id: ProviderId,
    model_id: ModelId,
) -> Result<ProviderHealth, AppError> {
    let provider_config = registry.get_provider(provider_id).await?;
    let model = registry.get_model(model_id).await?;
    if model.provider_id != provider_id {
        return Err(AppError::PermissionDenied {
            reason: "health-check model does not belong to provider".to_owned(),
        });
    }
    let health = match build_llm_provider(
        &provider_config,
        Some(model.model_name.as_str()),
        secret_store.as_ref(),
    ) {
        Ok(provider) => {
            let configured_timeout = Duration::from_millis(provider_config.timeout_ms);
            let probe_timeout = PROVIDER_HEALTH_TIMEOUT
                .min(configured_timeout)
                .saturating_sub(Duration::from_millis(100))
                .max(Duration::from_millis(100));
            match probe_llm_provider(provider.as_ref(), &model.model_name, probe_timeout).await {
                Ok(()) => ProviderHealth {
                    provider_id,
                    model_id,
                    status: ProviderHealthStatus::Ready,
                    error_code: None,
                    message: Some("Provider and model are ready.".to_owned()),
                    checked_at: chrono::Utc::now(),
                },
                Err(error) => safe_health_failure(provider_id, model_id, &error),
            }
        }
        Err(error) => safe_health_failure(provider_id, model_id, &error),
    };
    registry.record_provider_health(health.clone()).await?;
    Ok(health)
}

/// Per-run executor resolver backed by the live provider/model registry.
pub struct RegistryExecutorResolver {
    registry: Arc<dyn ModelProviderRegistry>,
    tools: Arc<PorticoToolRegistry>,
    secret_store: Arc<dyn SecretStore>,
    security: Option<Arc<app_runtime::SecurityContext>>,
}

impl RegistryExecutorResolver {
    #[must_use]
    pub fn new(
        registry: Arc<dyn ModelProviderRegistry>,
        tools: Arc<PorticoToolRegistry>,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            registry,
            tools,
            secret_store,
            security: None,
        }
    }

    /// Use the application runtime's authoritative security context.
    #[must_use]
    pub fn with_security(mut self, security: Arc<app_runtime::SecurityContext>) -> Self {
        self.security = Some(security);
        self
    }
}

#[async_trait::async_trait]
impl AgentExecutorResolver for RegistryExecutorResolver {
    async fn resolve(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
        run_id: AgentRunId,
    ) -> Result<ResolvedAgentExecutor, AppError> {
        let selection = self.registry.resolve_active_model(workspace_id, thread_id).await?;
        let provider = self.registry.get_provider(selection.provider_id).await?;
        let model = self.registry.get_model(selection.model_id).await?;
        let health =
            self.registry.get_provider_health(provider.id, model.id).await?.ok_or_else(|| {
                AppError::Internal {
                message:
                    "PROVIDER_HEALTH_REQUIRED: test the selected provider connection before running"
                        .to_owned(),
            }
            })?;
        if health.status != ProviderHealthStatus::Ready
            || chrono::Utc::now().signed_duration_since(health.checked_at)
                > chrono::Duration::hours(24)
        {
            return Err(AppError::Internal {
                message: "PROVIDER_HEALTH_NOT_READY: re-test the selected provider connection"
                    .to_owned(),
            });
        }
        let llm = build_llm_provider(
            &provider,
            Some(model.model_name.as_str()),
            self.secret_store.as_ref(),
        )?;
        let mut executor = AutoAgentsExecutor::new(llm, Arc::clone(&self.tools));
        if let Some(security) = &self.security {
            executor = executor.with_security(security.clone());
        }
        let executor = Arc::new(executor) as Arc<dyn AgentExecutor>;
        Ok(ResolvedAgentExecutor {
            executor,
            snapshot: RunModelSnapshot {
                run_id,
                provider_id: provider.id,
                model_id: model.id,
                provider_name: provider.display_name,
                model_name: model.model_name,
                provider_config_updated_at: provider.updated_at,
                created_at: chrono::Utc::now(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockLlmProvider;
    use app_runtime::{SqliteModelProviderRegistry, SqliteStorage, Storage};
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
    fn openai_provider_rejects_plaintext_key_fallback() {
        let store = InMemorySecretStore::new();
        let config = test_config(ProviderKind::OpenAI, "sk-plaintext", None);

        let Err(error) = build_llm_provider(&config, None, &store) else {
            panic!("plaintext must fail");
        };
        assert!(error.to_string().contains("PROVIDER_SECRET_MISSING"));
        assert!(!error.to_string().contains("sk-plaintext"));
    }

    #[test]
    fn openai_provider_errors_when_key_missing_and_reference_is_opaque() {
        let store = InMemorySecretStore::new();
        let config = test_config(ProviderKind::OpenAI, "my-openai-key", None);

        let Err(err) = build_llm_provider(&config, None, &store) else {
            panic!("expected provider construction to fail");
        };
        assert!(
            err.to_string().contains("PROVIDER_SECRET_MISSING"),
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

        assert_eq!(provider.model(), "deepseek-v4-pro");
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
        store.set("custom-ref", "sk-custom").unwrap();
        let config = test_config(ProviderKind::Custom, "custom-ref", None);

        let Err(err) = build_llm_provider(&config, None, &store) else {
            panic!("expected provider construction to fail");
        };
        assert!(
            err.to_string().contains("Custom provider requires a base_url"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn custom_provider_builds_with_base_url() {
        let store = InMemorySecretStore::new();
        store.set("custom-ref", "sk-custom").unwrap();
        let config = test_config(
            ProviderKind::Custom,
            "custom-ref",
            Some("http://localhost:1234/v1"),
        );

        let provider = build_llm_provider(&config, None, &store).unwrap();
        assert_eq!(provider.model(), "custom-model");
    }

    #[test]
    fn google_and_azure_openai_are_unsupported() {
        let store = InMemorySecretStore::new();

        let google = test_config(ProviderKind::Google, "ref", None);
        let Err(google_err) = build_llm_provider(&google, None, &store) else {
            panic!("expected Google provider to be unsupported");
        };
        assert!(
            google_err.to_string().contains("not supported"),
            "unexpected error: {google_err}"
        );

        let azure = test_config(ProviderKind::AzureOpenAI, "ref", None);
        let Err(azure_err) = build_llm_provider(&azure, None, &store) else {
            panic!("expected AzureOpenAI provider to be unsupported");
        };
        assert!(
            azure_err.to_string().contains("not supported"),
            "unexpected error: {azure_err}"
        );
    }

    #[tokio::test]
    async fn mock_provider_passes_the_bounded_health_probe() {
        let provider = MockLlmProvider::new();
        probe_llm_provider(&provider, "fixture-model", PROVIDER_HEALTH_TIMEOUT)
            .await
            .expect("health probe");
    }

    #[test]
    fn health_failures_are_sanitized() {
        let health = safe_health_failure(
            ProviderId::new(),
            ModelId::new(),
            &AppError::Internal {
                message: format!("401 secret sk-{}", "never-persist-this-value"),
            },
        );
        assert_eq!(health.status, ProviderHealthStatus::InvalidCredentials);
        assert!(!health.message.unwrap_or_default().contains("sk-never"));
    }

    #[tokio::test]
    async fn registry_resolver_uses_active_ready_model_and_returns_run_snapshot() {
        std::fs::create_dir_all("/tmp/portico-resolver-selection-test").expect("test root");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let registry: Arc<dyn ModelProviderRegistry> =
            Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
        let workspace = storage
            .create_workspace("test", "/tmp/portico-resolver-selection-test", false)
            .await
            .expect("workspace");
        let thread = storage.create_thread(workspace.id, "thread").await.expect("thread");
        let run = storage.create_run(workspace.id, thread.id).await.expect("run");
        let provider = registry
            .create_provider(ProviderKind::OpenAI, "OpenAI", None, "openai-ref")
            .await
            .expect("provider");
        let model = registry
            .add_model(
                provider.id,
                "selected-model",
                "Selected",
                app_models::ModelCapability {
                    supports_streaming: true,
                    supports_tools: false,
                    supports_json_schema: false,
                    supports_vision: false,
                    supports_pdf: false,
                    supports_system_prompt: true,
                    supports_embeddings: false,
                    max_context_tokens: Some(32_000),
                    input_price_per_1k: None,
                    output_price_per_1k: None,
                },
            )
            .await
            .expect("model");
        registry
            .set_active_model(
                app_models::ModelSelectionScope::Global,
                None,
                None,
                provider.id,
                model.id,
            )
            .await
            .expect("select");
        registry
            .record_provider_health(ProviderHealth {
                provider_id: provider.id,
                model_id: model.id,
                status: ProviderHealthStatus::Ready,
                error_code: None,
                message: Some("ready".to_owned()),
                checked_at: chrono::Utc::now(),
            })
            .await
            .expect("health");
        let secrets = Arc::new(InMemorySecretStore::new());
        secrets.set("openai-ref", "sk-test").expect("secret");
        let executor_resolver =
            RegistryExecutorResolver::new(registry, Arc::new(PorticoToolRegistry::new()), secrets);

        let resolved_executor = executor_resolver
            .resolve(workspace.id, thread.id, run.id)
            .await
            .expect("resolve");
        assert_eq!(resolved_executor.snapshot.run_id, run.id);
        assert_eq!(resolved_executor.snapshot.provider_id, provider.id);
        assert_eq!(resolved_executor.snapshot.model_id, model.id);
        assert_eq!(resolved_executor.snapshot.model_name, "selected-model");
    }
}
