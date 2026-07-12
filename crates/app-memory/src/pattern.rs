//! Workflow pattern storage for memory-conditioned multi-agent planning.
//!
//! This module is intentionally free of orchestration / runtime types beyond
//! shared DTOs in `app_models`. Workflows depend only on ports they define
//! themselves; the composition root wires this store through adapters.

use app_models::{
    AppError, MemoryScope, OrchestrationOutcome, PatternHint, WorkflowPattern, WorkflowPatternId,
    WorkflowPatternStatus, WorkspaceId,
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;

/// Persistence port for workflow patterns.
///
/// Implementors must not pull in orchestration crates. Callers outside memory
/// should prefer thin adapters over using this trait directly when possible.
#[async_trait]
pub trait PatternStore: Send + Sync {
    /// Upsert a pattern (insert or replace by id).
    async fn upsert_pattern(&self, pattern: &WorkflowPattern) -> Result<(), AppError>;

    /// Fetch a pattern by id.
    async fn get_pattern(&self, id: WorkflowPatternId) -> Result<WorkflowPattern, AppError>;

    /// List patterns for a scope (and optional workspace).
    async fn list_patterns(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Vec<WorkflowPattern>, AppError>;

    /// Recall patterns relevant to a free-text task.
    async fn recall_patterns(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
    ) -> Result<Vec<PatternHint>, AppError>;

    /// Record success/failure feedback for patterns used in an orchestration.
    async fn apply_outcome(&self, outcome: &OrchestrationOutcome) -> Result<(), AppError>;

    /// Soft-mute a pattern so it no longer influences planning.
    async fn mute_pattern(&self, id: WorkflowPatternId) -> Result<(), AppError>;
}

/// SQLite-backed [`PatternStore`].
#[derive(Debug, Clone)]
pub struct SqlitePatternStore {
    pool: SqlitePool,
}

impl SqlitePatternStore {
    /// Create a store over an existing pool (typically the app DB).
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PatternStore for SqlitePatternStore {
    async fn upsert_pattern(&self, pattern: &WorkflowPattern) -> Result<(), AppError> {
        let roles =
            serde_json::to_string(&pattern.preferred_roles).map_err(|e| AppError::Internal {
                message: format!("serialize preferred_roles failed: {e}"),
            })?;
        let workspace_id = pattern.workspace_id.map(|id| id.0.to_string());
        sqlx::query(
            r"
            INSERT INTO workflow_patterns (
                id, scope, workspace_id, name, summary, trigger_text,
                preferred_roles_json, collaboration_style, strength,
                success_count, failure_count, last_used_at, status,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                scope = excluded.scope,
                workspace_id = excluded.workspace_id,
                name = excluded.name,
                summary = excluded.summary,
                trigger_text = excluded.trigger_text,
                preferred_roles_json = excluded.preferred_roles_json,
                collaboration_style = excluded.collaboration_style,
                strength = excluded.strength,
                success_count = excluded.success_count,
                failure_count = excluded.failure_count,
                last_used_at = excluded.last_used_at,
                status = excluded.status,
                updated_at = excluded.updated_at
            ",
        )
        .bind(pattern.id.0.to_string())
        .bind(pattern.scope.as_str())
        .bind(workspace_id)
        .bind(&pattern.name)
        .bind(&pattern.summary)
        .bind(&pattern.trigger_text)
        .bind(roles)
        .bind(&pattern.collaboration_style)
        .bind(pattern.strength)
        .bind(pattern.success_count)
        .bind(pattern.failure_count)
        .bind(pattern.last_used_at.map(|t| t.to_rfc3339()))
        .bind(pattern.status.as_str())
        .bind(pattern.created_at.to_rfc3339())
        .bind(pattern.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("upsert_pattern failed: {e}"),
        })?;
        Ok(())
    }

    async fn get_pattern(&self, id: WorkflowPatternId) -> Result<WorkflowPattern, AppError> {
        let row = sqlx::query_as::<_, PatternRow>(r"SELECT * FROM workflow_patterns WHERE id = ?")
            .bind(id.0.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("get_pattern failed: {e}"),
            })?
            .ok_or_else(|| AppError::NotFound {
                resource: format!("workflow_pattern:{}", id.0),
            })?;
        row.into_pattern()
    }

    async fn list_patterns(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Vec<WorkflowPattern>, AppError> {
        let rows = if let Some(ws) = workspace_id {
            sqlx::query_as::<_, PatternRow>(
                r"
                SELECT * FROM workflow_patterns
                WHERE scope = ? AND workspace_id = ?
                ORDER BY strength DESC, updated_at DESC
                ",
            )
            .bind(scope.as_str())
            .bind(ws.0.to_string())
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, PatternRow>(
                r"
                SELECT * FROM workflow_patterns
                WHERE scope = ? AND workspace_id IS NULL
                ORDER BY strength DESC, updated_at DESC
                ",
            )
            .bind(scope.as_str())
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| AppError::Internal {
            message: format!("list_patterns failed: {e}"),
        })?;

        rows.into_iter().map(PatternRow::into_pattern).collect()
    }

    async fn recall_patterns(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
    ) -> Result<Vec<PatternHint>, AppError> {
        let limit = limit.clamp(1, 20);
        let mut rows = sqlx::query_as::<_, PatternRow>(
            r"
            SELECT * FROM workflow_patterns
            WHERE status = 'active'
              AND (scope = 'User' OR (scope = 'Workspace' AND workspace_id = ?))
            ORDER BY strength DESC, updated_at DESC
            LIMIT 50
            ",
        )
        .bind(workspace_id.map(|id| id.0.to_string()))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("recall_patterns query failed: {e}"),
        })?;

        // Also include pure user-scope rows when workspace filter excluded them via NULL.
        if workspace_id.is_some() {
            let user_rows = sqlx::query_as::<_, PatternRow>(
                r"
                SELECT * FROM workflow_patterns
                WHERE status = 'active' AND scope = 'User' AND workspace_id IS NULL
                ORDER BY strength DESC, updated_at DESC
                LIMIT 50
                ",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("recall_patterns user query failed: {e}"),
            })?;
            rows.extend(user_rows);
        }

        let task_lower = task.to_lowercase();
        let mut hints = Vec::new();
        for row in rows {
            let pattern = row.into_pattern()?;
            let score = score_pattern(&pattern, &task_lower);
            if score <= 0.0 {
                continue;
            }
            hints.push(PatternHint {
                id: pattern.id,
                name: pattern.name,
                summary: pattern.summary,
                preferred_roles: pattern.preferred_roles,
                collaboration_style: pattern.collaboration_style,
                strength: pattern.strength,
                score,
            });
        }
        hints.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        hints.dedup_by(|a, b| a.id == b.id);
        hints.truncate(limit);
        Ok(hints)
    }

    async fn apply_outcome(&self, outcome: &OrchestrationOutcome) -> Result<(), AppError> {
        let now = Utc::now();
        if outcome.pattern_ids.is_empty() {
            // Seed or strengthen a workspace pattern from observed agent mix.
            if outcome.success && !outcome.agent_names.is_empty() {
                let name = format!("learned:{}", outcome.agent_names.join("+"));
                let pattern = WorkflowPattern {
                    id: WorkflowPatternId::new(),
                    scope: MemoryScope::Workspace,
                    workspace_id: Some(outcome.workspace_id),
                    name: name.clone(),
                    summary: outcome
                        .result_summary
                        .clone()
                        .unwrap_or_else(|| "Auto-learned from a successful multi-agent run".into()),
                    trigger_text: outcome.task.chars().take(200).collect(),
                    preferred_roles: outcome.agent_names.clone(),
                    collaboration_style: String::new(),
                    strength: 1.0,
                    success_count: 1,
                    failure_count: 0,
                    last_used_at: Some(now),
                    status: WorkflowPatternStatus::Suggested,
                    created_at: now,
                    updated_at: now,
                };
                self.upsert_pattern(&pattern).await?;
            }
            return Ok(());
        }

        for id in &outcome.pattern_ids {
            let Ok(mut pattern) = self.get_pattern(*id).await else {
                continue;
            };
            if outcome.success {
                pattern.success_count = pattern.success_count.saturating_add(1);
                pattern.strength = (pattern.strength + 0.15).min(10.0);
            } else {
                pattern.failure_count = pattern.failure_count.saturating_add(1);
                pattern.strength = (pattern.strength - 0.2).max(0.1);
            }
            pattern.last_used_at = Some(now);
            pattern.updated_at = now;
            // Promote suggested patterns after repeated success.
            if pattern.status == WorkflowPatternStatus::Suggested && pattern.success_count >= 2 {
                pattern.status = WorkflowPatternStatus::Active;
            }
            self.upsert_pattern(&pattern).await?;
        }
        Ok(())
    }

    async fn mute_pattern(&self, id: WorkflowPatternId) -> Result<(), AppError> {
        let mut pattern = self.get_pattern(id).await?;
        pattern.status = WorkflowPatternStatus::Muted;
        pattern.updated_at = Utc::now();
        self.upsert_pattern(&pattern).await
    }
}

fn score_pattern(pattern: &WorkflowPattern, task_lower: &str) -> f64 {
    let mut score = 0.0;
    let trigger = pattern.trigger_text.to_lowercase();
    if !trigger.is_empty() {
        for token in trigger.split(|c: char| !c.is_alphanumeric() && c != '_') {
            if token.len() < 2 {
                continue;
            }
            if task_lower.contains(token) {
                score += 1.0;
            }
        }
    }
    let name = pattern.name.to_lowercase();
    if !name.is_empty() && task_lower.contains(&name) {
        score += 1.5;
    }
    for role in &pattern.preferred_roles {
        let role_l = role.to_lowercase();
        if task_lower.contains(&role_l) {
            score += 0.5;
        }
    }
    // Always give a small base score to strong active habits so they surface.
    if score == 0.0 && pattern.strength >= 2.0 {
        score = 0.25;
    }
    score * (1.0 + pattern.strength.ln_1p())
}

#[derive(Debug, sqlx::FromRow)]
struct PatternRow {
    id: String,
    scope: String,
    workspace_id: Option<String>,
    name: String,
    summary: String,
    trigger_text: String,
    preferred_roles_json: String,
    collaboration_style: String,
    strength: f64,
    success_count: i64,
    failure_count: i64,
    last_used_at: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
}

impl PatternRow {
    fn into_pattern(self) -> Result<WorkflowPattern, AppError> {
        let id = uuid::Uuid::parse_str(&self.id).map_err(|e| AppError::Internal {
            message: format!("invalid pattern id: {e}"),
        })?;
        let scope = MemoryScope::try_from(self.scope.as_str())?;
        let workspace_id = self
            .workspace_id
            .as_deref()
            .map(|s| {
                uuid::Uuid::parse_str(s).map(WorkspaceId).map_err(|e| AppError::Internal {
                    message: format!("invalid workspace id on pattern: {e}"),
                })
            })
            .transpose()?;
        let preferred_roles: Vec<String> =
            serde_json::from_str(&self.preferred_roles_json).unwrap_or_default();
        let status = WorkflowPatternStatus::try_from(self.status.as_str())?;
        let parse_dt = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| AppError::Internal {
                    message: format!("invalid datetime on pattern: {e}"),
                })
        };
        Ok(WorkflowPattern {
            id: WorkflowPatternId(id),
            scope,
            workspace_id,
            name: self.name,
            summary: self.summary,
            trigger_text: self.trigger_text,
            preferred_roles,
            collaboration_style: self.collaboration_style,
            strength: self.strength,
            success_count: self.success_count,
            failure_count: self.failure_count,
            last_used_at: self.last_used_at.as_deref().map(parse_dt).transpose()?,
            status,
            created_at: parse_dt(&self.created_at)?,
            updated_at: parse_dt(&self.updated_at)?,
        })
    }
}

/// In-memory pattern store for unit tests (no `SQLite`).
#[derive(Debug, Default)]
pub struct InMemoryPatternStore {
    patterns: std::sync::Mutex<Vec<WorkflowPattern>>,
}

impl InMemoryPatternStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl PatternStore for InMemoryPatternStore {
    async fn upsert_pattern(&self, pattern: &WorkflowPattern) -> Result<(), AppError> {
        let mut guard = self.patterns.lock().map_err(|_| AppError::Internal {
            message: "pattern store lock poisoned".to_owned(),
        })?;
        if let Some(existing) = guard.iter_mut().find(|p| p.id == pattern.id) {
            *existing = pattern.clone();
        } else {
            guard.push(pattern.clone());
        }
        drop(guard);
        Ok(())
    }

    async fn get_pattern(&self, id: WorkflowPatternId) -> Result<WorkflowPattern, AppError> {
        self.patterns
            .lock()
            .map_err(|_| AppError::Internal {
                message: "pattern store lock poisoned".to_owned(),
            })?
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound {
                resource: format!("workflow_pattern:{}", id.0),
            })
    }

    async fn list_patterns(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Vec<WorkflowPattern>, AppError> {
        Ok(self
            .patterns
            .lock()
            .map_err(|_| AppError::Internal {
                message: "pattern store lock poisoned".to_owned(),
            })?
            .iter()
            .filter(|p| p.scope == scope && p.workspace_id == workspace_id)
            .cloned()
            .collect())
    }

    async fn recall_patterns(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
    ) -> Result<Vec<PatternHint>, AppError> {
        let task_lower = task.to_lowercase();
        let mut hints: Vec<PatternHint> = self
            .patterns
            .lock()
            .map_err(|_| AppError::Internal {
                message: "pattern store lock poisoned".to_owned(),
            })?
            .iter()
            .filter(|p| {
                p.status == WorkflowPatternStatus::Active
                    && (p.scope == MemoryScope::User
                        || (p.scope == MemoryScope::Workspace && p.workspace_id == workspace_id))
            })
            .filter_map(|p| {
                let score = score_pattern(p, &task_lower);
                (score > 0.0).then(|| PatternHint {
                    id: p.id,
                    name: p.name.clone(),
                    summary: p.summary.clone(),
                    preferred_roles: p.preferred_roles.clone(),
                    collaboration_style: p.collaboration_style.clone(),
                    strength: p.strength,
                    score,
                })
            })
            .collect();
        hints.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hints.truncate(limit.clamp(1, 20));
        Ok(hints)
    }

    async fn apply_outcome(&self, outcome: &OrchestrationOutcome) -> Result<(), AppError> {
        let now = Utc::now();
        let mut guard = self.patterns.lock().map_err(|_| AppError::Internal {
            message: "pattern store lock poisoned".to_owned(),
        })?;
        if outcome.pattern_ids.is_empty() && outcome.success && !outcome.agent_names.is_empty() {
            guard.push(WorkflowPattern {
                id: WorkflowPatternId::new(),
                scope: MemoryScope::Workspace,
                workspace_id: Some(outcome.workspace_id),
                name: format!("learned:{}", outcome.agent_names.join("+")),
                summary: outcome
                    .result_summary
                    .clone()
                    .unwrap_or_else(|| "Auto-learned pattern".into()),
                trigger_text: outcome.task.chars().take(200).collect(),
                preferred_roles: outcome.agent_names.clone(),
                collaboration_style: String::new(),
                strength: 1.0,
                success_count: 1,
                failure_count: 0,
                last_used_at: Some(now),
                status: WorkflowPatternStatus::Suggested,
                created_at: now,
                updated_at: now,
            });
            return Ok(());
        }
        for id in &outcome.pattern_ids {
            if let Some(p) = guard.iter_mut().find(|p| p.id == *id) {
                if outcome.success {
                    p.success_count += 1;
                    p.strength = (p.strength + 0.15).min(10.0);
                } else {
                    p.failure_count += 1;
                    p.strength = (p.strength - 0.2).max(0.1);
                }
                p.last_used_at = Some(now);
                p.updated_at = now;
            }
        }
        Ok(())
    }

    async fn mute_pattern(&self, id: WorkflowPatternId) -> Result<(), AppError> {
        let mut p = self.get_pattern(id).await?;
        p.status = WorkflowPatternStatus::Muted;
        p.updated_at = Utc::now();
        self.upsert_pattern(&p).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recall_scores_trigger_overlap() {
        let store = InMemoryPatternStore::new();
        let now = Utc::now();
        let ws = WorkspaceId::new();
        store
            .upsert_pattern(&WorkflowPattern {
                id: WorkflowPatternId::new(),
                scope: MemoryScope::Workspace,
                workspace_id: Some(ws),
                name: "security-first".into(),
                summary: "Explore then security review".into(),
                trigger_text: "安全 审计 security review".into(),
                preferred_roles: vec!["explorer".into(), "security-reviewer".into()],
                collaboration_style: "read-only".into(),
                strength: 2.0,
                success_count: 3,
                failure_count: 0,
                last_used_at: Some(now),
                status: WorkflowPatternStatus::Active,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        let hints = store.recall_patterns("请做一次安全审计", Some(ws), 5).await.unwrap();
        assert_eq!(hints.len(), 1);
        assert!(hints[0].score > 0.0);
        assert_eq!(hints[0].preferred_roles.len(), 2);
    }

    #[tokio::test]
    async fn apply_outcome_learns_new_suggested_pattern() {
        let store = InMemoryPatternStore::new();
        let ws = WorkspaceId::new();
        store
            .apply_outcome(&OrchestrationOutcome {
                workspace_id: ws,
                task: "list files and summarize".into(),
                success: true,
                agent_names: vec!["explorer".into(), "default".into()],
                pattern_ids: vec![],
                result_summary: Some("ok".into()),
            })
            .await
            .unwrap();
        let listed = store.list_patterns(MemoryScope::Workspace, Some(ws)).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, WorkflowPatternStatus::Suggested);
    }
}
