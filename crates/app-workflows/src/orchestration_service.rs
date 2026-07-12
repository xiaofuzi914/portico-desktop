//! End-to-end multi-agent orchestration closed loop.
//!
//! Depends on [`PatternSource`]/[`PatternSink`] ports only — never on a concrete
//! memory crate — so memory optimizations cannot break orchestration compile-time
//! coupling, and orchestration can run with no-op memory.

use crate::memory_plan::{
    build_memory_conditioned_plan, needs_execution_followup, result_oriented_mandate,
    wants_deliverable,
};
use crate::ports::{
    NoopPatternSink, NoopPatternSource, PatternRecallQuery, PatternSink, PatternSource,
};
use crate::{AgentRegistry, Orchestrator};
use app_models::{
    AgentRunId, AgentRunStatus, AppError, BuiltInAgent, Orchestration, OrchestrationId,
    OrchestrationOutcome, OrchestrationPlan, OrchestrationStatus, PatternHint, SubagentRun,
    ThreadId, WorkspaceId,
};
use app_runtime::PorticoRuntimeHandle;
use chrono::Utc;
use std::sync::Arc;

struct ExecutionOutcome {
    success: bool,
    summary: Option<String>,
    status: OrchestrationStatus,
}

/// Memory-conditioned multi-agent facade used by the Tauri layer.
///
/// Orchestration sessions are durable in `SQLite` (`orchestrations` table).
pub struct OrchestrationService {
    runtime: Arc<PorticoRuntimeHandle>,
    registry: AgentRegistry,
    orchestrator: Orchestrator,
    patterns_in: Arc<dyn PatternSource>,
    patterns_out: Arc<dyn PatternSink>,
}

impl OrchestrationService {
    /// Create a service with no-op memory ports (safe default).
    #[must_use]
    pub fn new(runtime: Arc<PorticoRuntimeHandle>, registry: AgentRegistry) -> Self {
        let orchestrator = Orchestrator::new(runtime.clone(), registry.clone());
        Self {
            runtime,
            registry,
            orchestrator,
            patterns_in: Arc::new(NoopPatternSource),
            patterns_out: Arc::new(NoopPatternSink),
        }
    }

    /// Inject memory ports without creating a compile-time dependency on app-memory.
    #[must_use]
    pub fn with_pattern_ports(
        mut self,
        source: Arc<dyn PatternSource>,
        sink: Arc<dyn PatternSink>,
    ) -> Self {
        self.patterns_in = source;
        self.patterns_out = sink;
        self
    }

    /// List built-in agent definitions.
    #[must_use]
    pub fn list_agents(&self) -> Vec<app_models::AgentDefinition> {
        self.orchestrator.list_agents()
    }

    /// Recall patterns for a task (for UI preview).
    ///
    /// # Errors
    ///
    /// Returns an error if the pattern source fails.
    pub async fn recall_patterns(
        &self,
        task: &str,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Vec<PatternHint>, AppError> {
        self.patterns_in
            .recall(PatternRecallQuery {
                task: task.to_owned(),
                workspace_id,
                limit: 5,
            })
            .await
    }

    /// Preview a plan without executing (memory-conditioned).
    ///
    /// # Errors
    ///
    /// Returns an error if the parent run is missing or recall fails.
    pub async fn preview_plan(
        &self,
        parent_run_id: AgentRunId,
        task: &str,
    ) -> Result<OrchestrationPlan, AppError> {
        let parent = self.runtime.get_run(parent_run_id).await?;
        let hints = self.recall_patterns(task, Some(parent.workspace_id)).await.unwrap_or_default();
        Ok(build_memory_conditioned_plan(
            parent_run_id,
            task,
            &hints,
            &self.registry,
        ))
    }

    /// Start a full multi-agent closed loop for a thread task.
    ///
    /// # Errors
    ///
    /// Returns an error if planning or execution fails fatally.
    pub async fn start_orchestration(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
        task: &str,
    ) -> Result<Orchestration, AppError> {
        let task = task.trim();
        if task.is_empty() {
            return Err(AppError::PermissionDenied {
                reason: "orchestration task must not be empty".to_owned(),
            });
        }

        // Parent run anchors the orchestration for audit/events.
        let parent = self.runtime.start_run(workspace_id, thread_id).await?;
        let parent_run_id = parent.id;
        // Persist the user's task on the conversation timeline so failures still
        // show what was asked (and Retry can re-send the same text).
        let _ = self
            .runtime
            .storage()
            .create_run_message(
                thread_id,
                parent_run_id,
                app_models::MessageRole::User,
                task,
            )
            .await;
        let now = Utc::now();
        let mut session = Orchestration {
            id: OrchestrationId::new(),
            parent_run_id,
            workspace_id,
            thread_id,
            task: task.to_owned(),
            status: OrchestrationStatus::Planning,
            plan: OrchestrationPlan {
                parent_run_id,
                subagents: vec![],
                pattern_ids: vec![],
                planning_rationale: String::new(),
            },
            pattern_ids: vec![],
            result_summary: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        self.store_session(&session).await;

        let hints = self.recall_patterns(task, Some(workspace_id)).await.unwrap_or_default();
        let plan = build_memory_conditioned_plan(parent_run_id, task, &hints, &self.registry);
        session.plan = plan.clone();
        session.pattern_ids = plan.pattern_ids.clone();
        session.status = OrchestrationStatus::Running;
        session.updated_at = Utc::now();
        self.store_session(&session).await;

        // Persist subagent rows via legacy orchestrator path for durability.
        for sub in &plan.subagents {
            self.runtime.storage().create_subagent(sub).await?;
        }

        let outcome = self.execute_closed_loop(&mut session, plan, task).await;
        let final_plan = session.plan.clone();
        Ok(self
            .finalize_orchestration(session, &final_plan, task, outcome)
            .await)
    }

    /// Execute planned roles, then auto follow-up with a worker when the user
    /// asked for a deliverable but the cast was still plan-only (or no writer).
    async fn execute_closed_loop(
        &self,
        session: &mut Orchestration,
        plan: OrchestrationPlan,
        task: &str,
    ) -> ExecutionOutcome {
        let mut plan = plan;
        match self.orchestrator.execute_plan(plan.clone()).await {
            Ok(mut results) => {
                // Closed loop: if user wanted deliverables but no writer ran, run worker now.
                let role_names: Vec<String> =
                    results.iter().map(|r| r.agent_name.clone()).collect();
                if needs_execution_followup(task, &role_names)
                    && let Some(worker) = self
                        .spawn_followup_worker(session, &plan, task, &results)
                        .await
                {
                    match self.orchestrator.run_one_subagent(worker).await {
                        Ok(done) => {
                            plan.subagents.push(SubagentRun {
                                id: done.id,
                                parent_run_id: done.parent_run_id,
                                agent_name: done.agent_name.clone(),
                                status: done.status,
                                task_description: done.task_description.clone(),
                                output_summary: done.output_summary.clone(),
                                created_at: done.created_at,
                                completed_at: done.completed_at,
                            });
                            session.plan = plan.clone();
                            session.updated_at = Utc::now();
                            self.store_session(session).await;
                            results.push(done);
                        }
                        Err(err) => {
                            // Keep primary results; surface follow-up failure in summary.
                            results.push(SubagentRun {
                                id: AgentRunId::new(),
                                parent_run_id: plan.parent_run_id,
                                agent_name: "worker".to_owned(),
                                status: AgentRunStatus::Failed,
                                task_description: "follow-up execution".to_owned(),
                                output_summary: Some(format!("自动执行阶段失败: {err}")),
                                created_at: Utc::now(),
                                completed_at: Some(Utc::now()),
                            });
                        }
                    }
                }

                let summary = self
                    .orchestrator
                    .synthesize(&results)
                    .await
                    .unwrap_or_else(|_| "多角色协作已完成。".to_owned());
                let all_ok = results.iter().all(|r| {
                    matches!(
                        r.status,
                        AgentRunStatus::Completed | AgentRunStatus::WaitingApproval
                    )
                });
                let ok = all_ok || results.iter().any(|r| r.status == AgentRunStatus::Completed);
                let status = if ok {
                    OrchestrationStatus::Completed
                } else {
                    OrchestrationStatus::Failed
                };
                let loop_note = if wants_deliverable(task) {
                    "闭环：结果导向（交付物优先）"
                } else {
                    "闭环：结论优先"
                };
                let enriched = format!(
                    "{summary}\n\n---\n{loop_note}\n编排说明：{}\nPatterns: {}",
                    plan.planning_rationale,
                    if plan.pattern_ids.is_empty() {
                        "none".to_owned()
                    } else {
                        plan.pattern_ids
                            .iter()
                            .map(|id| id.0.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                );
                session.plan = plan;
                ExecutionOutcome {
                    success: ok,
                    summary: Some(enriched),
                    status,
                }
            }
            Err(err) => ExecutionOutcome {
                success: false,
                summary: Some(format!("多角色协作失败: {err}")),
                status: OrchestrationStatus::Failed,
            },
        }
    }

    /// Build and persist a worker subagent that continues from prior role outputs.
    async fn spawn_followup_worker(
        &self,
        session: &Orchestration,
        plan: &OrchestrationPlan,
        task: &str,
        prior: &[SubagentRun],
    ) -> Option<SubagentRun> {
        let prior_text: String = prior
            .iter()
            .filter_map(|r| {
                r.output_summary
                    .as_ref()
                    .map(|s| format!("### {}\n{}", r.agent_name, s.chars().take(3_000).collect::<String>()))
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let def = self.registry.built_in(BuiltInAgent::Worker);
        let worker = SubagentRun {
            id: AgentRunId::new(),
            parent_run_id: plan.parent_run_id,
            agent_name: def.name.clone(),
            status: AgentRunStatus::Queued,
            task_description: format!(
                "Task:\n{}\n\nMandate:\n{}\n\nPrior role outputs (use as plan/context, DO NOT stop at planning):\n{}\n\nRole ({}): {}\nFocus: {}\n\n\
Execute now: produce the concrete deliverable the user asked for (files, PlantUML, code). \
List paths or paste the final artifact.",
                task.trim(),
                result_oriented_mandate(task),
                if prior_text.is_empty() {
                    "（前置角色无长输出，请直接根据 Task 交付）".to_owned()
                } else {
                    prior_text
                },
                def.name,
                def.description,
                def.system_instructions
            ),
            output_summary: None,
            created_at: Utc::now(),
            completed_at: None,
        };

        // Ensure worktree exists for write agent (best-effort).
        let _ = self
            .runtime
            .worktree_manager()
            .list_worktrees(session.workspace_id)
            .await;
        if let Ok(wts) = self
            .runtime
            .worktree_manager()
            .list_worktrees(session.workspace_id)
            .await
            && wts.is_empty()
        {
            let _ = self
                .runtime
                .worktree_manager()
                .create_worktree(session.workspace_id, session.thread_id, "default")
                .await;
        }

        if self.runtime.storage().create_subagent(&worker).await.is_err() {
            return None;
        }
        Some(worker)
    }

    async fn finalize_orchestration(
        &self,
        mut session: Orchestration,
        plan: &OrchestrationPlan,
        task: &str,
        outcome: ExecutionOutcome,
    ) -> Orchestration {
        session.status = outcome.status;
        session.result_summary = outcome.summary.clone();
        session.updated_at = Utc::now();
        session.completed_at = Some(Utc::now());
        self.store_session(&session).await;

        // Parent run terminal state for UI consistency.
        let _ = self
            .runtime
            .storage()
            .update_run_status(
                session.parent_run_id,
                if outcome.success {
                    AgentRunStatus::Completed
                } else {
                    AgentRunStatus::Failed
                },
            )
            .await;

        // Best-effort learning — never fails the user-facing orchestration.
        let agent_names: Vec<String> =
            plan.subagents.iter().map(|s| s.agent_name.clone()).collect();
        let _ = self
            .patterns_out
            .observe(OrchestrationOutcome {
                workspace_id: session.workspace_id,
                task: task.to_owned(),
                success: outcome.success,
                agent_names,
                pattern_ids: plan.pattern_ids.clone(),
                result_summary: outcome.summary,
            })
            .await;

        // Surface summary as a system message on the parent run when possible.
        if let Some(text) = &session.result_summary {
            let _ = self
                .runtime
                .storage()
                .create_run_message(
                    session.thread_id,
                    session.parent_run_id,
                    app_models::MessageRole::Assistant,
                    text,
                )
                .await;
        }

        session
    }

    /// Fetch a session by id (durable).
    ///
    /// # Errors
    ///
    /// Returns not found when the session is unknown.
    pub async fn get_orchestration(&self, id: OrchestrationId) -> Result<Orchestration, AppError> {
        self.runtime.storage().get_orchestration(id).await
    }

    /// List sessions for a thread (most recent first, durable).
    pub async fn list_for_thread(&self, thread_id: ThreadId) -> Vec<Orchestration> {
        self.runtime
            .storage()
            .list_orchestrations_for_thread(thread_id)
            .await
            .unwrap_or_default()
    }

    /// Cancel a running parent / children best-effort.
    ///
    /// # Errors
    ///
    /// Returns not found when the session is unknown.
    pub async fn cancel(&self, id: OrchestrationId) -> Result<Orchestration, AppError> {
        let mut session = self.get_orchestration(id).await?;
        let _ = self.runtime.cancel_run(session.parent_run_id).await;
        for sub in &session.plan.subagents {
            let _ = self.orchestrator.cancel_subagent(sub.id).await;
        }
        session.status = OrchestrationStatus::Cancelled;
        session.updated_at = Utc::now();
        session.completed_at = Some(Utc::now());
        self.store_session(&session).await;
        Ok(session)
    }

    async fn store_session(&self, session: &Orchestration) {
        if let Err(err) = self.runtime.storage().upsert_orchestration(session).await {
            eprintln!(
                "failed to persist orchestration {}: {err}",
                session.id.0
            );
        }
    }
}
