//! Pure progress projection for multi-agent orchestration sessions.

use app_models::{
    Orchestration, OrchestrationProgress, OrchestrationStageProgress, OrchestrationStageStatus,
    OrchestrationStatus,
};

/// Build a UI-facing progress snapshot from a durable orchestration session.
#[must_use]
pub fn build_orchestration_progress(session: &Orchestration) -> OrchestrationProgress {
    let stages = &session.plan.stages;
    let mut stage_views = Vec::with_capacity(stages.len());
    let mut tasks_done = 0u32;
    let mut tasks_total = 0u32;
    let mut can_retry = Vec::new();
    let mut current_stage_id = None;

    for stage in stages {
        let task_total = u32::try_from(stage.tasks.len()).unwrap_or(0).max(1);
        let task_completed = u32::try_from(
            stage
                .tasks
                .iter()
                .filter(|t| t.status == OrchestrationStageStatus::Completed)
                .count(),
        )
        .unwrap_or(0);
        tasks_total = tasks_total.saturating_add(task_total);
        tasks_done = tasks_done.saturating_add(task_completed);

        let attempt = stage
            .tasks
            .iter()
            .map(|t| t.attempt.max(1))
            .max()
            .unwrap_or(1);
        let retryable = matches!(
            stage.status,
            OrchestrationStageStatus::Failed | OrchestrationStageStatus::Cancelled
        ) && stage.kind != app_models::OrchestrationStageKind::Loop
            && attempt < 3;
        if retryable {
            can_retry.push(stage.id.clone());
        }
        if matches!(
            stage.status,
            OrchestrationStageStatus::Running | OrchestrationStageStatus::Pending
        ) && current_stage_id.is_none()
        {
            current_stage_id = Some(stage.id.clone());
        }

        let spec = stage.execution_spec.as_ref();
        stage_views.push(OrchestrationStageProgress {
            id: stage.id.clone(),
            title: stage.title.clone(),
            agent_name: stage.agent_name.clone(),
            status: stage.status.as_str().to_owned(),
            model_tier: spec.map(|s| s.model_tier.as_str().to_owned()),
            thinking_mode: spec.map(|s| s.thinking_mode.as_str().to_owned()),
            attempt,
            error_code: stage
                .tasks
                .iter()
                .find_map(|t| t.last_error_code.clone())
                .or_else(|| {
                    stage
                        .error_message
                        .as_ref()
                        .map(|_| "STAGE_FAILED".to_owned())
                }),
            error_message: stage.error_message.clone(),
            tasks_completed: task_completed,
            tasks_total: task_total,
            can_retry: retryable,
            allowed_tools: spec.map(|s| s.allowed_tools.clone()).unwrap_or_default(),
        });
    }

    // Legacy plans without stages: estimate from subagents.
    if stages.is_empty() {
        let subs = &session.plan.subagents;
        tasks_total = u32::try_from(subs.len()).unwrap_or(0).max(1);
        tasks_done = u32::try_from(
            subs.iter()
                .filter(|s| {
                    matches!(
                        s.status,
                        app_models::AgentRunStatus::Completed
                            | app_models::AgentRunStatus::WaitingApproval
                    )
                })
                .count(),
        )
        .unwrap_or(0);
    }

    let percent = if tasks_total == 0 {
        match session.status {
            OrchestrationStatus::Completed | OrchestrationStatus::PartialCompleted => 100,
            OrchestrationStatus::Planning => 5,
            _ => 0,
        }
    } else {
        ((tasks_done.saturating_mul(100)) / tasks_total).min(100)
    };

    OrchestrationProgress {
        id: session.id,
        status: session.status,
        percent,
        current_stage_id,
        stages: stage_views,
        can_retry_stage_ids: can_retry,
        can_continue: session.status.is_continuable(),
        result_summary: session.result_summary.clone(),
        soft_timeout_warned: session
            .result_summary
            .as_deref()
            .is_some_and(|s| s.contains("软超时")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::{
        AgentRunId, OrchestrationId, OrchestrationPlan, OrchestrationStage, OrchestrationStageKind,
        OrchestrationStageTask, ThreadId, WorkspaceId,
    };
    use chrono::Utc;

    fn sample_session(status: OrchestrationStatus) -> Orchestration {
        Orchestration {
            id: OrchestrationId::new(),
            parent_run_id: AgentRunId::new(),
            workspace_id: WorkspaceId::new(),
            thread_id: ThreadId::new(),
            task: "t".into(),
            status,
            plan: OrchestrationPlan {
                parent_run_id: AgentRunId::new(),
                subagents: vec![],
                pattern_ids: vec![],
                planning_rationale: String::new(),
                stages: vec![OrchestrationStage {
                    id: "s1".into(),
                    kind: OrchestrationStageKind::Single,
                    title: "Explore".into(),
                    agent_name: "explorer".into(),
                    status: OrchestrationStageStatus::Completed,
                    prompt_template: String::new(),
                    depends_on: vec![],
                    foreach_path: None,
                    body_stage_ids: vec![],
                    max_iterations: None,
                    stop_flag_path: None,
                    current_iteration: None,
                    tasks: vec![OrchestrationStageTask {
                        id: "t1".into(),
                        item_index: None,
                        label: "main".into(),
                        status: OrchestrationStageStatus::Completed,
                        subagent_id: None,
                        output_summary: Some("ok".into()),
                        output_payload: None,
                        attempt: 1,
                        last_error_code: None,
                    }],
                    output_payload: None,
                    error_message: None,
                    execution_spec: Some(crate::spec_for_role_name("explorer")),
                }],
                workflow_id: None,
                workflow_title: None,
            },
            pattern_ids: vec![],
            result_summary: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        }
    }

    #[test]
    fn progress_reports_percent_and_tools() {
        let session = sample_session(OrchestrationStatus::Completed);
        let progress = build_orchestration_progress(&session);
        assert_eq!(progress.percent, 100);
        assert_eq!(progress.stages.len(), 1);
        assert!(progress.stages[0]
            .allowed_tools
            .iter()
            .any(|t| t == "fs_read"));
        assert!(!progress.can_continue);
    }

    #[test]
    fn partial_is_continuable() {
        let mut session = sample_session(OrchestrationStatus::PartialCompleted);
        session.plan.stages[0].status = OrchestrationStageStatus::Failed;
        session.plan.stages[0].tasks[0].status = OrchestrationStageStatus::Failed;
        session.plan.stages[0].tasks[0].attempt = 1;
        let progress = build_orchestration_progress(&session);
        assert!(progress.can_continue);
        assert_eq!(progress.can_retry_stage_ids, vec!["s1".to_owned()]);
    }
}
