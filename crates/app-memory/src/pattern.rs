//! Workflow pattern storage for memory-conditioned multi-agent planning.
//!
//! This module is intentionally free of orchestration / runtime types beyond
//! shared DTOs in `app_models`. Workflows depend only on ports they define
//! themselves; the composition root wires this store through adapters.

use app_models::{
    AppError, MemoryScope, OrchestrationOutcome, PatternHint, WorkflowPattern, WorkflowPatternEvidence,
    WorkflowPatternId, WorkflowPatternPatch, WorkflowPatternStatus, WorkspaceId,
};
use async_trait::async_trait;
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Independent successes required before auto-promoting Suggested → Active.
pub const AUTO_PROMOTE_EVIDENCE_THRESHOLD: i64 = 3;

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

    /// Recall patterns relevant to a free-text task (Active only — may change execution).
    async fn recall_patterns(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
    ) -> Result<Vec<PatternHint>, AppError>;

    /// Shadow-match Suggested patterns for evidence / UI without applying them.
    async fn shadow_match_patterns(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
    ) -> Result<Vec<PatternHint>, AppError>;

    /// Record success/failure feedback for patterns used in an orchestration.
    async fn apply_outcome(&self, outcome: &OrchestrationOutcome) -> Result<(), AppError>;

    /// Soft-mute a pattern so it no longer influences planning.
    async fn mute_pattern(&self, id: WorkflowPatternId) -> Result<(), AppError>;

    /// Accept a Suggested pattern (immediately Active).
    async fn accept_pattern(&self, id: WorkflowPatternId) -> Result<WorkflowPattern, AppError>;

    /// Reject a pattern (suppress fingerprint re-suggestion).
    async fn reject_pattern(&self, id: WorkflowPatternId) -> Result<WorkflowPattern, AppError>;

    /// Apply a user edit patch.
    async fn edit_pattern(
        &self,
        id: WorkflowPatternId,
        patch: WorkflowPatternPatch,
    ) -> Result<WorkflowPattern, AppError>;

    /// Evidence summary for Inspector / accept UI.
    async fn list_pattern_evidence(
        &self,
        id: WorkflowPatternId,
    ) -> Result<WorkflowPatternEvidence, AppError>;

    /// Lookup by fingerprint (any non-rejected status).
    async fn find_by_fingerprint(
        &self,
        fingerprint: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Option<WorkflowPattern>, AppError>;
}

/// Compute a stable pattern fingerprint from normalized habit fields.
///
/// Does **not** include full task text — only task kind / roles / styles —
/// so identical strategies collapse into one Suggested row.
#[must_use]
pub fn pattern_fingerprint(
    scope: MemoryScope,
    task_kind: &str,
    preferred_roles: &[String],
    collaboration_style: &str,
    tool_strategy: &str,
    output_kind: &str,
) -> String {
    let mut roles: Vec<String> = preferred_roles.iter().map(|r| r.to_lowercase()).collect();
    roles.sort();
    let payload = serde_json::json!({
        "scope": scope.as_str(),
        "task_kind": task_kind.trim().to_lowercase(),
        "preferred_roles": roles,
        "collaboration_style": collaboration_style.trim().to_lowercase(),
        "tool_strategy": tool_strategy.trim().to_lowercase(),
        "output_kind": output_kind.trim().to_lowercase(),
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let digest = Sha256::digest(bytes);
    format!("wp_{}", hex_encode(&digest))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
                created_at, updated_at,
                fingerprint, evidence_count, confidence,
                last_success_at, last_failure_at,
                tool_strategy, output_kind, task_kind
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                updated_at = excluded.updated_at,
                fingerprint = excluded.fingerprint,
                evidence_count = excluded.evidence_count,
                confidence = excluded.confidence,
                last_success_at = excluded.last_success_at,
                last_failure_at = excluded.last_failure_at,
                tool_strategy = excluded.tool_strategy,
                output_kind = excluded.output_kind,
                task_kind = excluded.task_kind
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
        .bind(pattern.fingerprint.as_deref())
        .bind(pattern.evidence_count)
        .bind(pattern.confidence)
        .bind(pattern.last_success_at.map(|t| t.to_rfc3339()))
        .bind(pattern.last_failure_at.map(|t| t.to_rfc3339()))
        .bind(&pattern.tool_strategy)
        .bind(&pattern.output_kind)
        .bind(&pattern.task_kind)
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
        self.recall_with_statuses(task, workspace_id, limit, &["active"])
            .await
    }

    async fn shadow_match_patterns(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
    ) -> Result<Vec<PatternHint>, AppError> {
        self.recall_with_statuses(task, workspace_id, limit, &["suggested"])
            .await
    }

    async fn apply_outcome(&self, outcome: &OrchestrationOutcome) -> Result<(), AppError> {
        let now = Utc::now();
        if !outcome.pattern_ids.is_empty() {
            for id in &outcome.pattern_ids {
                let Ok(mut pattern) = self.get_pattern(*id).await else {
                    continue;
                };
                apply_success_or_failure(&mut pattern, outcome.success, now);
                self.upsert_pattern(&pattern).await?;
            }
            return Ok(());
        }

        // No explicit pattern_ids: fingerprint-based upsert for Suggested.
        if !outcome.success || outcome.agent_names.is_empty() {
            return Ok(());
        }

        let roles = outcome.agent_names.clone();
        let task_kind = infer_task_kind(&outcome.task);
        let tool_strategy = infer_tool_strategy(&outcome.task, &roles);
        let output_kind = infer_output_kind(&outcome.task);
        let fingerprint = pattern_fingerprint(
            MemoryScope::Workspace,
            &task_kind,
            &roles,
            "",
            &tool_strategy,
            &output_kind,
        );

        // Never re-suggest rejected fingerprints.
        if let Some(existing) = self
            .find_by_fingerprint(&fingerprint, Some(outcome.workspace_id))
            .await?
        {
            if existing.status == WorkflowPatternStatus::Rejected {
                return Ok(());
            }
            let mut pattern = existing;
            apply_success_or_failure(&mut pattern, true, now);
            // Strengthen trigger with a short task snippet (not fingerprint input).
            if pattern.trigger_text.is_empty() {
                pattern.trigger_text = outcome.task.chars().take(200).collect();
            }
            self.upsert_pattern(&pattern).await?;
            return Ok(());
        }

        let name = format!("learned:{}", roles.join("+"));
        let pattern = WorkflowPattern {
            id: WorkflowPatternId::new(),
            scope: MemoryScope::Workspace,
            workspace_id: Some(outcome.workspace_id),
            name,
            summary: outcome
                .result_summary
                .clone()
                .unwrap_or_else(|| "Auto-learned from a successful multi-agent run".into()),
            trigger_text: outcome.task.chars().take(200).collect(),
            preferred_roles: roles,
            collaboration_style: String::new(),
            strength: 1.0,
            success_count: 1,
            failure_count: 0,
            last_used_at: Some(now),
            status: WorkflowPatternStatus::Suggested,
            fingerprint: Some(fingerprint),
            evidence_count: 1,
            confidence: 0.35,
            last_success_at: Some(now),
            last_failure_at: None,
            tool_strategy,
            output_kind,
            task_kind,
            created_at: now,
            updated_at: now,
        };
        self.upsert_pattern(&pattern).await?;
        Ok(())
    }

    async fn mute_pattern(&self, id: WorkflowPatternId) -> Result<(), AppError> {
        let mut pattern = self.get_pattern(id).await?;
        pattern.status = WorkflowPatternStatus::Muted;
        pattern.updated_at = Utc::now();
        self.upsert_pattern(&pattern).await
    }

    async fn accept_pattern(&self, id: WorkflowPatternId) -> Result<WorkflowPattern, AppError> {
        let mut pattern = self.get_pattern(id).await?;
        pattern.status = WorkflowPatternStatus::Active;
        pattern.confidence = pattern.confidence.max(0.7);
        pattern.updated_at = Utc::now();
        self.upsert_pattern(&pattern).await?;
        Ok(pattern)
    }

    async fn reject_pattern(&self, id: WorkflowPatternId) -> Result<WorkflowPattern, AppError> {
        let mut pattern = self.get_pattern(id).await?;
        pattern.status = WorkflowPatternStatus::Rejected;
        pattern.updated_at = Utc::now();
        self.upsert_pattern(&pattern).await?;
        Ok(pattern)
    }

    async fn edit_pattern(
        &self,
        id: WorkflowPatternId,
        patch: WorkflowPatternPatch,
    ) -> Result<WorkflowPattern, AppError> {
        let mut pattern = self.get_pattern(id).await?;
        if let Some(name) = patch.name {
            pattern.name = name;
        }
        if let Some(summary) = patch.summary {
            pattern.summary = summary;
        }
        if let Some(trigger_text) = patch.trigger_text {
            pattern.trigger_text = trigger_text;
        }
        if let Some(preferred_roles) = patch.preferred_roles {
            pattern.preferred_roles = preferred_roles;
        }
        if let Some(collaboration_style) = patch.collaboration_style {
            pattern.collaboration_style = collaboration_style;
        }
        if let Some(tool_strategy) = patch.tool_strategy {
            pattern.tool_strategy = tool_strategy;
        }
        if let Some(output_kind) = patch.output_kind {
            pattern.output_kind = output_kind;
        }
        // Recompute fingerprint after structural edits.
        pattern.fingerprint = Some(pattern_fingerprint(
            pattern.scope,
            &pattern.task_kind,
            &pattern.preferred_roles,
            &pattern.collaboration_style,
            &pattern.tool_strategy,
            &pattern.output_kind,
        ));
        pattern.updated_at = Utc::now();
        self.upsert_pattern(&pattern).await?;
        Ok(pattern)
    }

    async fn list_pattern_evidence(
        &self,
        id: WorkflowPatternId,
    ) -> Result<WorkflowPatternEvidence, AppError> {
        let pattern = self.get_pattern(id).await?;
        Ok(WorkflowPatternEvidence {
            pattern_id: pattern.id,
            success_count: pattern.success_count,
            failure_count: pattern.failure_count,
            evidence_count: pattern.evidence_count,
            confidence: pattern.confidence,
            last_success_at: pattern.last_success_at,
            last_failure_at: pattern.last_failure_at,
            last_used_at: pattern.last_used_at,
            status: pattern.status,
        })
    }

    async fn find_by_fingerprint(
        &self,
        fingerprint: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Option<WorkflowPattern>, AppError> {
        let row = if let Some(ws) = workspace_id {
            sqlx::query_as::<_, PatternRow>(
                r"
                SELECT * FROM workflow_patterns
                WHERE fingerprint = ? AND (workspace_id = ? OR workspace_id IS NULL)
                ORDER BY updated_at DESC
                LIMIT 1
                ",
            )
            .bind(fingerprint)
            .bind(ws.0.to_string())
            .fetch_optional(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, PatternRow>(
                r"
                SELECT * FROM workflow_patterns
                WHERE fingerprint = ?
                ORDER BY updated_at DESC
                LIMIT 1
                ",
            )
            .bind(fingerprint)
            .fetch_optional(&self.pool)
            .await
        }
        .map_err(|e| AppError::Internal {
            message: format!("find_by_fingerprint failed: {e}"),
        })?;
        row.map(PatternRow::into_pattern).transpose()
    }
}

impl SqlitePatternStore {
    async fn recall_with_statuses(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
        statuses: &[&str],
    ) -> Result<Vec<PatternHint>, AppError> {
        let limit = limit.clamp(1, 20);
        let status_list = statuses
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(",");
        // status_list is built from static &str slices only.
        let sql = format!(
            r"
            SELECT * FROM workflow_patterns
            WHERE status IN ({status_list})
              AND (scope = 'User' OR (scope = 'Workspace' AND workspace_id = ?))
            ORDER BY strength DESC, confidence DESC, updated_at DESC
            LIMIT 80
            "
        );
        let mut rows = sqlx::query_as::<_, PatternRow>(&sql)
            .bind(workspace_id.map(|id| id.0.to_string()))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("recall_patterns query failed: {e}"),
            })?;

        if workspace_id.is_some() {
            let user_sql = format!(
                r"
                SELECT * FROM workflow_patterns
                WHERE status IN ({status_list}) AND scope = 'User' AND workspace_id IS NULL
                ORDER BY strength DESC, updated_at DESC
                LIMIT 50
                "
            );
            let user_rows = sqlx::query_as::<_, PatternRow>(&user_sql)
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
                    b.strength
                        .partial_cmp(&a.strength)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        hints.dedup_by(|a, b| a.id == b.id);
        hints.truncate(limit);
        Ok(hints)
    }
}

fn apply_success_or_failure(pattern: &mut WorkflowPattern, success: bool, now: chrono::DateTime<Utc>) {
    if success {
        pattern.success_count = pattern.success_count.saturating_add(1);
        pattern.evidence_count = pattern.evidence_count.saturating_add(1);
        pattern.strength = (pattern.strength + 0.15).min(10.0);
        pattern.confidence = (pattern.confidence + 0.12).min(1.0);
        pattern.last_success_at = Some(now);
    } else {
        pattern.failure_count = pattern.failure_count.saturating_add(1);
        pattern.strength = (pattern.strength - 0.2).max(0.1);
        pattern.confidence = (pattern.confidence - 0.15).max(0.0);
        pattern.last_failure_at = Some(now);
    }
    pattern.last_used_at = Some(now);
    pattern.updated_at = now;
    // Auto-promote Suggested after enough independent successes with no failures.
    if pattern.status == WorkflowPatternStatus::Suggested
        && pattern.evidence_count >= AUTO_PROMOTE_EVIDENCE_THRESHOLD
        && pattern.failure_count == 0
    {
        pattern.status = WorkflowPatternStatus::Active;
        pattern.confidence = pattern.confidence.max(0.7);
    }
}

fn infer_task_kind(task: &str) -> String {
    let lower = task.to_lowercase();
    if lower.contains("review") || lower.contains("审计") || lower.contains("审查") {
        "review".into()
    } else if lower.contains("test") || lower.contains("测试") {
        "test".into()
    } else if lower.contains("refactor") || lower.contains("重构") {
        "refactor".into()
    } else if lower.contains("implement") || lower.contains("实现") || lower.contains("开发") {
        "implement".into()
    } else if lower.contains("debug") || lower.contains("fix") || lower.contains("排查") {
        "debug".into()
    } else {
        "general".into()
    }
}

fn infer_tool_strategy(task: &str, roles: &[String]) -> String {
    let lower = task.to_lowercase();
    let joined = roles.join(" ").to_lowercase();
    if lower.contains("test") || joined.contains("tester") {
        "explore-edit-test".into()
    } else if joined.contains("explorer") || lower.contains("explore") {
        "explore-then-edit".into()
    } else {
        "default".into()
    }
}

fn infer_output_kind(task: &str) -> String {
    let lower = task.to_lowercase();
    if lower.contains("pr") || lower.contains("pull request") {
        "pr".into()
    } else if lower.contains("report") || lower.contains("报告") {
        "report".into()
    } else if lower.contains("diff") || lower.contains("patch") {
        "patch".into()
    } else {
        "answer".into()
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
    if !pattern.task_kind.is_empty() && task_lower.contains(&pattern.task_kind) {
        score += 0.8;
    }
    // Always give a small base score to strong active habits so they surface.
    if score == 0.0 && pattern.strength >= 2.0 {
        score = 0.25;
    }
    if score == 0.0 && pattern.confidence >= 0.5 {
        score = 0.2;
    }
    let conf_boost = 1.0 + pattern.confidence;
    score * (1.0 + pattern.strength.ln_1p()) * conf_boost
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
    fingerprint: Option<String>,
    evidence_count: Option<i64>,
    confidence: Option<f64>,
    last_success_at: Option<String>,
    last_failure_at: Option<String>,
    tool_strategy: Option<String>,
    output_kind: Option<String>,
    task_kind: Option<String>,
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
            fingerprint: self.fingerprint,
            evidence_count: self.evidence_count.unwrap_or(0),
            confidence: self.confidence.unwrap_or(0.0),
            last_success_at: self.last_success_at.as_deref().map(parse_dt).transpose()?,
            last_failure_at: self.last_failure_at.as_deref().map(parse_dt).transpose()?,
            tool_strategy: self.tool_strategy.unwrap_or_default(),
            output_kind: self.output_kind.unwrap_or_default(),
            task_kind: self.task_kind.unwrap_or_default(),
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
        self.recall_status(task, workspace_id, limit, WorkflowPatternStatus::Active)
            .await
    }

    async fn shadow_match_patterns(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
    ) -> Result<Vec<PatternHint>, AppError> {
        self.recall_status(task, workspace_id, limit, WorkflowPatternStatus::Suggested)
            .await
    }

    async fn apply_outcome(&self, outcome: &OrchestrationOutcome) -> Result<(), AppError> {
        let now = Utc::now();
        if !outcome.pattern_ids.is_empty() {
            let mut guard = self.patterns.lock().map_err(|_| AppError::Internal {
                message: "pattern store lock poisoned".to_owned(),
            })?;
            for id in &outcome.pattern_ids {
                if let Some(p) = guard.iter_mut().find(|p| p.id == *id) {
                    apply_success_or_failure(p, outcome.success, now);
                }
            }
            return Ok(());
        }
        if !outcome.success || outcome.agent_names.is_empty() {
            return Ok(());
        }
        let roles = outcome.agent_names.clone();
        let task_kind = infer_task_kind(&outcome.task);
        let tool_strategy = infer_tool_strategy(&outcome.task, &roles);
        let output_kind = infer_output_kind(&outcome.task);
        let fingerprint = pattern_fingerprint(
            MemoryScope::Workspace,
            &task_kind,
            &roles,
            "",
            &tool_strategy,
            &output_kind,
        );
        let mut guard = self.patterns.lock().map_err(|_| AppError::Internal {
            message: "pattern store lock poisoned".to_owned(),
        })?;
        if let Some(existing) = guard.iter_mut().find(|p| p.fingerprint.as_deref() == Some(&fingerprint))
        {
            if existing.status == WorkflowPatternStatus::Rejected {
                return Ok(());
            }
            apply_success_or_failure(existing, true, now);
            return Ok(());
        }
        guard.push(WorkflowPattern {
            id: WorkflowPatternId::new(),
            scope: MemoryScope::Workspace,
            workspace_id: Some(outcome.workspace_id),
            name: format!("learned:{}", roles.join("+")),
            summary: outcome
                .result_summary
                .clone()
                .unwrap_or_else(|| "Auto-learned pattern".into()),
            trigger_text: outcome.task.chars().take(200).collect(),
            preferred_roles: roles,
            collaboration_style: String::new(),
            strength: 1.0,
            success_count: 1,
            failure_count: 0,
            last_used_at: Some(now),
            status: WorkflowPatternStatus::Suggested,
            fingerprint: Some(fingerprint),
            evidence_count: 1,
            confidence: 0.35,
            last_success_at: Some(now),
            last_failure_at: None,
            tool_strategy,
            output_kind,
            task_kind,
            created_at: now,
            updated_at: now,
        });
        Ok(())
    }

    async fn mute_pattern(&self, id: WorkflowPatternId) -> Result<(), AppError> {
        let mut p = self.get_pattern(id).await?;
        p.status = WorkflowPatternStatus::Muted;
        p.updated_at = Utc::now();
        self.upsert_pattern(&p).await
    }

    async fn accept_pattern(&self, id: WorkflowPatternId) -> Result<WorkflowPattern, AppError> {
        let mut p = self.get_pattern(id).await?;
        p.status = WorkflowPatternStatus::Active;
        p.confidence = p.confidence.max(0.7);
        p.updated_at = Utc::now();
        self.upsert_pattern(&p).await?;
        Ok(p)
    }

    async fn reject_pattern(&self, id: WorkflowPatternId) -> Result<WorkflowPattern, AppError> {
        let mut p = self.get_pattern(id).await?;
        p.status = WorkflowPatternStatus::Rejected;
        p.updated_at = Utc::now();
        self.upsert_pattern(&p).await?;
        Ok(p)
    }

    async fn edit_pattern(
        &self,
        id: WorkflowPatternId,
        patch: WorkflowPatternPatch,
    ) -> Result<WorkflowPattern, AppError> {
        let mut p = self.get_pattern(id).await?;
        if let Some(name) = patch.name {
            p.name = name;
        }
        if let Some(summary) = patch.summary {
            p.summary = summary;
        }
        if let Some(trigger_text) = patch.trigger_text {
            p.trigger_text = trigger_text;
        }
        if let Some(preferred_roles) = patch.preferred_roles {
            p.preferred_roles = preferred_roles;
        }
        if let Some(collaboration_style) = patch.collaboration_style {
            p.collaboration_style = collaboration_style;
        }
        if let Some(tool_strategy) = patch.tool_strategy {
            p.tool_strategy = tool_strategy;
        }
        if let Some(output_kind) = patch.output_kind {
            p.output_kind = output_kind;
        }
        p.fingerprint = Some(pattern_fingerprint(
            p.scope,
            &p.task_kind,
            &p.preferred_roles,
            &p.collaboration_style,
            &p.tool_strategy,
            &p.output_kind,
        ));
        p.updated_at = Utc::now();
        self.upsert_pattern(&p).await?;
        Ok(p)
    }

    async fn list_pattern_evidence(
        &self,
        id: WorkflowPatternId,
    ) -> Result<WorkflowPatternEvidence, AppError> {
        let p = self.get_pattern(id).await?;
        Ok(WorkflowPatternEvidence {
            pattern_id: p.id,
            success_count: p.success_count,
            failure_count: p.failure_count,
            evidence_count: p.evidence_count,
            confidence: p.confidence,
            last_success_at: p.last_success_at,
            last_failure_at: p.last_failure_at,
            last_used_at: p.last_used_at,
            status: p.status,
        })
    }

    async fn find_by_fingerprint(
        &self,
        fingerprint: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Option<WorkflowPattern>, AppError> {
        Ok(self
            .patterns
            .lock()
            .map_err(|_| AppError::Internal {
                message: "pattern store lock poisoned".to_owned(),
            })?
            .iter()
            .find(|p| {
                p.fingerprint.as_deref() == Some(fingerprint)
                    && (workspace_id.is_none()
                        || p.workspace_id == workspace_id
                        || p.workspace_id.is_none())
            })
            .cloned())
    }
}

impl InMemoryPatternStore {
    async fn recall_status(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
        status: WorkflowPatternStatus,
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
                p.status == status
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recall_scores_trigger_overlap() {
        let store = InMemoryPatternStore::new();
        let ws = WorkspaceId::new();
        let now = Utc::now();
        store
            .upsert_pattern(&WorkflowPattern {
                id: WorkflowPatternId::new(),
                scope: MemoryScope::Workspace,
                workspace_id: Some(ws),
                name: "security".into(),
                summary: "security review".into(),
                trigger_text: "安全 审计 security".into(),
                preferred_roles: vec!["Explorer".into(), "Reviewer".into()],
                collaboration_style: String::new(),
                strength: 2.0,
                success_count: 3,
                failure_count: 0,
                last_used_at: None,
                status: WorkflowPatternStatus::Active,
                fingerprint: None,
                evidence_count: 3,
                confidence: 0.8,
                last_success_at: None,
                last_failure_at: None,
                tool_strategy: "explore-then-edit".into(),
                output_kind: "report".into(),
                task_kind: "review".into(),
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        let hints = store.recall_patterns("请做一次安全审计", Some(ws), 5).await.unwrap();
        assert!(!hints.is_empty());
        assert!(hints[0].score > 0.0);
    }

    #[tokio::test]
    async fn apply_outcome_dedupes_by_fingerprint() {
        let store = InMemoryPatternStore::new();
        let ws = WorkspaceId::new();
        let outcome = OrchestrationOutcome {
            workspace_id: ws,
            task: "implement feature X with explorer".into(),
            success: true,
            agent_names: vec!["Explorer".into(), "Coder".into()],
            pattern_ids: vec![],
            result_summary: Some("ok".into()),
        };
        store.apply_outcome(&outcome).await.unwrap();
        store.apply_outcome(&outcome).await.unwrap();
        store.apply_outcome(&outcome).await.unwrap();
        let listed = store.list_patterns(MemoryScope::Workspace, Some(ws)).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].evidence_count, 3);
        assert_eq!(listed[0].status, WorkflowPatternStatus::Active);
    }

    #[tokio::test]
    async fn suggested_not_recalled_for_execution() {
        let store = InMemoryPatternStore::new();
        let ws = WorkspaceId::new();
        let now = Utc::now();
        store
            .upsert_pattern(&WorkflowPattern {
                id: WorkflowPatternId::new(),
                scope: MemoryScope::Workspace,
                workspace_id: Some(ws),
                name: "suggested-only".into(),
                summary: "s".into(),
                trigger_text: "deploy production".into(),
                preferred_roles: vec!["Planner".into()],
                collaboration_style: String::new(),
                strength: 1.0,
                success_count: 1,
                failure_count: 0,
                last_used_at: None,
                status: WorkflowPatternStatus::Suggested,
                fingerprint: Some("fp1".into()),
                evidence_count: 1,
                confidence: 0.3,
                last_success_at: None,
                last_failure_at: None,
                tool_strategy: String::new(),
                output_kind: String::new(),
                task_kind: "general".into(),
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        let active = store.recall_patterns("deploy production", Some(ws), 5).await.unwrap();
        assert!(active.is_empty());
        let shadow = store
            .shadow_match_patterns("deploy production", Some(ws), 5)
            .await
            .unwrap();
        assert_eq!(shadow.len(), 1);
    }

    #[tokio::test]
    async fn accept_promotes_to_active() {
        let store = InMemoryPatternStore::new();
        let now = Utc::now();
        let id = WorkflowPatternId::new();
        store
            .upsert_pattern(&WorkflowPattern {
                id,
                scope: MemoryScope::User,
                workspace_id: None,
                name: "n".into(),
                summary: "s".into(),
                trigger_text: "t".into(),
                preferred_roles: vec![],
                collaboration_style: String::new(),
                strength: 1.0,
                success_count: 1,
                failure_count: 0,
                last_used_at: None,
                status: WorkflowPatternStatus::Suggested,
                fingerprint: None,
                evidence_count: 1,
                confidence: 0.3,
                last_success_at: None,
                last_failure_at: None,
                tool_strategy: String::new(),
                output_kind: String::new(),
                task_kind: String::new(),
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        let accepted = store.accept_pattern(id).await.unwrap();
        assert_eq!(accepted.status, WorkflowPatternStatus::Active);
    }
}
