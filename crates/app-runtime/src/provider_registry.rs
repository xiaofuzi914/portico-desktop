//! Model provider registry, capability matrix, and usage cost guard.

use app_models::{
    ActiveModelSelection, AgentRunId, AppError, ModelCapability, ModelId, ModelInfo,
    ModelSelectionScope, ProviderConfig, ProviderHealth, ProviderId, ProviderKind, RetryPolicy,
    RunModelSnapshot, ThreadId, UsageBudget, UsageRecord, WorkspaceId,
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

    /// Set the active provider/model at one selection scope.
    async fn set_active_model(
        &self,
        scope: ModelSelectionScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        provider_id: ProviderId,
        model_id: ModelId,
    ) -> Result<ActiveModelSelection, AppError>;

    /// Load one exact active selection without applying scope fallback.
    async fn get_active_model(
        &self,
        scope: ModelSelectionScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
    ) -> Result<Option<ActiveModelSelection>, AppError>;

    /// Resolve thread, workspace, then global selection and validate it is executable.
    async fn resolve_active_model(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<ActiveModelSelection, AppError>;

    /// Persist the safe result of a provider/model health check.
    async fn record_provider_health(&self, health: ProviderHealth) -> Result<(), AppError>;

    /// Load the most recent health result for a provider/model pair.
    async fn get_provider_health(
        &self,
        provider_id: ProviderId,
        model_id: ModelId,
    ) -> Result<Option<ProviderHealth>, AppError>;

    /// Invalidate cached health after provider metadata or credentials change.
    async fn invalidate_provider_health(&self, provider_id: ProviderId) -> Result<(), AppError>;

    /// Persist the immutable provider/model snapshot for a run.
    async fn snapshot_run_model(
        &self,
        snapshot: RunModelSnapshot,
    ) -> Result<RunModelSnapshot, AppError>;

    /// Load the immutable provider/model snapshot for a run.
    async fn get_run_model_snapshot(
        &self,
        run_id: AgentRunId,
    ) -> Result<Option<RunModelSnapshot>, AppError>;

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
            // Chat-with-tools + long generations routinely exceed 30s on remote
            // providers (DeepSeek/OpenAI-compatible). Default to 2 minutes.
            timeout_ms: 120_000,
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
        .bind(i64::try_from(config.timeout_ms).unwrap_or(120_000))
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

        let result = sqlx::query(
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
        .bind(i64::try_from(config.timeout_ms).unwrap_or(120_000))
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

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("provider {:?}", config.id),
            });
        }
        self.invalidate_provider_health(config.id).await?;

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

    async fn set_active_model(
        &self,
        scope: ModelSelectionScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        provider_id: ProviderId,
        model_id: ModelId,
    ) -> Result<ActiveModelSelection, AppError> {
        let scope_key = selection_scope_key(scope, workspace_id, thread_id)?;
        validate_selection_owner(&self.pool, scope, workspace_id, thread_id).await?;
        let provider = self.get_provider(provider_id).await?;
        if !provider.enabled {
            return Err(AppError::PermissionDenied {
                reason: "disabled providers cannot be selected".to_owned(),
            });
        }
        let model = self.get_model(model_id).await?;
        if model.provider_id != provider_id {
            return Err(AppError::PermissionDenied {
                reason: "selected model does not belong to the selected provider".to_owned(),
            });
        }
        let updated_at = Utc::now();
        sqlx::query(
            "INSERT INTO active_model_selections
                (scope_key, scope_type, workspace_id, thread_id, provider_id, model_id, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(scope_key) DO UPDATE SET
                provider_id = excluded.provider_id,
                model_id = excluded.model_id,
                updated_at = excluded.updated_at",
        )
        .bind(scope_key)
        .bind(scope.as_str())
        .bind(workspace_id.map(|id| id.0))
        .bind(thread_id.map(|id| id.0))
        .bind(provider_id.0)
        .bind(model_id.0)
        .bind(updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("set_active_model failed: {e}"),
        })?;

        Ok(ActiveModelSelection {
            scope,
            workspace_id,
            thread_id,
            provider_id,
            model_id,
            provider_name: provider.display_name,
            model_name: model.model_name,
            updated_at,
        })
    }

    async fn get_active_model(
        &self,
        scope: ModelSelectionScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
    ) -> Result<Option<ActiveModelSelection>, AppError> {
        let scope_key = selection_scope_key(scope, workspace_id, thread_id)?;
        let row = sqlx::query_as::<_, ActiveModelSelectionRow>(
            "SELECT s.scope_type, s.workspace_id, s.thread_id, s.provider_id, s.model_id,
                    p.display_name AS provider_name, m.model_name, s.updated_at
             FROM active_model_selections s
             JOIN provider_configs p ON p.id = s.provider_id
             JOIN model_infos m ON m.id = s.model_id
             WHERE s.scope_key = ?",
        )
        .bind(scope_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_active_model failed: {e}"),
        })?;
        row.map(ActiveModelSelection::try_from).transpose()
    }

    async fn resolve_active_model(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<ActiveModelSelection, AppError> {
        validate_selection_owner(
            &self.pool,
            ModelSelectionScope::Thread,
            Some(workspace_id),
            Some(thread_id),
        )
        .await?;
        let thread_key = selection_scope_key(
            ModelSelectionScope::Thread,
            Some(workspace_id),
            Some(thread_id),
        )?;
        let workspace_key =
            selection_scope_key(ModelSelectionScope::Workspace, Some(workspace_id), None)?;
        let row = sqlx::query_as::<_, ActiveModelSelectionRow>(
            "SELECT s.scope_type, s.workspace_id, s.thread_id, s.provider_id, s.model_id,
                    p.display_name AS provider_name, m.model_name, s.updated_at
             FROM active_model_selections s
             JOIN provider_configs p ON p.id = s.provider_id
             JOIN model_infos m ON m.id = s.model_id
             WHERE s.scope_key IN (?, ?, 'global')
             ORDER BY CASE s.scope_type WHEN 'Thread' THEN 0 WHEN 'Workspace' THEN 1 ELSE 2 END
             LIMIT 1",
        )
        .bind(thread_key)
        .bind(workspace_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("resolve_active_model failed: {e}"),
        })?
        .ok_or_else(|| AppError::Internal {
            message: "PROVIDER_SELECTION_REQUIRED: select an active provider and model".to_owned(),
        })?;
        let selection = ActiveModelSelection::try_from(row)?;
        let provider = self.get_provider(selection.provider_id).await?;
        if !provider.enabled {
            return Err(AppError::Internal {
                message: "PROVIDER_SELECTION_DISABLED: the selected provider is disabled"
                    .to_owned(),
            });
        }
        let model = self.get_model(selection.model_id).await?;
        if model.provider_id != provider.id {
            return Err(AppError::Internal {
                message: "PROVIDER_SELECTION_INVALID: model/provider relationship changed"
                    .to_owned(),
            });
        }
        Ok(selection)
    }

    async fn record_provider_health(&self, health: ProviderHealth) -> Result<(), AppError> {
        let message = health
            .message
            .as_deref()
            .map(|message| message.chars().take(512).collect::<String>());
        sqlx::query(
            "INSERT INTO provider_health
                (provider_id, model_id, status, error_code, message, checked_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(provider_id, model_id) DO UPDATE SET
                status = excluded.status,
                error_code = excluded.error_code,
                message = excluded.message,
                checked_at = excluded.checked_at",
        )
        .bind(health.provider_id.0)
        .bind(health.model_id.0)
        .bind(health.status.as_str())
        .bind(health.error_code)
        .bind(message)
        .bind(health.checked_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("record_provider_health failed: {e}"),
        })?;
        Ok(())
    }

    async fn get_provider_health(
        &self,
        provider_id: ProviderId,
        model_id: ModelId,
    ) -> Result<Option<ProviderHealth>, AppError> {
        let row = sqlx::query_as::<_, ProviderHealthRow>(
            "SELECT provider_id, model_id, status, error_code, message, checked_at
             FROM provider_health WHERE provider_id = ? AND model_id = ?",
        )
        .bind(provider_id.0)
        .bind(model_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_provider_health failed: {e}"),
        })?;
        row.map(ProviderHealth::try_from).transpose()
    }

    async fn invalidate_provider_health(&self, provider_id: ProviderId) -> Result<(), AppError> {
        sqlx::query("DELETE FROM provider_health WHERE provider_id = ?")
            .bind(provider_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("invalidate_provider_health failed: {e}"),
            })?;
        Ok(())
    }

    async fn snapshot_run_model(
        &self,
        snapshot: RunModelSnapshot,
    ) -> Result<RunModelSnapshot, AppError> {
        sqlx::query(
            "INSERT OR IGNORE INTO run_model_snapshots
                (run_id, provider_id, model_id, provider_name, model_name,
                 provider_config_updated_at, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(snapshot.run_id.0)
        .bind(snapshot.provider_id.0)
        .bind(snapshot.model_id.0)
        .bind(&snapshot.provider_name)
        .bind(&snapshot.model_name)
        .bind(snapshot.provider_config_updated_at)
        .bind(snapshot.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("snapshot_run_model failed: {e}"),
        })?;
        let persisted = self.get_run_model_snapshot(snapshot.run_id).await?.ok_or_else(|| {
            AppError::Internal {
                message: "run model snapshot was not persisted".to_owned(),
            }
        })?;
        if persisted != snapshot {
            return Err(AppError::PermissionDenied {
                reason: "run model snapshot is immutable".to_owned(),
            });
        }
        Ok(persisted)
    }

    async fn get_run_model_snapshot(
        &self,
        run_id: AgentRunId,
    ) -> Result<Option<RunModelSnapshot>, AppError> {
        let row = sqlx::query_as::<_, RunModelSnapshotRow>(
            "SELECT run_id, provider_id, model_id, provider_name, model_name,
                    provider_config_updated_at, created_at
             FROM run_model_snapshots WHERE run_id = ?",
        )
        .bind(run_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_run_model_snapshot failed: {e}"),
        })?;
        Ok(row.map(Into::into))
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

        if let Some(per_run) = self.budget.per_run_usd
            && estimated_cost_usd > per_run
        {
            return Err(AppError::PermissionDenied {
                reason: format!(
                    "estimated cost {estimated_cost_usd:.6} exceeds per-run budget {per_run:.2}"
                ),
            });
        }

        let daily_usage = self.get_daily_usage(provider_id).await?;
        if let Some(daily) = self.budget.daily_usd
            && estimated_cost_usd + daily_usage > daily
        {
            return Err(AppError::PermissionDenied {
                reason: format!(
                    "estimated cost {estimated_cost_usd:.6} + daily usage {daily_usage:.6} exceeds daily budget {daily:.2}"
                ),
            });
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

fn selection_scope_key(
    scope: ModelSelectionScope,
    workspace_id: Option<WorkspaceId>,
    thread_id: Option<ThreadId>,
) -> Result<String, AppError> {
    match (scope, workspace_id, thread_id) {
        (ModelSelectionScope::Global, None, None) => Ok("global".to_owned()),
        (ModelSelectionScope::Workspace, Some(workspace_id), None) => {
            Ok(format!("workspace:{}", workspace_id.0))
        }
        (ModelSelectionScope::Thread, Some(workspace_id), Some(thread_id)) => {
            Ok(format!("thread:{}:{}", workspace_id.0, thread_id.0))
        }
        _ => Err(AppError::PermissionDenied {
            reason: "model selection scope does not match its workspace/thread identifiers"
                .to_owned(),
        }),
    }
}

async fn validate_selection_owner(
    pool: &SqlitePool,
    scope: ModelSelectionScope,
    workspace_id: Option<WorkspaceId>,
    thread_id: Option<ThreadId>,
) -> Result<(), AppError> {
    selection_scope_key(scope, workspace_id, thread_id)?;
    match scope {
        ModelSelectionScope::Global => Ok(()),
        ModelSelectionScope::Workspace => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?)")
                    .bind(workspace_id.expect("validated workspace scope").0)
                    .fetch_one(pool)
                    .await
                    .map_err(|e| AppError::Internal {
                        message: format!("validate model selection workspace failed: {e}"),
                    })?;
            if exists {
                Ok(())
            } else {
                Err(AppError::NotFound {
                    resource: "model selection workspace".to_owned(),
                })
            }
        }
        ModelSelectionScope::Thread => {
            let owner: Option<uuid::Uuid> =
                sqlx::query_scalar("SELECT workspace_id FROM threads WHERE id = ?")
                    .bind(thread_id.expect("validated thread scope").0)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| AppError::Internal {
                        message: format!("validate model selection thread failed: {e}"),
                    })?;
            if owner == workspace_id.map(|id| id.0) {
                Ok(())
            } else {
                Err(AppError::PermissionDenied {
                    reason: "model selection thread does not belong to workspace".to_owned(),
                })
            }
        }
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
            timeout_ms: u64::try_from(row.timeout_ms).unwrap_or(120_000),
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

#[derive(FromRow)]
struct ActiveModelSelectionRow {
    scope_type: String,
    workspace_id: Option<uuid::Uuid>,
    thread_id: Option<uuid::Uuid>,
    provider_id: uuid::Uuid,
    model_id: uuid::Uuid,
    provider_name: String,
    model_name: String,
    updated_at: DateTime<Utc>,
}

impl TryFrom<ActiveModelSelectionRow> for ActiveModelSelection {
    type Error = AppError;

    fn try_from(row: ActiveModelSelectionRow) -> Result<Self, Self::Error> {
        Ok(Self {
            scope: row.scope_type.as_str().try_into()?,
            workspace_id: row.workspace_id.map(WorkspaceId),
            thread_id: row.thread_id.map(ThreadId),
            provider_id: ProviderId(row.provider_id),
            model_id: ModelId(row.model_id),
            provider_name: row.provider_name,
            model_name: row.model_name,
            updated_at: row.updated_at,
        })
    }
}

#[derive(FromRow)]
struct ProviderHealthRow {
    provider_id: uuid::Uuid,
    model_id: uuid::Uuid,
    status: String,
    error_code: Option<String>,
    message: Option<String>,
    checked_at: DateTime<Utc>,
}

impl TryFrom<ProviderHealthRow> for ProviderHealth {
    type Error = AppError;

    fn try_from(row: ProviderHealthRow) -> Result<Self, Self::Error> {
        Ok(Self {
            provider_id: ProviderId(row.provider_id),
            model_id: ModelId(row.model_id),
            status: row.status.as_str().try_into()?,
            error_code: row.error_code,
            message: row.message,
            checked_at: row.checked_at,
        })
    }
}

#[derive(FromRow)]
struct RunModelSnapshotRow {
    run_id: uuid::Uuid,
    provider_id: uuid::Uuid,
    model_id: uuid::Uuid,
    provider_name: String,
    model_name: String,
    provider_config_updated_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

impl From<RunModelSnapshotRow> for RunModelSnapshot {
    fn from(row: RunModelSnapshotRow) -> Self {
        Self {
            run_id: AgentRunId(row.run_id),
            provider_id: ProviderId(row.provider_id),
            model_id: ModelId(row.model_id),
            provider_name: row.provider_name,
            model_name: row.model_name,
            provider_config_updated_at: row.provider_config_updated_at,
            created_at: row.created_at,
        }
    }
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
    use crate::{SqliteStorage, Storage};
    use std::sync::Arc;

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

    fn executable_capabilities() -> ModelCapability {
        ModelCapability {
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
        }
    }

    #[tokio::test]
    async fn active_model_resolution_prefers_thread_then_workspace_then_global() {
        std::fs::create_dir_all("/tmp/portico-selection-test").expect("test root");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let registry = SqliteModelProviderRegistry::new(storage.pool().clone());
        let workspace = storage
            .create_workspace("test", "/tmp/portico-selection-test", false)
            .await
            .expect("workspace");
        let thread = storage.create_thread(workspace.id, "thread").await.expect("thread");
        let first_provider = registry
            .create_provider(ProviderKind::OpenAI, "one", None, "one-ref")
            .await
            .expect("provider one");
        let first_model = registry
            .add_model(
                first_provider.id,
                "model-one",
                "one",
                executable_capabilities(),
            )
            .await
            .expect("model one");
        let second_provider = registry
            .create_provider(ProviderKind::Moonshot, "two", None, "two-ref")
            .await
            .expect("provider two");
        let second_model = registry
            .add_model(
                second_provider.id,
                "model-two",
                "two",
                executable_capabilities(),
            )
            .await
            .expect("model two");

        registry
            .set_active_model(
                ModelSelectionScope::Global,
                None,
                None,
                first_provider.id,
                first_model.id,
            )
            .await
            .expect("global selection");
        assert_eq!(
            registry
                .resolve_active_model(workspace.id, thread.id)
                .await
                .expect("resolve global")
                .model_id,
            first_model.id
        );

        registry
            .set_active_model(
                ModelSelectionScope::Workspace,
                Some(workspace.id),
                None,
                second_provider.id,
                second_model.id,
            )
            .await
            .expect("workspace selection");
        assert_eq!(
            registry
                .resolve_active_model(workspace.id, thread.id)
                .await
                .expect("resolve workspace")
                .model_id,
            second_model.id
        );

        registry
            .set_active_model(
                ModelSelectionScope::Thread,
                Some(workspace.id),
                Some(thread.id),
                first_provider.id,
                first_model.id,
            )
            .await
            .expect("thread selection");
        let resolved = registry
            .resolve_active_model(workspace.id, thread.id)
            .await
            .expect("resolve thread");
        assert_eq!(resolved.scope, ModelSelectionScope::Thread);
        assert_eq!(resolved.model_id, first_model.id);
    }

    #[tokio::test]
    async fn active_model_rejects_provider_model_mismatch() {
        let registry = setup().await;
        let first = registry
            .create_provider(ProviderKind::OpenAI, "one", None, "one-ref")
            .await
            .expect("provider one");
        let second = registry
            .create_provider(ProviderKind::Moonshot, "two", None, "two-ref")
            .await
            .expect("provider two");
        let model = registry
            .add_model(second.id, "model", "model", executable_capabilities())
            .await
            .expect("model");

        let result = registry
            .set_active_model(ModelSelectionScope::Global, None, None, first.id, model.id)
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn provider_health_and_run_snapshot_are_durable_and_snapshot_is_immutable() {
        std::fs::create_dir_all("/tmp/portico-snapshot-test").expect("test root");
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let registry = SqliteModelProviderRegistry::new(storage.pool().clone());
        let workspace = storage
            .create_workspace("test", "/tmp/portico-snapshot-test", false)
            .await
            .expect("workspace");
        let thread = storage.create_thread(workspace.id, "thread").await.expect("thread");
        let run = storage.create_run(workspace.id, thread.id).await.expect("run");
        let provider = registry
            .create_provider(ProviderKind::OpenAI, "OpenAI", None, "ref")
            .await
            .expect("provider");
        let model = registry
            .add_model(provider.id, "model", "model", executable_capabilities())
            .await
            .expect("model");
        let checked_at = Utc::now();
        let health = ProviderHealth {
            provider_id: provider.id,
            model_id: model.id,
            status: app_models::ProviderHealthStatus::Ready,
            error_code: None,
            message: Some("ready".to_owned()),
            checked_at,
        };
        registry.record_provider_health(health.clone()).await.expect("health");
        assert_eq!(
            registry.get_provider_health(provider.id, model.id).await.expect("get health"),
            Some(health)
        );

        let mut updated_provider = provider.clone();
        updated_provider.base_url = Some("https://example.invalid/v2".to_owned());
        registry
            .update_provider(updated_provider.clone())
            .await
            .expect("update provider");
        assert_eq!(
            registry
                .get_provider_health(provider.id, model.id)
                .await
                .expect("invalidated health"),
            None
        );

        let snapshot = RunModelSnapshot {
            run_id: run.id,
            provider_id: provider.id,
            model_id: model.id,
            provider_name: provider.display_name,
            model_name: model.model_name,
            provider_config_updated_at: updated_provider.updated_at,
            created_at: Utc::now(),
        };
        assert_eq!(
            registry.snapshot_run_model(snapshot.clone()).await.expect("snapshot"),
            snapshot
        );
        assert_eq!(
            registry.get_run_model_snapshot(run.id).await.expect("get snapshot"),
            Some(snapshot.clone())
        );

        let changed = RunModelSnapshot {
            model_name: "other".to_owned(),
            ..snapshot
        };
        assert!(matches!(
            registry.snapshot_run_model(changed).await,
            Err(AppError::PermissionDenied { .. })
        ));
    }
}
