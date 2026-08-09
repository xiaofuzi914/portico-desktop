//! Local learning pipeline: ExperienceEvent → candidates / pattern evidence.
//!
//! Learning failures must never change the user-visible run status.

use crate::outcome_evaluator::{evaluate_outcome, provisional_outcome, OutcomeEvidence};
use crate::storage::Storage;
use crate::task_queue::BackgroundTaskQueue;
use app_memory::{
    extract_from_experience, CandidateStore, ExperienceStore, PatternStore, SqliteCandidateStore,
    SqliteExperienceStore, EXPERIENCE_SCHEMA_VERSION,
};
use app_models::{
    AgentRunId, AgentRunStatus, AppError, BehaviorPolicy, ContextItemDisposition,
    ContextSnapshotItem, ExperienceEvent, ExperienceEventId, ExecutionMode, LearningDataExport,
    LearningOverview, LearningQueueStatus, MemoryCandidate, MemoryId, MemoryScope, OutcomeSignal,
    PrivacySettings, RunContextSnapshot, RunFeedback, RunFeedbackRating, RunLearningSummary,
    TaskKind, ThreadId, ToolUsageSummary, WorkflowPattern, WorkflowPatternId, WorkspaceId,
};
use chrono::Utc;
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::{info, warn};

/// Coordinates experience persistence and background learning jobs.
#[derive(Clone)]
pub struct LearningCoordinator {
    pool: SqlitePool,
    storage: Arc<dyn Storage>,
    task_queue: BackgroundTaskQueue,
    experience: Arc<dyn ExperienceStore>,
    candidates: Arc<dyn CandidateStore>,
    patterns: Arc<dyn PatternStore>,
}

impl LearningCoordinator {
    /// Build a coordinator sharing the app DB pool.
    #[must_use]
    pub fn new(
        pool: SqlitePool,
        storage: Arc<dyn Storage>,
        task_queue: BackgroundTaskQueue,
        patterns: Arc<dyn PatternStore>,
    ) -> Self {
        Self {
            experience: Arc::new(SqliteExperienceStore::new(pool.clone())),
            candidates: Arc::new(SqliteCandidateStore::new(pool.clone())),
            patterns,
            pool,
            storage,
            task_queue,
        }
    }

    /// Access the candidate store (for IPC).
    #[must_use]
    pub fn candidates(&self) -> Arc<dyn CandidateStore> {
        self.candidates.clone()
    }

    /// Access the experience store (for IPC).
    #[must_use]
    pub fn experience(&self) -> Arc<dyn ExperienceStore> {
        self.experience.clone()
    }

    /// Access the pattern store (for IPC).
    #[must_use]
    pub fn patterns(&self) -> Arc<dyn PatternStore> {
        self.patterns.clone()
    }

    /// Called after a run reaches a terminal state. Never fails the caller.
    pub async fn on_run_terminal(
        &self,
        run_id: AgentRunId,
        status: AgentRunStatus,
        execution_mode: ExecutionMode,
        pattern_ids: Vec<WorkflowPatternId>,
        roles: Vec<String>,
        result_summary: Option<String>,
    ) {
        if let Err(err) = self
            .on_run_terminal_inner(
                run_id,
                status,
                execution_mode,
                pattern_ids,
                roles,
                result_summary,
            )
            .await
        {
            warn!(run_id = %run_id.0, error = %err, "learning on_run_terminal failed (non-fatal)");
        }
    }

    async fn on_run_terminal_inner(
        &self,
        run_id: AgentRunId,
        status: AgentRunStatus,
        execution_mode: ExecutionMode,
        pattern_ids: Vec<WorkflowPatternId>,
        roles: Vec<String>,
        result_summary: Option<String>,
    ) -> Result<(), AppError> {
        if !status.is_terminal() {
            return Ok(());
        }

        let run = self.storage.get_run(run_id).await?;
        let messages = self.storage.list_messages(run.thread_id).await.unwrap_or_default();
        let task_text = messages
            .iter()
            .rev()
            .find(|m| m.run_id == Some(run_id) && m.role == app_models::MessageRole::User)
            .map(|m| m.content.clone())
            .or_else(|| {
                messages
                    .iter()
                    .rev()
                    .find(|m| m.role == app_models::MessageRole::User)
                    .map(|m| m.content.clone())
            })
            .unwrap_or_default();

        let tools_used = self.collect_tool_usage(run_id).await.unwrap_or_default();
        // Artifact paths and model snapshots are optional enrichment; learning
        // must work without them (no Storage coupling to those tables).
        let artifact_paths: Vec<String> = Vec::new();
        let model_snapshot = None;

        let feedback = self.get_run_feedback(run_id).await.ok().flatten();
        let has_test_success = tools_used.iter().any(|t| {
            let name = t.tool_name.to_lowercase();
            (name.contains("shell") || name.contains("test")) && t.success_count > 0
        });
        let outcome = evaluate_outcome(&OutcomeEvidence {
            terminal_status: Some(status),
            tools: tools_used.clone(),
            has_artifacts: !artifact_paths.is_empty(),
            has_test_success,
            immediate_retry: false,
            feedback: feedback.clone(),
        });

        let event = ExperienceEvent {
            id: ExperienceEventId::new(),
            run_id,
            workspace_id: run.workspace_id,
            thread_id: run.thread_id,
            task_text,
            task_kind: TaskKind::AgentRun,
            execution_mode,
            model_snapshot,
            roles,
            tools_used,
            pattern_ids,
            terminal_status: status,
            outcome,
            result_summary,
            artifact_paths,
            schema_version: EXPERIENCE_SCHEMA_VERSION,
            created_at: Utc::now(),
        };

        self.experience.upsert_event(&event).await?;

        // Enqueue learning job (idempotent processing via experience upsert + candidate fingerprint).
        let payload = json!({
            "run_id": run_id.0.to_string(),
            "schema_version": EXPERIENCE_SCHEMA_VERSION,
        });
        let _ = self
            .task_queue
            .enqueue(
                run.workspace_id,
                Some(run.thread_id),
                Some(run_id),
                TaskKind::LearnFromRun,
                payload,
                10,
                None,
                Some(5),
            )
            .await;

        // Process immediately as well so UI sees candidates without waiting for a worker.
        // Safe because upserts are idempotent.
        self.process_experience(&event).await?;
        Ok(())
    }

    /// Process a learning job payload (also used by the background worker).
    pub async fn process_learning_job(&self, payload: &serde_json::Value) -> Result<(), AppError> {
        let run_id_str = payload
            .get("run_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Internal {
                message: "LearnFromRun payload missing run_id".into(),
            })?;
        let run_id = uuid::Uuid::parse_str(run_id_str)
            .map(AgentRunId)
            .map_err(|e| AppError::Internal {
                message: format!("invalid run_id in learning job: {e}"),
            })?;
        let schema_version = payload
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .map_or(EXPERIENCE_SCHEMA_VERSION, |v| v as u32);

        let Some(event) = self.experience.get_by_run(run_id, schema_version).await? else {
            return Err(AppError::NotFound {
                resource: format!("experience_event for run {}", run_id.0),
            });
        };
        self.process_experience(&event).await
    }

    async fn process_experience(&self, event: &ExperienceEvent) -> Result<(), AppError> {
        // 1) Candidate memory extraction (rules only; never writes long-term memory).
        let candidates = extract_from_experience(event);
        let mut inserted = 0u32;
        for candidate in candidates {
            if self.candidates.upsert_proposed(&candidate).await? {
                inserted += 1;
            }
        }

        // 2) Pattern evidence for multi-agent style outcomes when roles present.
        if event.execution_mode == ExecutionMode::MultiAgent
            || (!event.roles.is_empty() && event.roles.iter().any(|r| r != "Default"))
        {
            let success = event.outcome.is_positive_evidence()
                || (event.outcome == OutcomeSignal::Unknown
                    && event.terminal_status == AgentRunStatus::Completed);
            let _ = self
                .patterns
                .apply_outcome(&app_models::OrchestrationOutcome {
                    workspace_id: event.workspace_id,
                    task: event.task_text.clone(),
                    success,
                    agent_names: event.roles.clone(),
                    pattern_ids: event.pattern_ids.clone(),
                    result_summary: event.result_summary.clone(),
                })
                .await;
        }

        info!(
            run_id = %event.run_id.0,
            candidates_inserted = inserted,
            outcome = event.outcome.as_str(),
            "learning processed experience event"
        );
        Ok(())
    }

    async fn collect_tool_usage(&self, run_id: AgentRunId) -> Result<Vec<ToolUsageSummary>, AppError> {
        let invocations = self.storage.list_tool_invocations(run_id).await?;
        let mut map: std::collections::BTreeMap<String, ToolUsageSummary> =
            std::collections::BTreeMap::new();
        for inv in invocations {
            let entry = map.entry(inv.tool_name.clone()).or_insert_with(|| ToolUsageSummary {
                tool_name: inv.tool_name.clone(),
                call_count: 0,
                success_count: 0,
                failure_count: 0,
            });
            entry.call_count += 1;
            match inv.status {
                app_models::ToolInvocationStatus::Succeeded => entry.success_count += 1,
                app_models::ToolInvocationStatus::Failed => entry.failure_count += 1,
                _ => {}
            }
        }
        Ok(map.into_values().collect())
    }

    /// Persist user feedback and re-evaluate outcome when possible.
    pub async fn submit_run_feedback(
        &self,
        run_id: AgentRunId,
        rating: RunFeedbackRating,
        comment: Option<String>,
    ) -> Result<RunFeedback, AppError> {
        let feedback = RunFeedback {
            run_id,
            rating,
            comment,
            created_at: Utc::now(),
        };
        sqlx::query(
            r"
            INSERT INTO run_feedback (run_id, rating, comment, created_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(run_id) DO UPDATE SET
                rating = excluded.rating,
                comment = excluded.comment,
                created_at = excluded.created_at
            ",
        )
        .bind(run_id.0)
        .bind(feedback.rating.as_str())
        .bind(&feedback.comment)
        .bind(feedback.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("submit_run_feedback failed: {e}"),
        })?;

        // Re-score experience if present and re-process learning.
        if let Some(mut event) = self
            .experience
            .get_by_run(run_id, EXPERIENCE_SCHEMA_VERSION)
            .await?
        {
            let tools = event.tools_used.clone();
            event.outcome = evaluate_outcome(&OutcomeEvidence {
                terminal_status: Some(event.terminal_status),
                tools,
                has_artifacts: !event.artifact_paths.is_empty(),
                has_test_success: false,
                immediate_retry: false,
                feedback: Some(feedback.clone()),
            });
            self.experience.upsert_event(&event).await?;
            let _ = self.process_experience(&event).await;

            // Negative feedback lowers pattern confidence.
            if rating == RunFeedbackRating::NotHelpful {
                for pid in &event.pattern_ids {
                    let _ = self
                        .patterns
                        .apply_outcome(&app_models::OrchestrationOutcome {
                            workspace_id: event.workspace_id,
                            task: event.task_text.clone(),
                            success: false,
                            agent_names: event.roles.clone(),
                            pattern_ids: vec![*pid],
                            result_summary: event.result_summary.clone(),
                        })
                        .await;
                }
            }
        }

        Ok(feedback)
    }

    /// Load feedback for a run.
    pub async fn get_run_feedback(&self, run_id: AgentRunId) -> Result<Option<RunFeedback>, AppError> {
        let row = sqlx::query_as::<_, FeedbackRow>(
            "SELECT run_id, rating, comment, created_at FROM run_feedback WHERE run_id = ?",
        )
        .bind(run_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_run_feedback failed: {e}"),
        })?;
        row.map(FeedbackRow::into_feedback).transpose()
    }

    /// Learning summary for Inspector.
    pub async fn get_run_learning_summary(
        &self,
        run_id: AgentRunId,
    ) -> Result<RunLearningSummary, AppError> {
        let experience = self
            .experience
            .get_by_run(run_id, EXPERIENCE_SCHEMA_VERSION)
            .await?;
        let candidates = self.candidates.list_for_run(run_id).await.unwrap_or_default();
        let feedback = self.get_run_feedback(run_id).await.ok().flatten();
        let snapshot = self.load_context_snapshot(run_id).await.unwrap_or_default();
        Ok(RunLearningSummary {
            run_id,
            experience,
            candidates,
            feedback,
            memory_ids_used: snapshot.memory_ids,
            pattern_ids_used: snapshot.pattern_ids,
            behavior_policy: snapshot.behavior_policy,
            outbound_manifest: snapshot.outbound_manifest,
            recall_scores: snapshot.recall_scores,
        })
    }

    /// Authoritative frozen context snapshot for Inspector (not a live re-query).
    pub async fn get_run_context_snapshot(
        &self,
        run_id: AgentRunId,
        memory_manager: &dyn app_memory::MemoryManager,
    ) -> Result<RunContextSnapshot, AppError> {
        let learning = self.get_run_learning_summary(run_id).await?;
        let snap = self.load_context_snapshot(run_id).await.unwrap_or_default();
        let mut items = Vec::new();

        // Map recalled memories to Sent; mark known privacy blocks.
        for (id, score) in &snap.recall_scores {
            let title = format!("memory:{}", id.0);
            items.push(ContextSnapshotItem {
                kind: "memory".into(),
                title: title.clone(),
                summary: title,
                disposition: ContextItemDisposition::Sent,
                reason: None,
                score: Some(*score),
                memory_id: Some(*id),
                pattern_id: None,
                path: None,
            });
        }
        // Memories used without scores still count as sent.
        for id in &snap.memory_ids {
            if snap.recall_scores.iter().any(|(mid, _)| mid == id) {
                continue;
            }
            items.push(ContextSnapshotItem {
                kind: "memory".into(),
                title: format!("memory:{}", id.0),
                summary: format!("memory {}", id.0),
                disposition: ContextItemDisposition::Sent,
                reason: None,
                score: None,
                memory_id: Some(*id),
                pattern_id: None,
                path: None,
            });
        }
        for pid in &snap.pattern_ids {
            items.push(ContextSnapshotItem {
                kind: "pattern".into(),
                title: format!("pattern:{}", pid.0),
                summary: format!("pattern {}", pid.0),
                disposition: ContextItemDisposition::Sent,
                reason: None,
                score: None,
                memory_id: None,
                pattern_id: Some(*pid),
                path: None,
            });
        }
        if let Some(manifest) = &snap.outbound_manifest {
            for path in &manifest.rag_paths {
                items.push(ContextSnapshotItem {
                    kind: "rag".into(),
                    title: path.clone(),
                    summary: path.clone(),
                    disposition: ContextItemDisposition::Sent,
                    reason: None,
                    score: None,
                    memory_id: None,
                    pattern_id: None,
                    path: Some(path.clone()),
                });
            }
            if manifest.sensitive_content_blocked {
                items.push(ContextSnapshotItem {
                    kind: "privacy".into(),
                    title: "sensitive_memory".into(),
                    summary: "Sensitive memory blocked from outbound request".into(),
                    disposition: ContextItemDisposition::BlockedSensitive,
                    reason: Some("sensitive".into()),
                    score: None,
                    memory_id: None,
                    pattern_id: None,
                    path: None,
                });
            }
        }

        // Enrich memory titles from manager when possible.
        if let Ok(run) = self.storage.get_run(run_id).await {
            if let Ok(all) = memory_manager
                .list_for_recall(run.workspace_id, run.thread_id)
                .await
            {
                for item in &mut items {
                    if let Some(mid) = item.memory_id {
                        if let Some(m) = all.iter().find(|x| x.id == mid) {
                            item.title = m.key.clone();
                            item.summary = if m.sensitive {
                                "[sensitive content hidden]".into()
                            } else {
                                m.value.chars().take(200).collect()
                            };
                            if m.sensitive {
                                item.disposition = ContextItemDisposition::BlockedSensitive;
                                item.reason = Some("sensitive".into());
                            }
                        }
                    }
                }
            }
        }

        // Pattern name enrichment.
        for item in &mut items {
            if let Some(pid) = item.pattern_id {
                if let Ok(p) = self.patterns.get_pattern(pid).await {
                    item.title = p.name.clone();
                    item.summary = p.summary.clone();
                }
            }
        }

        Ok(RunContextSnapshot {
            run_id,
            memory_ids: snap.memory_ids,
            pattern_ids: snap.pattern_ids,
            behavior_policy: snap.behavior_policy,
            outbound_manifest: snap.outbound_manifest,
            recall_scores: snap.recall_scores,
            items,
            learning: Some(learning),
        })
    }

    /// Export all local learning data as a single portable payload.
    pub async fn export_learning_data(
        &self,
        memory_manager: &dyn app_memory::MemoryManager,
    ) -> Result<LearningDataExport, AppError> {
        let mut memories = Vec::new();
        for scope in [
            MemoryScope::User,
            MemoryScope::Workspace,
            MemoryScope::Thread,
        ] {
            if let Ok(list) = memory_manager.list_memories(scope, None, None).await {
                memories.extend(list);
            }
        }
        // Also list workspace-scoped rows that have workspace_id set.
        // list_memories(Workspace, None) returns workspace-null rows; pull via SQL for completeness.
        let ws_rows = sqlx::query_as::<_, (uuid::Uuid,)>(
            "SELECT DISTINCT workspace_id FROM memories WHERE workspace_id IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();
        for (wid,) in ws_rows {
            if let Ok(list) = memory_manager
                .list_memories(MemoryScope::Workspace, Some(WorkspaceId(wid)), None)
                .await
            {
                for m in list {
                    if !memories.iter().any(|x| x.id == m.id) {
                        memories.push(m);
                    }
                }
            }
        }

        let candidates = self
            .candidates
            .list(None, None, 500)
            .await
            .unwrap_or_default();

        let mut patterns: Vec<WorkflowPattern> = Vec::new();
        if let Ok(user) = self.patterns.list_patterns(MemoryScope::User, None).await {
            patterns.extend(user);
        }
        let ws_pattern_ids = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT DISTINCT workspace_id FROM workflow_patterns",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();
        for (ws,) in ws_pattern_ids {
            let workspace_id = ws
                .as_deref()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .map(WorkspaceId);
            if let Ok(list) = self
                .patterns
                .list_patterns(MemoryScope::Workspace, workspace_id)
                .await
            {
                for p in list {
                    if !patterns.iter().any(|x| x.id == p.id) {
                        patterns.push(p);
                    }
                }
            }
        }

        let privacy = self.get_privacy_settings().await.unwrap_or_default();
        Ok(LearningDataExport {
            exported_at: Utc::now(),
            memories,
            candidates,
            patterns,
            privacy,
            schema_version: 1,
        })
    }

    /// Persist the frozen context/policy snapshot for a run.
    pub async fn save_context_snapshot(
        &self,
        run_id: AgentRunId,
        memory_ids: &[MemoryId],
        pattern_ids: &[WorkflowPatternId],
        policy: &BehaviorPolicy,
        outbound: Option<&app_models::OutboundContextManifest>,
        recall_scores: &[(MemoryId, f64)],
    ) -> Result<(), AppError> {
        let memory_ids_json = serde_json::to_string(memory_ids).unwrap_or_else(|_| "[]".into());
        let pattern_ids_json = serde_json::to_string(pattern_ids).unwrap_or_else(|_| "[]".into());
        let behavior_policy_json =
            serde_json::to_string(policy).unwrap_or_else(|_| "{}".into());
        let outbound_manifest_json = outbound
            .map(|m| serde_json::to_string(m).unwrap_or_else(|_| "null".into()));
        let recall_scores_json = serde_json::to_string(recall_scores).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            r"
            INSERT INTO run_context_snapshots (
                run_id, memory_ids_json, pattern_ids_json, behavior_policy_json,
                outbound_manifest_json, recall_scores_json, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id) DO UPDATE SET
                memory_ids_json = excluded.memory_ids_json,
                pattern_ids_json = excluded.pattern_ids_json,
                behavior_policy_json = excluded.behavior_policy_json,
                outbound_manifest_json = excluded.outbound_manifest_json,
                recall_scores_json = excluded.recall_scores_json
            ",
        )
        .bind(run_id.0)
        .bind(memory_ids_json)
        .bind(pattern_ids_json)
        .bind(behavior_policy_json)
        .bind(outbound_manifest_json)
        .bind(recall_scores_json)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("save_context_snapshot failed: {e}"),
        })?;
        Ok(())
    }

    async fn load_context_snapshot(&self, run_id: AgentRunId) -> Result<ContextSnapshot, AppError> {
        let row = sqlx::query_as::<_, SnapshotRow>(
            r"
            SELECT memory_ids_json, pattern_ids_json, behavior_policy_json,
                   outbound_manifest_json, recall_scores_json
            FROM run_context_snapshots WHERE run_id = ?
            ",
        )
        .bind(run_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("load_context_snapshot failed: {e}"),
        })?;
        let Some(row) = row else {
            return Ok(ContextSnapshot::default());
        };
        Ok(ContextSnapshot {
            memory_ids: serde_json::from_str(&row.memory_ids_json).unwrap_or_default(),
            pattern_ids: serde_json::from_str(&row.pattern_ids_json).unwrap_or_default(),
            behavior_policy: serde_json::from_str(&row.behavior_policy_json).ok(),
            outbound_manifest: row
                .outbound_manifest_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok()),
            recall_scores: serde_json::from_str(&row.recall_scores_json).unwrap_or_default(),
        })
    }

    /// Queue depth diagnostics.
    pub async fn get_learning_queue_status(&self) -> Result<LearningQueueStatus, AppError> {
        let queued = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM background_tasks WHERE task_kind = 'LearnFromRun' AND status = 'Queued'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let running = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM background_tasks WHERE task_kind = 'LearnFromRun' AND status = 'Running'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let failed = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM background_tasks WHERE task_kind = 'LearnFromRun' AND status = 'Failed'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        let completed_recent = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM background_tasks WHERE task_kind = 'LearnFromRun' AND status = 'Completed'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        Ok(LearningQueueStatus {
            queued: queued as u64,
            running: running as u64,
            failed: failed as u64,
            completed_recent: completed_recent as u64,
        })
    }

    /// Aggregate stats for the Memory Center overview tab.
    pub async fn get_learning_overview(
        &self,
        sensitive_encryption_enabled: bool,
    ) -> Result<LearningOverview, AppError> {
        let pending_candidates = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM memory_candidates WHERE status = 'Proposed'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0) as u64;
        let confirmed_preferences = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM memories WHERE scope IN ('User', 'Workspace', 'Thread')",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0) as u64;
        let active_patterns = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM workflow_patterns WHERE status = 'active'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0) as u64;
        let suggested_patterns = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM workflow_patterns WHERE status = 'suggested'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0) as u64;

        let recent_candidates: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT value FROM memory_candidates
            WHERE status = 'Proposed'
            ORDER BY created_at DESC LIMIT 5
            ",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();
        let recent_memories: Vec<(String,)> = sqlx::query_as(
            r"
            SELECT COALESCE(key, value) FROM memories
            ORDER BY updated_at DESC LIMIT 5
            ",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();

        let learning_queue = self.get_learning_queue_status().await.unwrap_or(LearningQueueStatus {
            queued: 0,
            running: 0,
            failed: 0,
            completed_recent: 0,
        });

        Ok(LearningOverview {
            pending_candidates,
            confirmed_preferences,
            active_patterns,
            suggested_patterns,
            recent_candidate_summaries: recent_candidates
                .into_iter()
                .map(|(v,)| v.chars().take(120).collect())
                .collect(),
            recent_memory_keys: recent_memories.into_iter().map(|(v,)| v).collect(),
            learning_queue,
            sensitive_encryption_enabled,
            local_storage: true,
        })
    }

    /// Load privacy / learning product settings.
    pub async fn get_privacy_settings(&self) -> Result<PrivacySettings, AppError> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT value_json FROM app_settings WHERE key = 'privacy_settings'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("get_privacy_settings failed: {e}"),
        })?;
        if let Some((json,)) = row {
            serde_json::from_str(&json).map_err(|e| AppError::Internal {
                message: format!("parse privacy_settings failed: {e}"),
            })
        } else {
            Ok(PrivacySettings::default())
        }
    }

    /// Persist privacy / learning product settings.
    pub async fn update_privacy_settings(
        &self,
        settings: PrivacySettings,
    ) -> Result<PrivacySettings, AppError> {
        let json = serde_json::to_string(&settings).map_err(|e| AppError::Internal {
            message: format!("serialize privacy_settings failed: {e}"),
        })?;
        sqlx::query(
            r"
            INSERT INTO app_settings (key, value_json, updated_at)
            VALUES ('privacy_settings', ?, ?)
            ON CONFLICT(key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at = excluded.updated_at
            ",
        )
        .bind(json)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("update_privacy_settings failed: {e}"),
        })?;
        Ok(settings)
    }

    /// Clear learning artifacts without touching workspace source files.
    pub async fn clear_learning_data(
        &self,
        clear_candidates: bool,
        clear_memories: bool,
        clear_patterns: bool,
        clear_rag: bool,
    ) -> Result<(), AppError> {
        if clear_candidates {
            let _ = sqlx::query("DELETE FROM memory_candidates").execute(&self.pool).await;
        }
        if clear_memories {
            let _ = sqlx::query("DELETE FROM memories").execute(&self.pool).await;
        }
        if clear_patterns {
            let _ = sqlx::query("DELETE FROM workflow_patterns").execute(&self.pool).await;
        }
        if clear_rag {
            let _ = sqlx::query("DELETE FROM rag_chunks").execute(&self.pool).await;
            let _ = sqlx::query("DELETE FROM rag_documents").execute(&self.pool).await;
        }
        Ok(())
    }

    /// Accept a candidate into long-term memory.
    pub async fn accept_memory_candidate(
        &self,
        id: app_models::MemoryCandidateId,
        edited_value: Option<String>,
        scope: Option<app_models::MemoryScope>,
        sensitive: Option<bool>,
        memory_manager: &dyn app_memory::MemoryManager,
    ) -> Result<(MemoryCandidate, app_models::MemoryItem), AppError> {
        let candidate = self
            .candidates
            .accept(id, edited_value, scope, sensitive)
            .await?;
        let memory = memory_manager
            .create_memory_with_meta(
                candidate.scope,
                candidate.workspace_id,
                candidate.thread_id,
                &candidate.key,
                &candidate.value,
                candidate.sensitive,
                Some(candidate.kind),
                Some(candidate.run_id),
                Some(candidate.confidence),
            )
            .await?;
        Ok((candidate, memory))
    }

    /// Background worker loop body: lease and process one LearnFromRun task.
    pub async fn process_next_job(&self, worker_id: &str) -> Result<bool, AppError> {
        let lease = chrono::Duration::seconds(60);
        let Some(task) = self.task_queue.lease_next(worker_id, lease).await? else {
            return Ok(false);
        };
        if task.task_kind != TaskKind::LearnFromRun {
            // Other task kinds are owned by other workers (automations, etc.).
            // Release the lease without treating this as a learning failure.
            let _ = self
                .task_queue
                .complete(task.id, "skipped: not a LearnFromRun task")
                .await;
            return Ok(true);
        }
        match self.process_learning_job(&task.payload).await {
            Ok(()) => {
                let _ = self
                    .task_queue
                    .complete(task.id, "learning processed")
                    .await;
            }
            Err(err) => {
                let _ = self.task_queue.fail(task.id, err.to_string()).await;
            }
        }
        Ok(true)
    }
}

#[derive(Default)]
struct ContextSnapshot {
    memory_ids: Vec<MemoryId>,
    pattern_ids: Vec<WorkflowPatternId>,
    behavior_policy: Option<BehaviorPolicy>,
    outbound_manifest: Option<app_models::OutboundContextManifest>,
    recall_scores: Vec<(MemoryId, f64)>,
}

#[derive(sqlx::FromRow)]
struct FeedbackRow {
    run_id: uuid::Uuid,
    rating: String,
    comment: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl FeedbackRow {
    fn into_feedback(self) -> Result<RunFeedback, AppError> {
        Ok(RunFeedback {
            run_id: AgentRunId(self.run_id),
            rating: RunFeedbackRating::try_from(self.rating.as_str())?,
            comment: self.comment,
            created_at: self.created_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct SnapshotRow {
    memory_ids_json: String,
    pattern_ids_json: String,
    behavior_policy_json: String,
    outbound_manifest_json: Option<String>,
    recall_scores_json: String,
}

/// Helper used when only a provisional signal is needed before full evaluation.
#[must_use]
pub fn provisional_from_status(status: AgentRunStatus) -> OutcomeSignal {
    provisional_outcome(status)
}

/// Type alias for workspace/thread context when enqueueing learning.
#[derive(Debug, Clone, Copy)]
pub struct LearningRunRef {
    pub run_id: AgentRunId,
    pub workspace_id: WorkspaceId,
    pub thread_id: ThreadId,
}
