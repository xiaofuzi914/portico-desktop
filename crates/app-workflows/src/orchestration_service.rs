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
use crate::stage_graph::{
    apply_stage_edit, assemble_reduce_upstream, clamp_loop_max_iterations,
    collect_upstream_payloads, expand_foreach_tasks, list_bundled_workflows, loop_should_stop,
    merge_foreach_outputs, plan_adaptive_stage_graph, plan_bundled_workflow, plan_from_stages,
    plan_has_stages, primary_upstream_control_payload, render_stage_prompt,
    topological_stage_order, validate_stage_dag, BundledWorkflowMeta,
};
use crate::{AgentRegistry, Orchestrator};
use app_models::{
    AgentRunId, AgentRunStatus, AppError, BuiltInAgent, Orchestration, OrchestrationId,
    OrchestrationOutcome, OrchestrationPlan, OrchestrationStage, OrchestrationStageKind,
    OrchestrationStageStatus, OrchestrationStatus, PatternHint, SubagentRun, ThreadId, WorkspaceId,
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
///
/// [`Clone`] is cheap (shared runtime / pattern ports) so stage graphs can run
/// on a background task while the start API returns immediately.
#[derive(Clone)]
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

    /// Bundled multi-stage workflows available to the product UI.
    #[must_use]
    pub fn list_bundled_workflows(&self) -> Vec<BundledWorkflowMeta> {
        list_bundled_workflows()
    }

    /// Ensure shipped catalog templates exist in storage (idempotent seed).
    pub async fn ensure_builtin_templates_seeded(&self) -> Result<(), AppError> {
        for meta in list_bundled_workflows() {
            if self
                .runtime
                .storage()
                .get_workflow_template_by_catalog_key(meta.id)
                .await?
                .is_some()
            {
                continue;
            }
            let plan = plan_bundled_workflow(meta.id, AgentRunId::new(), "{task}")
                .ok_or_else(|| AppError::Internal {
                    message: format!("missing bundled plan for {}", meta.id),
                })?;
            let now = Utc::now();
            let template = app_models::WorkflowTemplate {
                id: app_models::WorkflowTemplateId::new(),
                catalog_key: Some(meta.id.to_owned()),
                title: meta.title.to_owned(),
                summary: meta.summary.to_owned(),
                stages: plan.stages,
                builtin: true,
                workspace_id: None,
                created_at: now,
                updated_at: now,
            };
            self.runtime.storage().upsert_workflow_template(&template).await?;
        }
        Ok(())
    }

    /// List catalog + user templates (seeds builtins first).
    pub async fn list_workflow_templates(
        &self,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Vec<app_models::WorkflowTemplate>, AppError> {
        let _ = self.ensure_builtin_templates_seeded().await;
        self.runtime.storage().list_workflow_templates(workspace_id).await
    }

    /// Save an edited DAG template (validates stages).
    pub async fn save_workflow_template(
        &self,
        mut template: app_models::WorkflowTemplate,
    ) -> Result<app_models::WorkflowTemplate, AppError> {
        let cleaned = apply_stage_edit(template.stages).map_err(|e| AppError::PermissionDenied {
            reason: e,
        })?;
        validate_stage_dag(&cleaned).map_err(|e| AppError::PermissionDenied { reason: e })?;
        template.stages = cleaned;
        template.updated_at = Utc::now();
        if template.created_at.timestamp() == 0 {
            template.created_at = template.updated_at;
        }
        self.runtime.storage().upsert_workflow_template(&template).await?;
        Ok(template)
    }

    /// Load template by id.
    pub async fn get_workflow_template(
        &self,
        id: app_models::WorkflowTemplateId,
    ) -> Result<app_models::WorkflowTemplate, AppError> {
        self.runtime.storage().get_workflow_template(id).await
    }

    /// Delete user template (not builtin).
    pub async fn delete_workflow_template(
        &self,
        id: app_models::WorkflowTemplateId,
    ) -> Result<(), AppError> {
        self.runtime.storage().delete_workflow_template(id).await
    }

    /// Start a full multi-agent closed loop for a thread task (adaptive stage graph by default).
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
        self.start_orchestration_with_workflow(workspace_id, thread_id, task, None)
            .await
    }

    /// Start multi-agent work with an optional named bundled workflow id.
    ///
    /// - `workflow_id = Some("multi-lens-review")` → fixed plan→foreach→reduce graph
    /// - `workflow_id = None` → adaptive multi-stage graph (still secondary to default chat)
    ///
    /// # Errors
    ///
    /// Returns an error if the workflow id is unknown or execution fails fatally.
    pub async fn start_orchestration_with_workflow(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
        task: &str,
        workflow_id: Option<&str>,
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
                stages: vec![],
                workflow_id: None,
                workflow_title: None,
            },
            pattern_ids: vec![],
            result_summary: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        self.store_session(&session).await;

        let hints = self.recall_patterns(task, Some(workspace_id)).await.unwrap_or_default();
        let plan = if let Some(wf) = workflow_id.map(str::trim).filter(|s| !s.is_empty()) {
            // Catalog key (bundled) or UUID of user-edited template.
            if let Some(bundled) = plan_bundled_workflow(wf, parent_run_id, task) {
                bundled
            } else if let Ok(template_id) = wf.parse::<app_models::WorkflowTemplateId>() {
                let template = self
                    .runtime
                    .storage()
                    .get_workflow_template(template_id)
                    .await?;
                validate_stage_dag(&template.stages).map_err(|e| AppError::PermissionDenied {
                    reason: e,
                })?;
                plan_from_stages(
                    parent_run_id,
                    task,
                    template.stages,
                    template.catalog_key.or_else(|| Some(template.id.0.to_string())),
                    Some(template.title),
                )
            } else if let Some(template) = self
                .runtime
                .storage()
                .get_workflow_template_by_catalog_key(wf)
                .await?
            {
                validate_stage_dag(&template.stages).map_err(|e| AppError::PermissionDenied {
                    reason: e,
                })?;
                plan_from_stages(
                    parent_run_id,
                    task,
                    template.stages,
                    Some(wf.to_owned()),
                    Some(template.title),
                )
            } else {
                return Err(AppError::PermissionDenied {
                    reason: format!("unknown workflow template: {wf}"),
                });
            }
        } else {
            // Adaptive multi-stage graph (plan → foreach roles → reduce).
            let role_names: Vec<String> = {
                let legacy = build_memory_conditioned_plan(parent_run_id, task, &hints, &self.registry);
                legacy
                    .subagents
                    .iter()
                    .map(|s| s.agent_name.clone())
                    .collect()
            };
            plan_adaptive_stage_graph(parent_run_id, task, &hints, &role_names)
        };

        session.plan = plan.clone();
        session.pattern_ids = plan.pattern_ids.clone();
        session.status = OrchestrationStatus::Running;
        session.updated_at = Utc::now();
        self.store_session(&session).await;

        // Persist any pre-expanded subagents (legacy); stage path creates as it runs.
        for sub in &plan.subagents {
            let _ = self.runtime.storage().create_subagent(sub).await;
        }

        // Return immediately so the UI is not stuck on "sending / delivering"
        // for the whole multi-stage run. Progress is polled via durable session.
        let service = self.clone();
        let mut session_bg = session.clone();
        let plan_bg = plan;
        let task_bg = task.to_owned();
        tokio::spawn(async move {
            let outcome = if plan_has_stages(&plan_bg) {
                service
                    .execute_stage_graph(&mut session_bg, plan_bg, &task_bg)
                    .await
            } else {
                service
                    .execute_closed_loop(&mut session_bg, plan_bg, &task_bg)
                    .await
            };
            let final_plan = session_bg.plan.clone();
            let _ = service
                .finalize_orchestration(session_bg, &final_plan, &task_bg, outcome)
                .await;
        });

        Ok(session)
    }

    /// Execute a durable single/foreach/reduce stage graph with inter-stage payloads.
    async fn execute_stage_graph(
        &self,
        session: &mut Orchestration,
        mut plan: OrchestrationPlan,
        task: &str,
    ) -> ExecutionOutcome {
        let order = topological_stage_order(&plan.stages);
        let mut all_subagents: Vec<SubagentRun> = Vec::new();

        for stage_id in order {
            // Honor cancel() written to durable storage while this graph is running.
            if self.session_is_cancelled(session.id).await {
                for s in &mut plan.stages {
                    if matches!(
                        s.status,
                        OrchestrationStageStatus::Pending | OrchestrationStageStatus::Running
                    ) {
                        s.status = OrchestrationStageStatus::Skipped;
                    }
                }
                plan.subagents = all_subagents;
                session.plan = plan;
                session.status = OrchestrationStatus::Cancelled;
                session.updated_at = Utc::now();
                self.store_session(session).await;
                return ExecutionOutcome {
                    success: false,
                    summary: Some("Orchestration cancelled.".to_owned()),
                    status: OrchestrationStatus::Cancelled,
                };
            }

            let Some(stage_idx) = plan.stages.iter().position(|s| s.id == stage_id) else {
                continue;
            };
            plan.stages[stage_idx].status = OrchestrationStageStatus::Running;
            session.plan = plan.clone();
            session.updated_at = Utc::now();
            self.store_session(session).await;

            let stage = plan.stages[stage_idx].clone();
            // Foreach needs the dep stage's raw control JSON ({"items":[...]}), not a
            // stage-id-keyed map from collect_upstream_payloads.
            let foreach_control =
                primary_upstream_control_payload(&plan.stages, &stage.depends_on);
            let upstream_payload = collect_upstream_payloads(&plan.stages, &stage.depends_on);
            let upstream_text = assemble_reduce_upstream(&plan.stages, &stage.depends_on);

            let stage_result = match stage.kind {
                OrchestrationStageKind::Single => {
                    self.run_single_stage(session, &stage, task, &upstream_payload, &upstream_text)
                        .await
                }
                OrchestrationStageKind::Foreach => {
                    self.run_foreach_stage(session, &stage, task, &foreach_control, &upstream_text)
                        .await
                }
                OrchestrationStageKind::Reduce => {
                    self.run_reduce_stage(session, &stage, task, &upstream_text)
                        .await
                }
                OrchestrationStageKind::Loop => {
                    self.run_loop_stage(session, &mut plan, stage_idx, task)
                        .await
                }
            };

            match stage_result {
                Ok((updated_stage, mut subs)) => {
                    plan.stages[stage_idx] = updated_stage;
                    all_subagents.append(&mut subs);
                    session.plan = plan.clone();
                    session.updated_at = Utc::now();
                    self.store_session(session).await;
                    // Cancel may have landed during this stage; stop before next stage.
                    if self.session_is_cancelled(session.id).await {
                        for s in &mut plan.stages {
                            if matches!(
                                s.status,
                                OrchestrationStageStatus::Pending
                                    | OrchestrationStageStatus::Running
                            ) {
                                s.status = OrchestrationStageStatus::Skipped;
                            }
                        }
                        plan.subagents = all_subagents;
                        session.plan = plan;
                        session.status = OrchestrationStatus::Cancelled;
                        session.updated_at = Utc::now();
                        self.store_session(session).await;
                        return ExecutionOutcome {
                            success: false,
                            summary: Some("Orchestration cancelled.".to_owned()),
                            status: OrchestrationStatus::Cancelled,
                        };
                    }
                    if plan.stages[stage_idx].status == OrchestrationStageStatus::Failed {
                        plan.subagents = all_subagents;
                        session.plan = plan;
                        return ExecutionOutcome {
                            success: false,
                            summary: Some(format!(
                                "Stage `{}` failed: {}",
                                stage_id,
                                session
                                    .plan
                                    .stages
                                    .iter()
                                    .find(|s| s.id == stage_id)
                                    .and_then(|s| s.error_message.as_deref())
                                    .unwrap_or("unknown error")
                            )),
                            status: OrchestrationStatus::Failed,
                        };
                    }
                }
                Err(err) => {
                    plan.stages[stage_idx].status = OrchestrationStageStatus::Failed;
                    plan.stages[stage_idx].error_message = Some(err.to_string());
                    plan.subagents = all_subagents;
                    session.plan = plan;
                    session.updated_at = Utc::now();
                    self.store_session(session).await;
                    return ExecutionOutcome {
                        success: false,
                        summary: Some(format!("Stage graph failed at `{stage_id}`: {err}")),
                        status: OrchestrationStatus::Failed,
                    };
                }
            }
        }

        let summary = self
            .orchestrator
            .synthesize(&all_subagents)
            .await
            .unwrap_or_else(|_| "多阶段协作已完成。".to_owned());
        let ok = all_subagents
            .iter()
            .any(|r| r.status == AgentRunStatus::Completed)
            || plan
                .stages
                .iter()
                .any(|s| s.status == OrchestrationStageStatus::Completed);
        let wf = plan
            .workflow_title
            .clone()
            .or(plan.workflow_id.clone())
            .unwrap_or_else(|| "multi-stage".to_owned());
        let enriched = format!(
            "{summary}\n\n---\nWorkflow: {wf}\nStages: {}\n{}",
            plan.stages
                .iter()
                .map(|s| format!("{}={}", s.id, s.status.as_str()))
                .collect::<Vec<_>>()
                .join(", "),
            plan.planning_rationale
        );
        plan.subagents = all_subagents;
        session.plan = plan;
        session.updated_at = Utc::now();
        self.store_session(session).await;
        // Final cancel check before claiming Completed.
        if self.session_is_cancelled(session.id).await {
            session.status = OrchestrationStatus::Cancelled;
            return ExecutionOutcome {
                success: false,
                summary: Some("Orchestration cancelled.".to_owned()),
                status: OrchestrationStatus::Cancelled,
            };
        }
        ExecutionOutcome {
            success: ok,
            summary: Some(enriched),
            status: if ok {
                OrchestrationStatus::Completed
            } else {
                OrchestrationStatus::Failed
            },
        }
    }

    async fn run_single_stage(
        &self,
        session: &Orchestration,
        stage: &OrchestrationStage,
        task: &str,
        upstream_payload: &str,
        upstream_text: &str,
    ) -> Result<(OrchestrationStage, Vec<SubagentRun>), AppError> {
        let mut stage = stage.clone();
        let prompt = render_stage_prompt(&stage.prompt_template, task, upstream_text, "");
        // Prefer seeded control payload; agent output may refine it later.
        let mut sub = self.make_stage_subagent(session, &stage, &stage.title, &prompt)?;
        self.runtime.storage().create_subagent(&sub).await?;
        sub = self.orchestrator.run_one_subagent(sub).await?;
        let summary = sub.output_summary.clone();
        // Keep seeded payload if present so foreach can expand reliably; append agent text.
        let refined = if let Some(seed) = stage.output_payload.clone() {
            if summary.as_ref().is_some_and(|s| s.contains('{')) {
                summary.clone().unwrap_or(seed)
            } else {
                seed
            }
        } else {
            summary
                .clone()
                .unwrap_or_else(|| upstream_payload.to_owned())
        };
        stage.output_payload = Some(refined);
        stage.tasks = vec![app_models::OrchestrationStageTask {
            id: format!("{}-main", stage.id),
            item_index: None,
            label: stage.title.clone(),
            status: map_run_to_stage_status(sub.status),
            subagent_id: Some(sub.id),
            output_summary: summary,
            output_payload: stage.output_payload.clone(),
        }];
        stage.status = map_run_to_stage_status(sub.status);
        if stage.status == OrchestrationStageStatus::Failed {
            stage.error_message = sub.output_summary.clone();
        }
        Ok((stage, vec![sub]))
    }

    async fn run_foreach_stage(
        &self,
        session: &Orchestration,
        stage: &OrchestrationStage,
        task: &str,
        upstream_payload: &str,
        upstream_text: &str,
    ) -> Result<(OrchestrationStage, Vec<SubagentRun>), AppError> {
        let mut stage = stage.clone();
        let mut tasks = expand_foreach_tasks(&stage, upstream_payload);
        if tasks.is_empty() {
            stage.status = OrchestrationStageStatus::Failed;
            stage.error_message = Some("foreach produced zero items".to_owned());
            return Ok((stage, vec![]));
        }

        let mut subs = Vec::new();
        for task_row in &mut tasks {
            let item_json = task_row
                .output_payload
                .clone()
                .unwrap_or_else(|| format!("{{\"label\":\"{}\"}}", task_row.label));
            // Allow per-item agent override from item JSON.
            let agent_override = serde_json::from_str::<serde_json::Value>(&item_json)
                .ok()
                .and_then(|v| {
                    v.get("agent")
                        .and_then(|a| a.as_str())
                        .map(ToOwned::to_owned)
                });
            let mut stage_for_agent = stage.clone();
            if let Some(name) = agent_override {
                stage_for_agent.agent_name = name;
            }
            let prompt =
                render_stage_prompt(&stage.prompt_template, task, upstream_text, &item_json);
            let mut sub =
                self.make_stage_subagent(session, &stage_for_agent, &task_row.label, &prompt)?;
            let _ = self.runtime.storage().create_subagent(&sub).await;
            sub = self.orchestrator.run_one_subagent(sub).await?;
            task_row.subagent_id = Some(sub.id);
            task_row.status = map_run_to_stage_status(sub.status);
            task_row.output_summary = sub.output_summary.clone();
            if task_row.output_payload.is_none() {
                task_row.output_payload = sub.output_summary.clone();
            }
            subs.push(sub);
        }
        stage.tasks = tasks;
        stage.output_payload = Some(merge_foreach_outputs(&stage.tasks));
        let any_ok = stage
            .tasks
            .iter()
            .any(|t| t.status == OrchestrationStageStatus::Completed);
        let all_failed = stage
            .tasks
            .iter()
            .all(|t| t.status == OrchestrationStageStatus::Failed);
        stage.status = if all_failed {
            OrchestrationStageStatus::Failed
        } else if any_ok {
            OrchestrationStageStatus::Completed
        } else {
            OrchestrationStageStatus::Failed
        };
        Ok((stage, subs))
    }

    /// Bounded loop: run body stages up to max_iterations or until stop flag is true.
    async fn run_loop_stage(
        &self,
        session: &Orchestration,
        plan: &mut OrchestrationPlan,
        loop_idx: usize,
        task: &str,
    ) -> Result<(OrchestrationStage, Vec<SubagentRun>), AppError> {
        let mut loop_stage = plan.stages[loop_idx].clone();
        let max = clamp_loop_max_iterations(loop_stage.max_iterations);
        let stop_path = loop_stage
            .stop_flag_path
            .clone()
            .unwrap_or_else(|| "pass".to_owned());
        let body_ids = loop_stage.body_stage_ids.clone();
        let mut all_subs = Vec::new();
        let mut last_control = String::from(r#"{"pass":false}"#);
        let mut rounds: Vec<serde_json::Value> = Vec::new();

        for iter in 1..=max {
            if self.session_is_cancelled(session.id).await {
                loop_stage.status = OrchestrationStageStatus::Cancelled;
                loop_stage.current_iteration = Some(iter);
                break;
            }
            loop_stage.current_iteration = Some(iter);
            loop_stage.status = OrchestrationStageStatus::Running;
            plan.stages[loop_idx] = loop_stage.clone();
            // Persist round progress for the board.
            let mut sess = session.clone();
            sess.plan = plan.clone();
            self.store_session(&sess).await;

            let mut round_summaries = Vec::new();
            for body_id in &body_ids {
                let Some(body_idx) = plan.stages.iter().position(|s| &s.id == body_id) else {
                    continue;
                };
                let body = plan.stages[body_idx].clone();
                let foreach_control =
                    primary_upstream_control_payload(&plan.stages, &body.depends_on);
                let upstream_payload = collect_upstream_payloads(&plan.stages, &body.depends_on);
                let upstream_text = assemble_reduce_upstream(&plan.stages, &body.depends_on);
                let result = match body.kind {
                    OrchestrationStageKind::Single => {
                        self.run_single_stage(
                            session,
                            &body,
                            task,
                            &upstream_payload,
                            &upstream_text,
                        )
                        .await?
                    }
                    OrchestrationStageKind::Foreach => {
                        self.run_foreach_stage(
                            session,
                            &body,
                            task,
                            &foreach_control,
                            &upstream_text,
                        )
                        .await?
                    }
                    OrchestrationStageKind::Reduce => {
                        self.run_reduce_stage(session, &body, task, &upstream_text)
                            .await?
                    }
                    OrchestrationStageKind::Loop => {
                        return Err(AppError::Internal {
                            message: "nested loop stages are not supported".to_owned(),
                        });
                    }
                };
                let (updated, mut subs) = result;
                if let Some(p) = &updated.output_payload {
                    last_control = p.clone();
                } else if let Some(t) = updated.tasks.last().and_then(|t| t.output_summary.clone()) {
                    last_control = t;
                }
                round_summaries.push(serde_json::json!({
                    "stage": body_id,
                    "status": updated.status.as_str(),
                    "summary": updated.tasks.last().and_then(|t| t.output_summary.clone()),
                }));
                plan.stages[body_idx] = updated;
                all_subs.append(&mut subs);
            }

            let stopped = loop_should_stop(&last_control, Some(stop_path.as_str()));
            rounds.push(serde_json::json!({
                "iteration": iter,
                "control": serde_json::from_str::<serde_json::Value>(&last_control).unwrap_or(serde_json::json!({"raw": last_control})),
                "body": round_summaries,
                "stopped": stopped,
            }));
            // One board task per round for progressive disclosure.
            loop_stage.tasks.push(app_models::OrchestrationStageTask {
                id: format!("{}-round-{}", loop_stage.id, iter),
                item_index: Some(iter),
                label: format!("round {iter}/{max}"),
                status: if stopped {
                    OrchestrationStageStatus::Completed
                } else if iter == max {
                    OrchestrationStageStatus::Completed
                } else {
                    OrchestrationStageStatus::Running
                },
                subagent_id: None,
                output_summary: Some(if stopped {
                    format!("stop flag `{stop_path}` true")
                } else {
                    format!("continue (pass=false)")
                }),
                output_payload: Some(last_control.clone()),
            });

            if stopped {
                loop_stage.status = OrchestrationStageStatus::Completed;
                break;
            }
            if iter == max {
                loop_stage.status = OrchestrationStageStatus::Completed;
                loop_stage.error_message =
                    Some(format!("loop reached max_iterations={max} without stop flag"));
            }
        }

        // Finalize round task statuses when loop ends.
        for t in &mut loop_stage.tasks {
            if t.status == OrchestrationStageStatus::Running {
                t.status = OrchestrationStageStatus::Completed;
            }
        }

        loop_stage.output_payload = Some(
            serde_json::to_string_pretty(&serde_json::json!({
                "pass": loop_should_stop(&last_control, Some(stop_path.as_str())),
                "iterations": loop_stage.current_iteration,
                "max_iterations": max,
                "rounds": rounds,
                "last_control": last_control,
            }))
            .unwrap_or_else(|_| last_control.clone()),
        );
        plan.stages[loop_idx] = loop_stage.clone();
        Ok((loop_stage, all_subs))
    }

    async fn run_reduce_stage(
        &self,
        session: &Orchestration,
        stage: &OrchestrationStage,
        task: &str,
        upstream_text: &str,
    ) -> Result<(OrchestrationStage, Vec<SubagentRun>), AppError> {
        let mut stage = stage.clone();
        let prompt = render_stage_prompt(&stage.prompt_template, task, upstream_text, "");
        let mut sub = self.make_stage_subagent(session, &stage, &stage.title, &prompt)?;
        self.runtime.storage().create_subagent(&sub).await?;
        sub = self.orchestrator.run_one_subagent(sub).await?;
        let summary = sub.output_summary.clone();
        stage.output_payload = summary.clone();
        stage.tasks = vec![app_models::OrchestrationStageTask {
            id: format!("{}-reduce", stage.id),
            item_index: None,
            label: stage.title.clone(),
            status: map_run_to_stage_status(sub.status),
            subagent_id: Some(sub.id),
            output_summary: summary,
            output_payload: stage.output_payload.clone(),
        }];
        stage.status = map_run_to_stage_status(sub.status);
        if stage.status == OrchestrationStageStatus::Failed {
            stage.error_message = sub.output_summary.clone();
        }
        Ok((stage, vec![sub]))
    }

    fn make_stage_subagent(
        &self,
        session: &Orchestration,
        stage: &OrchestrationStage,
        label: &str,
        prompt: &str,
    ) -> Result<SubagentRun, AppError> {
        let agent = resolve_built_in(&stage.agent_name);
        let def = self.registry.built_in(agent);
        Ok(SubagentRun {
            id: AgentRunId::new(),
            parent_run_id: session.parent_run_id,
            agent_name: def.name.clone(),
            status: AgentRunStatus::Queued,
            task_description: format!(
                "[{stage}] {label}\n\n{prompt}\n\nRole focus: {focus}",
                stage = stage.title,
                label = label,
                prompt = prompt,
                focus = def.system_instructions
            ),
            output_summary: None,
            created_at: Utc::now(),
            completed_at: None,
        })
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
                // Persist terminal subagent statuses (Failed/Completed/…) so the
                // composer footer does not keep showing "agent(Queued)" after a run.
                let rationale = plan.planning_rationale.clone();
                let pattern_ids = plan.pattern_ids.clone();
                let parent_run_id = plan.parent_run_id;
                plan.subagents = results;
                plan.planning_rationale = rationale;
                plan.pattern_ids = pattern_ids;
                plan.parent_run_id = parent_run_id;
                session.plan = plan;
                session.updated_at = Utc::now();
                self.store_session(session).await;
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

    async fn session_is_cancelled(&self, id: OrchestrationId) -> bool {
        matches!(
            self.get_orchestration(id).await,
            Ok(s) if s.status == OrchestrationStatus::Cancelled
        )
    }

    async fn finalize_orchestration(
        &self,
        mut session: Orchestration,
        plan: &OrchestrationPlan,
        task: &str,
        outcome: ExecutionOutcome,
    ) -> Orchestration {
        // Re-read durable status first (cancel may have won during the last stage).
        let durable_status = self
            .get_orchestration(session.id)
            .await
            .ok()
            .map(|s| s.status);
        let merged_memory = crate::stage_graph::merge_status_for_store(durable_status, session.status);
        // Never clobber Cancelled with a late Completed/Failed outcome.
        let final_status =
            crate::stage_graph::coalesce_orchestration_status(merged_memory, outcome.status);
        session.status = final_status;
        session.result_summary = outcome.summary.clone();
        session.updated_at = Utc::now();
        session.completed_at = Some(Utc::now());
        self.store_session(&session).await;

        // Parent run terminal state for UI consistency.
        let parent_status = match final_status {
            OrchestrationStatus::Completed => AgentRunStatus::Completed,
            OrchestrationStatus::Cancelled => AgentRunStatus::Cancelled,
            _ => AgentRunStatus::Failed,
        };
        let _ = self
            .runtime
            .storage()
            .update_run_status(session.parent_run_id, parent_status)
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

}

fn map_run_to_stage_status(status: AgentRunStatus) -> OrchestrationStageStatus {
    match status {
        AgentRunStatus::Completed | AgentRunStatus::WaitingApproval => {
            OrchestrationStageStatus::Completed
        }
        AgentRunStatus::Failed | AgentRunStatus::Interrupted => OrchestrationStageStatus::Failed,
        AgentRunStatus::Cancelled => OrchestrationStageStatus::Cancelled,
        AgentRunStatus::Queued | AgentRunStatus::Running | AgentRunStatus::Paused => {
            OrchestrationStageStatus::Running
        }
    }
}

fn resolve_built_in(name: &str) -> BuiltInAgent {
    match name.to_ascii_lowercase().replace('_', "-").as_str() {
        "explorer" => BuiltInAgent::Explorer,
        "planner" => BuiltInAgent::Planner,
        "worker" => BuiltInAgent::Worker,
        "reviewer" => BuiltInAgent::Reviewer,
        "security-reviewer" | "security" => BuiltInAgent::SecurityReviewer,
        "tester" => BuiltInAgent::Tester,
        "researcher" => BuiltInAgent::Researcher,
        "doc-writer" | "docwriter" | "writer" => BuiltInAgent::DocWriter,
        _ => BuiltInAgent::Default,
    }
}

// keep impl block closed properly — store_session continues below
impl OrchestrationService {
    /// Persist session without reviving Running/Completed over a durable Cancelled.
    async fn store_session(&self, session: &Orchestration) {
        let mut to_store = session.clone();
        let durable_status = self
            .runtime
            .storage()
            .get_orchestration(session.id)
            .await
            .ok()
            .map(|s| s.status);
        to_store.status =
            crate::stage_graph::merge_status_for_store(durable_status, session.status);
        if to_store.status == OrchestrationStatus::Cancelled {
            to_store.completed_at = to_store.completed_at.or_else(|| Some(Utc::now()));
        }
        // Keep caller's in-memory view aligned when cancel won.
        if to_store.status == OrchestrationStatus::Cancelled
            && session.status != OrchestrationStatus::Cancelled
        {
            // Best-effort: caller still holds &Orchestration; graph loop re-reads via
            // session_is_cancelled on the next iteration.
        }
        if let Err(err) = self.runtime.storage().upsert_orchestration(&to_store).await {
            eprintln!(
                "failed to persist orchestration {}: {err}",
                session.id.0
            );
        }
    }
}
