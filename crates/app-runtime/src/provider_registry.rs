//! Model provider registry, capability matrix, and usage cost guard.

use app_models::{
    AgentRunId, AppError, ModelCapability, ModelId, ModelInfo, ProviderConfig, ProviderId,
    ProviderKind, RetryPolicy, UsageBudget, UsageRecord,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json;
use sqlx::{FromRow, SqlitePool, sqlite::SqlitePoolOptions};
use std::collections::HashMap;

/// Registry of model providers, their models, and usage-based cost guards.
#[async_trait]
pub trait ModelProviderRegistry: Send + Sync {
    /// Register a new provider configuration.
    async fn create_provider(
        &self,
        kind: ProviderKind,
        display_name: &str,
        base_url: Option<&str>,
        api_key_reference: &str,
    ) -> Result<ProviderConfig, AppError>;

    /// List all provider configurations.
    async fn list_providers(&self) -> Result<Vec<ProviderConfig>, AppError>;

    /// Fetch a provider configuration by id.
    async fn get_provider(&self, id: ProviderId) -> Result<ProviderConfig, AppError>;

    /// Update an existing provider configuration.
    async fn update_provider(&self, config: ProviderConfig) -> Result<(), AppError>;

    /// Delete a provider and its registered models.
    async fn delete_provider(&self, id: ProviderId) -> Result<(), AppError>;

    /// Register a model under a provider.
    async fn add_model(
        &self,
        provider_id: ProviderId,
        model_name: &str,
        display_name: &str,
        capabilities: ModelCapability,
    ) -> Result<ModelInfo, AppError>;

    /// List models, optionally filtered to a single provider.
    async fn list_models(
        &self,
        provider_id: Option<ProviderId>,
    ) -> Result<Vec<ModelInfo>, AppError>;

    /// Fetch a model by id.
    async fn get_model(&self, id: ModelId) -> Result<ModelInfo, AppError>;

    /// Delete a model by id.
    async fn delete_model(&self, id: ModelId) -> Result<(), AppError>;

    /// Persist a usage record.
    async fn record_usage(&self, record: UsageRecord) -> Result<(), AppError>;

    /// Enforce the app-level usage budget for a provider/run.
    async fn check_budget(
        &self,
        provider_id: ProviderId,
        run_id: AgentRunId,
        estimated_cost_usd: f64,
    ) -> Result<(), AppError>;

    /// Return today's total usage (USD) for a provider.
    async fn get_daily_usage(&self, provider_id: ProviderId) -> Result<f64, AppError>;

    /// Return today's total usage (USD) across all providers.
    async fn get_total_daily_usage(&self) -> Result<f64, AppError>;

    /// Return the configured app-level usage budget.
    fn budget(&self) -> UsageBudget;
}

/// SQLite-backed implementation of [`ModelProviderRegistry`].
#[derive(Debug, Clone)]
pub struct SqliteModelProviderRegistry {
    pool: SqlitePool,
    budget: UsageBudget,
}

impl SqliteModelProviderRegistry {
    /// Create a new registry around an existing `SQLite` pool.
    ///
    /// Uses the default app-level budget (per-run 1.0 USD, daily 10.0 USD).
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            budget: UsageBudget::default(),
        }
    }

    /// Create a new registry with a custom app-level budget.
    #[must_use]
    pub const fn with_budget(pool: SqlitePool, budget: UsageBudget) -> Self {
        Self { pool, budget }
    }

    /// Create an in-memory registry for tests.
    ///
    /// # Errors
    ///
    /// Returns an error if the in-memory database cannot be set up.
    pub async fn open_in_memory() -> Result<Self, AppError> {
        let pool =
            SqlitePoolOptions::new()
                .connect(":memory:")
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("failed to connect to in-memory sqlite: {e}"),
                })?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("migration failed: {e}"),
            })?;

        Ok(Self::new(pool))
    }
}

#[async_trait]
impl ModelProviderRegistry for SqliteModelProviderRegistry {
    async fn create_provider(
        &self,
        kind: ProviderKind,
        display_name: &str,
        base_url: Option<&str>,
        api_key_reference: &str,
    ) -> Result<ProviderConfig, AppError> {
        let id = ProviderId::new();
        let now = Utc::now();
        let base_url = base_url.map(std::borrow::ToOwned::to_owned);

        let config = ProviderConfig {
            id,
            kind,
            display_name: display_name.to_owned(),
            base_url,
            api_key_reference: api_key_reference.to_owned(),
            organization_id: None,
            project_id: None,
            default_headers: HashMap::new(),
            timeout_ms: 30_000,
            retry_policy: RetryPolicy::default(),
            fallback_provider_ids: Vec::new(),
            enabled: true,
            created_at: now,
            updated_at: now,
        };

        let default_headers_json =
            serde_json::to_string(&config.default_headers).map_err(|e| AppError::Internal {
                message: format!("serialize default_headers failed: {e}"),
            })?;
        let retry_policy_json =
            serde_json::to_string(&config.retry_policy).map_err(|e| AppError::Internal {
                message: format!("serialize retry_policy failed: {e}"),
            })?;
        let fallback_ids_json = serde_json::to_string(
            &config.fallback_provider_ids.iter().map(|pid| pid.0).collect::<Vec<_>>(),
        )
        .map_err(|e| AppError::Internal {
            message: format!("serialize fallback_provider_ids failed: {e}"),
        })?;

        sqlx::query(
            "INSERT INTO provider_configs (
                id, kind, display_name, base_url, api_key_reference,
                organization_id, project_id, default_headers, timeout_ms,
                retry_policy, fallback_provider_ids, enabled, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(config.id.0)
        .bind(config.kind.as_str())
        .bind(&config.display_name)
        .bind(&config.base_url)
        .bind(&config.api_key_reference)
        .bind(&config.organization_id)
        .bind(&config.project_id)
        .bind(default_headers_json)
        .bind(i64::try_from(config.timeout_ms).unwrap_or(30_000))
        .bind(retry_policy_json)
        .bind(fallback_ids_json)
        .bind(i64::from(config.enabled))
        .bind(config.created_at)
        .bind(config.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create_provider failed: {e}"),
        })?;

        Ok(config)
    }

    async fn list_providers(&self) -> Result<Vec<ProviderConfig>, AppError> {
        let rows = sqlx::query_as::<_, ProviderConfigRow>(
            "SELECT id, kind, display_name, base_url, api_key_reference,
                    organization_id, project_id, default_headers, timeout_ms,
                    retry_policy, fallback_provider_ids, enabled, created_at, updated_at
             FROM provider_configs
             ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list_providers failed: {e}"),
        })?;

        rows.into_iter().map(ProviderConfigRow::try_into).collect()
    }

    async fn get_provider(&self, id: ProviderId) -> Result<ProviderConfig, AppError> {
        let row = sqlx::query_as::<_, ProviderConfigRow>(
            "SELECT id, kind, display_name, base_url, api_key_reference,
                    organization_id, project_id, default_headers, timeout_ms,
                    retry_policy, fallback_provider_ids, enabled, created_at, updated_at
             FROM provider_configs
             WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_provider failed: {e}"),
        })?;

        row.ok_or_else(|| AppError::NotFound {
            resource: format!("provider {id:?}"),
        })?
        .try_into()
    }

    async fn update_provider(&self, config: ProviderConfig) -> Result<(), AppError> {
        let default_headers_json =
            serde_json::to_string(&config.default_headers).map_err(|e| AppError::Internal {
                message: format!("serialize default_headers failed: {e}"),
            })?;
        let retry_policy_json =
            serde_json::to_string(&config.retry_policy).map_err(|e| AppError::Internal {
                message: format!("serialize retry_policy failed: {e}"),
            })?;
        let fallback_ids_json = serde_json::to_string(
            &config.fallback_provider_ids.iter().map(|pid| pid.0).collect::<Vec<_>>(),
        )
        .map_err(|e| AppError::Internal {
            message: format!("serialize fallback_provider_ids failed: {e}"),
        })?;

        sqlx::query(
            "UPDATE provider_configs SET
                kind = ?, display_name = ?, base_url = ?, api_key_reference = ?,
                organization_id = ?, project_id = ?, default_headers = ?, timeout_ms = ?,
                retry_policy = ?, fallback_provider_ids = ?, enabled = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(config.kind.as_str())
        .bind(&config.display_name)
        .bind(&config.base_url)
        .bind(&config.api_key_reference)
        .bind(&config.organization_id)
        .bind(&config.project_id)
        .bind(default_headers_json)
        .bind(i64::try_from(config.timeout_ms).unwrap_or(30_000))
        .bind(retry_policy_json)
        .bind(fallback_ids_json)
        .bind(i64::from(config.enabled))
        .bind(Utc::now())
        .bind(config.id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("update_provider failed: {e}"),
        })?;

        Ok(())
    }

    async fn delete_provider(&self, id: ProviderId) -> Result<(), AppError> {
        sqlx::query("DELETE FROM provider_configs WHERE id = ?")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("delete_provider failed: {e}"),
            })?;
        Ok(())
    }

    async fn add_model(
        &self,
        provider_id: ProviderId,
        model_name: &str,
        display_name: &str,
        capabilities: ModelCapability,
    ) -> Result<ModelInfo, AppError> {
        let provider = self.get_provider(provider_id).await?;
        let id = ModelId::new();

        let model = ModelInfo {
            id,
            provider_id,
            provider_name: provider.display_name,
            model_name: model_name.to_owned(),
            display_name: display_name.to_owned(),
            capabilities,
        };

        let capabilities_json =
            serde_json::to_string(&model.capabilities).map_err(|e| AppError::Internal {
                message: format!("serialize capabilities failed: {e}"),
            })?;

        sqlx::query(
            "INSERT INTO model_infos (id, provider_id, provider_name, model_name, display_name, capabilities)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(model.id.0)
        .bind(model.provider_id.0)
        .bind(&model.provider_name)
        .bind(&model.model_name)
        .bind(&model.display_name)
        .bind(capabilities_json)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("add_model failed: {e}"),
        })?;

        Ok(model)
    }

    async fn list_models(
        &self,
        provider_id: Option<ProviderId>,
    ) -> Result<Vec<ModelInfo>, AppError> {
        let rows = if let Some(provider_id) = provider_id {
            sqlx::query_as::<_, ModelInfoRow>(
                "SELECT id, provider_id, provider_name, model_name, display_name, capabilities
                 FROM model_infos
                 WHERE provider_id = ?
                 ORDER BY display_name ASC",
            )
            .bind(provider_id.0)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, ModelInfoRow>(
                "SELECT id, provider_id, provider_name, model_name, display_name, capabilities
                 FROM model_infos
                 ORDER BY display_name ASC",
            )
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| AppError::Internal {
            message: format!("list_models failed: {e}"),
        })?;

        rows.into_iter().map(ModelInfoRow::try_into).collect()
    }

    async fn get_model(&self, id: ModelId) -> Result<ModelInfo, AppError> {
        let row = sqlx::query_as::<_, ModelInfoRow>(
            "SELECT id, provider_id, provider_name, model_name, display_name, capabilities
             FROM model_infos
             WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_model failed: {e}"),
        })?;

        row.ok_or_else(|| AppError::NotFound {
            resource: format!("model {id:?}"),
        })?
        .try_into()
    }

    async fn delete_model(&self, id: ModelId) -> Result<(), AppError> {
        sqlx::query("DELETE FROM model_infos WHERE id = ?")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("delete_model failed: {e}"),
            })?;
        Ok(())
    }

    async fn record_usage(&self, record: UsageRecord) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO usage_records (run_id, provider_id, model_id, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(record.run_id.0)
        .bind(record.provider_id.0)
        .bind(record.model_id.0)
        .bind(i64::try_from(record.input_tokens).unwrap_or(0))
        .bind(i64::try_from(record.output_tokens).unwrap_or(0))
        .bind(record.cost_usd)
        .bind(record.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("record_usage failed: {e}"),
        })?;
        Ok(())
    }

    async fn check_budget(
        &self,
        provider_id: ProviderId,
        _run_id: AgentRunId,
        estimated_cost_usd: f64,
    ) -> Result<(), AppError> {
        if estimated_cost_usd < 0.0 {
            return Err(AppError::PermissionDenied {
                reason: "estimated cost cannot be negative".to_owned(),
            });
        }

        if let Some(per_run) = self.budget.per_run_usd {
            if estimated_cost_usd > per_run {
                return Err(AppError::PermissionDenied {
                    reason: format!(
                        "estimated cost {estimated_cost_usd:.6} exceeds per-run budget {per_run:.2}"
                    ),
                });
            }
        }

        let daily_usage = self.get_daily_usage(provider_id).await?;
        if let Some(daily) = self.budget.daily_usd {
            if estimated_cost_usd + daily_usage > daily {
                return Err(AppError::PermissionDenied {
                    reason: format!(
                        "estimated cost {estimated_cost_usd:.6} + daily usage {daily_usage:.6} exceeds daily budget {daily:.2}"
                    ),
                });
            }
        }

        Ok(())
    }

    async fn get_daily_usage(&self, provider_id: ProviderId) -> Result<f64, AppError> {
        let day_start = day_start()?;
        let total: Option<f64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_records WHERE provider_id = ? AND created_at >= ?",
        )
        .bind(provider_id.0)
        .bind(day_start)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_daily_usage failed: {e}"),
        })?;

        Ok(total.unwrap_or(0.0))
    }

    async fn get_total_daily_usage(&self) -> Result<f64, AppError> {
        let day_start = day_start()?;
        let total: Option<f64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_records WHERE created_at >= ?",
        )
        .bind(day_start)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_total_daily_usage failed: {e}"),
        })?;

        Ok(total.unwrap_or(0.0))
    }

    fn budget(&self) -> UsageBudget {
        self.budget
    }
}

/// Return the start of the current UTC day as a `DateTime<Utc>`.
fn day_start() -> Result<DateTime<Utc>, AppError> {
    let today = Utc::now().date_naive();
    let naive = today.and_hms_opt(0, 0, 0).ok_or_else(|| AppError::Internal {
        message: "failed to compute day start".to_owned(),
    })?;
    Ok(DateTime::from_naive_utc_and_offset(naive, Utc))
}

#[derive(FromRow)]
struct ProviderConfigRow {
    id: uuid::Uuid,
    kind: String,
    display_name: String,
    base_url: Option<String>,
    api_key_reference: String,
    organization_id: Option<String>,
    project_id: Option<String>,
    default_headers: String,
    timeout_ms: i64,
    retry_policy: String,
    fallback_provider_ids: String,
    enabled: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<ProviderConfigRow> for ProviderConfig {
    type Error = AppError;

    fn try_from(row: ProviderConfigRow) -> Result<Self, Self::Error> {
        let default_headers: HashMap<String, String> = serde_json::from_str(&row.default_headers)
            .map_err(|e| AppError::Internal {
            message: format!("deserialize default_headers failed: {e}"),
        })?;
        let retry_policy: RetryPolicy =
            serde_json::from_str(&row.retry_policy).map_err(|e| AppError::Internal {
                message: format!("deserialize retry_policy failed: {e}"),
            })?;
        let fallback_uuids: Vec<uuid::Uuid> = serde_json::from_str(&row.fallback_provider_ids)
            .map_err(|e| AppError::Internal {
                message: format!("deserialize fallback_provider_ids failed: {e}"),
            })?;

        Ok(Self {
            id: ProviderId(row.id),
            kind: row.kind.as_str().try_into()?,
            display_name: row.display_name,
            base_url: row.base_url,
            api_key_reference: row.api_key_reference,
            organization_id: row.organization_id,
            project_id: row.project_id,
            default_headers,
            timeout_ms: u64::try_from(row.timeout_ms).unwrap_or(30_000),
            retry_policy,
            fallback_provider_ids: fallback_uuids.into_iter().map(ProviderId).collect(),
            enabled: row.enabled != 0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(FromRow)]
struct ModelInfoRow {
    id: uuid::Uuid,
    provider_id: uuid::Uuid,
    provider_name: String,
    model_name: String,
    display_name: String,
    capabilities: String,
}

impl TryFrom<ModelInfoRow> for ModelInfo {
    type Error = AppError;

    fn try_from(row: ModelInfoRow) -> Result<Self, Self::Error> {
        let capabilities: ModelCapability =
            serde_json::from_str(&row.capabilities).map_err(|e| AppError::Internal {
                message: format!("deserialize capabilities failed: {e}"),
            })?;

        Ok(Self {
            id: ModelId(row.id),
            provider_id: ProviderId(row.provider_id),
            provider_name: row.provider_name,
            model_name: row.model_name,
            display_name: row.display_name,
            capabilities,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup() -> SqliteModelProviderRegistry {
        SqliteModelProviderRegistry::open_in_memory().await.expect("open registry")
    }

    #[tokio::test]
    async fn provider_crud_works() {
        let registry = setup().await;

        let provider = registry
            .create_provider(
                ProviderKind::OpenAI,
                "OpenAI",
                Some("https://api.openai.com"),
                "keyref-1",
            )
            .await
            .expect("create provider");
        assert_eq!(provider.kind, ProviderKind::OpenAI);
        assert_eq!(provider.display_name, "OpenAI");
        assert!(provider.enabled);

        let providers = registry.list_providers().await.expect("list providers");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, provider.id);

        let fetched = registry.get_provider(provider.id).await.expect("get provider");
        assert_eq!(fetched.api_key_reference, "keyref-1");

        let mut updated = fetched;
        updated.display_name = "OpenAI Updated".to_owned();
        updated.timeout_ms = 60_000;
        registry.update_provider(updated).await.expect("update provider");

        let after_update = registry.get_provider(provider.id).await.expect("get after update");
        assert_eq!(after_update.display_name, "OpenAI Updated");
        assert_eq!(after_update.timeout_ms, 60_000);

        registry.delete_provider(provider.id).await.expect("delete provider");
        let remaining = registry.list_providers().await.expect("list after delete");
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn model_crud_works() {
        let registry = setup().await;
        let provider = registry
            .create_provider(ProviderKind::Anthropic, "Anthropic", None, "keyref-2")
            .await
            .expect("create provider");

        let capabilities = ModelCapability {
            supports_streaming: true,
            supports_tools: true,
            supports_json_schema: true,
            supports_vision: false,
            supports_pdf: false,
            supports_system_prompt: true,
            supports_embeddings: false,
            max_context_tokens: Some(200_000),
            input_price_per_1k: Some(3.0),
            output_price_per_1k: Some(15.0),
        };

        let model = registry
            .add_model(provider.id, "claude-3-opus", "Claude 3 Opus", capabilities)
            .await
            .expect("add model");
        assert_eq!(model.provider_id, provider.id);
        assert_eq!(model.provider_name, "Anthropic");

        let models = registry.list_models(Some(provider.id)).await.expect("list models");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, model.id);

        let fetched = registry.get_model(model.id).await.expect("get model");
        assert!(fetched.capabilities.supports_tools);

        registry.delete_model(model.id).await.expect("delete model");
        let remaining = registry.list_models(None).await.expect("list after delete");
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn budget_guard_blocks_excess_costs() {
        let registry = setup().await;
        let provider = registry
            .create_provider(ProviderKind::Groq, "Groq", None, "keyref-3")
            .await
            .expect("create provider");
        let run_id = AgentRunId::new();

        // Default budget: per_run 1.0, daily 10.0.
        registry.check_budget(provider.id, run_id, 0.5).await.expect("within budget");

        let err = registry
            .check_budget(provider.id, run_id, 1.5)
            .await
            .expect_err("exceeds per-run budget");
        assert!(matches!(err, AppError::PermissionDenied { .. }));

        // Record enough usage to push daily total near the limit.
        let model_id = ModelId::new();
        registry
            .record_usage(UsageRecord {
                id: 0,
                run_id,
                provider_id: provider.id,
                model_id,
                input_tokens: 0,
                output_tokens: 0,
                cost_usd: 9.6,
                created_at: Utc::now(),
            })
            .await
            .expect("record usage");

        let err = registry
            .check_budget(provider.id, run_id, 0.5)
            .await
            .expect_err("exceeds daily budget");
        assert!(matches!(err, AppError::PermissionDenied { .. }));
    }

    #[tokio::test]
    async fn daily_usage_aggregates_for_provider() {
        let registry = setup().await;
        let provider = registry
            .create_provider(ProviderKind::OpenRouter, "OpenRouter", None, "keyref-4")
            .await
            .expect("create provider");
        let run_id = AgentRunId::new();
        let model_id = ModelId::new();

        for cost in [0.1, 0.2, 0.3] {
            registry
                .record_usage(UsageRecord {
                    id: 0,
                    run_id,
                    provider_id: provider.id,
                    model_id,
                    input_tokens: 0,
                    output_tokens: 0,
                    cost_usd: cost,
                    created_at: Utc::now(),
                })
                .await
                .expect("record usage");
        }

        let daily = registry.get_daily_usage(provider.id).await.expect("get daily usage");
        assert!((daily - 0.6).abs() < f64::EPSILON);
    }
}
