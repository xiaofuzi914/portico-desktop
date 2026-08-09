//! Memory candidate store: proposed habits awaiting user review.

use app_models::{
    AgentRunId, AppError, CandidateStatus, MemoryCandidate, MemoryCandidateId, MemoryKind,
    MemoryScope, ThreadId, WorkspaceId,
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;

/// Persistence port for memory candidates.
#[async_trait]
pub trait CandidateStore: Send + Sync {
    /// Insert a candidate if the same (scope, fingerprint, status) does not exist.
    ///
    /// Rejected fingerprints of the same scope suppress new Proposed rows.
    async fn upsert_proposed(&self, candidate: &MemoryCandidate) -> Result<bool, AppError>;

    /// List candidates filtered by optional status and workspace.
    async fn list(
        &self,
        status: Option<CandidateStatus>,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
    ) -> Result<Vec<MemoryCandidate>, AppError>;

    /// Fetch one candidate.
    async fn get(&self, id: MemoryCandidateId) -> Result<MemoryCandidate, AppError>;

    /// Accept a candidate (optionally with edited fields). Returns the updated row.
    async fn accept(
        &self,
        id: MemoryCandidateId,
        edited_value: Option<String>,
        scope: Option<MemoryScope>,
        sensitive: Option<bool>,
    ) -> Result<MemoryCandidate, AppError>;

    /// Reject a candidate.
    async fn reject(&self, id: MemoryCandidateId) -> Result<MemoryCandidate, AppError>;

    /// Expire a candidate.
    async fn expire(&self, id: MemoryCandidateId) -> Result<MemoryCandidate, AppError>;

    /// List candidates produced by a run.
    async fn list_for_run(&self, run_id: AgentRunId) -> Result<Vec<MemoryCandidate>, AppError>;

    /// Whether a rejected fingerprint exists for the scope.
    async fn is_fingerprint_rejected(
        &self,
        scope: MemoryScope,
        fingerprint: &str,
    ) -> Result<bool, AppError>;
}

/// SQLite-backed [`CandidateStore`].
#[derive(Debug, Clone)]
pub struct SqliteCandidateStore {
    pool: SqlitePool,
}

impl SqliteCandidateStore {
    /// Create a store over an existing pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CandidateStore for SqliteCandidateStore {
    async fn upsert_proposed(&self, candidate: &MemoryCandidate) -> Result<bool, AppError> {
        if self
            .is_fingerprint_rejected(candidate.scope, &candidate.fingerprint)
            .await?
        {
            return Ok(false);
        }

        // Already proposed with same fingerprint — keep existing.
        let existing = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(1) FROM memory_candidates
            WHERE scope = ? AND fingerprint = ? AND status = 'Proposed'
            ",
        )
        .bind(candidate.scope.as_str())
        .bind(&candidate.fingerprint)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("check candidate fingerprint failed: {e}"),
        })?;
        if existing > 0 {
            return Ok(false);
        }

        let evidence = serde_json::to_string(&candidate.evidence).map_err(|e| AppError::Internal {
            message: format!("serialize candidate evidence failed: {e}"),
        })?;
        let result = sqlx::query(
            r"
            INSERT INTO memory_candidates (
                id, run_id, workspace_id, thread_id, scope, kind, key, value,
                fingerprint, confidence, sensitive, evidence_json, status,
                extractor_version, created_at, reviewed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(scope, fingerprint, status) DO NOTHING
            ",
        )
        .bind(candidate.id.0)
        .bind(candidate.run_id.0)
        .bind(candidate.workspace_id.map(|w| w.0))
        .bind(candidate.thread_id.map(|t| t.0))
        .bind(candidate.scope.as_str())
        .bind(candidate.kind.as_str())
        .bind(&candidate.key)
        .bind(&candidate.value)
        .bind(&candidate.fingerprint)
        .bind(candidate.confidence)
        .bind(i64::from(candidate.sensitive))
        .bind(evidence)
        .bind(candidate.status.as_str())
        .bind(i64::from(candidate.extractor_version))
        .bind(candidate.created_at)
        .bind(candidate.reviewed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("insert memory candidate failed: {e}"),
        })?;
        Ok(result.rows_affected() > 0)
    }

    async fn list(
        &self,
        status: Option<CandidateStatus>,
        workspace_id: Option<WorkspaceId>,
        limit: usize,
    ) -> Result<Vec<MemoryCandidate>, AppError> {
        let limit = i64::try_from(limit.clamp(1, 200)).unwrap_or(50);
        let rows = match (status, workspace_id) {
            (Some(st), Some(ws)) => {
                sqlx::query_as::<_, CandidateRow>(
                    r"
                    SELECT * FROM memory_candidates
                    WHERE status = ? AND (workspace_id = ? OR workspace_id IS NULL)
                    ORDER BY created_at DESC LIMIT ?
                    ",
                )
                .bind(st.as_str())
                .bind(ws.0)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            (Some(st), None) => {
                sqlx::query_as::<_, CandidateRow>(
                    r"
                    SELECT * FROM memory_candidates
                    WHERE status = ?
                    ORDER BY created_at DESC LIMIT ?
                    ",
                )
                .bind(st.as_str())
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            (None, Some(ws)) => {
                sqlx::query_as::<_, CandidateRow>(
                    r"
                    SELECT * FROM memory_candidates
                    WHERE workspace_id = ? OR workspace_id IS NULL
                    ORDER BY created_at DESC LIMIT ?
                    ",
                )
                .bind(ws.0)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            (None, None) => {
                sqlx::query_as::<_, CandidateRow>(
                    r"
                    SELECT * FROM memory_candidates
                    ORDER BY created_at DESC LIMIT ?
                    ",
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(|e| AppError::Internal {
            message: format!("list memory candidates failed: {e}"),
        })?;
        rows.into_iter().map(CandidateRow::into_candidate).collect()
    }

    async fn get(&self, id: MemoryCandidateId) -> Result<MemoryCandidate, AppError> {
        let row = sqlx::query_as::<_, CandidateRow>(
            "SELECT * FROM memory_candidates WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get memory candidate failed: {e}"),
        })?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory_candidate:{}", id.0),
        })?;
        row.into_candidate()
    }

    async fn accept(
        &self,
        id: MemoryCandidateId,
        edited_value: Option<String>,
        scope: Option<MemoryScope>,
        sensitive: Option<bool>,
    ) -> Result<MemoryCandidate, AppError> {
        let mut candidate = self.get(id).await?;
        if candidate.status != CandidateStatus::Proposed {
            return Err(AppError::Internal {
                message: format!(
                    "candidate {} is not Proposed (status={})",
                    id.0,
                    candidate.status.as_str()
                ),
            });
        }
        if let Some(value) = edited_value {
            candidate.value = value;
        }
        if let Some(scope) = scope {
            candidate.scope = scope;
        }
        if let Some(sensitive) = sensitive {
            candidate.sensitive = sensitive;
        }
        candidate.status = CandidateStatus::Accepted;
        candidate.reviewed_at = Some(Utc::now());
        self.update_row(&candidate).await?;
        Ok(candidate)
    }

    async fn reject(&self, id: MemoryCandidateId) -> Result<MemoryCandidate, AppError> {
        let mut candidate = self.get(id).await?;
        candidate.status = CandidateStatus::Rejected;
        candidate.reviewed_at = Some(Utc::now());
        self.update_row(&candidate).await?;
        Ok(candidate)
    }

    async fn expire(&self, id: MemoryCandidateId) -> Result<MemoryCandidate, AppError> {
        let mut candidate = self.get(id).await?;
        candidate.status = CandidateStatus::Expired;
        candidate.reviewed_at = Some(Utc::now());
        self.update_row(&candidate).await?;
        Ok(candidate)
    }

    async fn list_for_run(&self, run_id: AgentRunId) -> Result<Vec<MemoryCandidate>, AppError> {
        let rows = sqlx::query_as::<_, CandidateRow>(
            r"
            SELECT * FROM memory_candidates
            WHERE run_id = ?
            ORDER BY created_at DESC
            ",
        )
        .bind(run_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list candidates for run failed: {e}"),
        })?;
        rows.into_iter().map(CandidateRow::into_candidate).collect()
    }

    async fn is_fingerprint_rejected(
        &self,
        scope: MemoryScope,
        fingerprint: &str,
    ) -> Result<bool, AppError> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            SELECT COUNT(1) FROM memory_candidates
            WHERE scope = ? AND fingerprint = ? AND status = 'Rejected'
            ",
        )
        .bind(scope.as_str())
        .bind(fingerprint)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("check rejected fingerprint failed: {e}"),
        })?;
        Ok(count > 0)
    }
}

impl SqliteCandidateStore {
    async fn update_row(&self, candidate: &MemoryCandidate) -> Result<(), AppError> {
        let evidence = serde_json::to_string(&candidate.evidence).map_err(|e| AppError::Internal {
            message: format!("serialize candidate evidence failed: {e}"),
        })?;
        let result = sqlx::query(
            r"
            UPDATE memory_candidates SET
                scope = ?, kind = ?, key = ?, value = ?,
                confidence = ?, sensitive = ?, evidence_json = ?,
                status = ?, reviewed_at = ?
            WHERE id = ?
            ",
        )
        .bind(candidate.scope.as_str())
        .bind(candidate.kind.as_str())
        .bind(&candidate.key)
        .bind(&candidate.value)
        .bind(candidate.confidence)
        .bind(i64::from(candidate.sensitive))
        .bind(evidence)
        .bind(candidate.status.as_str())
        .bind(candidate.reviewed_at)
        .bind(candidate.id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("update memory candidate failed: {e}"),
        })?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("memory_candidate:{}", candidate.id.0),
            });
        }
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct CandidateRow {
    id: uuid::Uuid,
    run_id: uuid::Uuid,
    workspace_id: Option<uuid::Uuid>,
    thread_id: Option<uuid::Uuid>,
    scope: String,
    kind: String,
    key: String,
    value: String,
    fingerprint: String,
    confidence: f64,
    sensitive: i64,
    evidence_json: String,
    status: String,
    extractor_version: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl CandidateRow {
    fn into_candidate(self) -> Result<MemoryCandidate, AppError> {
        let evidence: Vec<String> =
            serde_json::from_str(&self.evidence_json).unwrap_or_default();
        Ok(MemoryCandidate {
            id: MemoryCandidateId(self.id),
            run_id: AgentRunId(self.run_id),
            workspace_id: self.workspace_id.map(WorkspaceId),
            thread_id: self.thread_id.map(ThreadId),
            scope: MemoryScope::try_from(self.scope.as_str())?,
            kind: MemoryKind::try_from(self.kind.as_str())?,
            key: self.key,
            value: self.value,
            fingerprint: self.fingerprint,
            confidence: self.confidence,
            sensitive: self.sensitive != 0,
            evidence,
            status: CandidateStatus::try_from(self.status.as_str())?,
            extractor_version: u32::try_from(self.extractor_version).unwrap_or(1),
            created_at: self.created_at,
            reviewed_at: self.reviewed_at,
        })
    }
}

/// Compute a stable content fingerprint for candidate dedup.
#[must_use]
pub fn candidate_fingerprint(scope: MemoryScope, kind: MemoryKind, key: &str, value: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    scope.as_str().hash(&mut hasher);
    kind.as_str().hash(&mut hasher);
    normalize_text(key).hash(&mut hasher);
    normalize_text(value).hash(&mut hasher);
    format!("mc_{:016x}", hasher.finish())
}

fn normalize_text(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
