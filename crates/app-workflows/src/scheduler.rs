//! Automation scheduler that turns automations into background tasks.

use app_models::{AppError, Automation, AutomationId, AutomationTrigger, WorkspaceId};
use app_runtime::{BackgroundTaskQueue, PorticoRuntimeHandle, Storage, TaskKind};
use chrono::{DateTime, Local, Utc};
use cron::Schedule;
use std::str::FromStr;
use std::sync::Arc;

/// Schedules automations by enqueueing background tasks when they are due.
#[derive(Clone)]
pub struct AutomationScheduler {
    queue: BackgroundTaskQueue,
    storage: Arc<dyn Storage>,
}

impl AutomationScheduler {
    /// Create a new scheduler using the given task queue and storage.
    #[must_use]
    pub fn new(queue: BackgroundTaskQueue, storage: Arc<dyn Storage>) -> Self {
        Self { queue, storage }
    }

    /// Create an automation.
    ///
    /// # Errors
    ///
    /// Returns an error if the cron expression is invalid or the automation
    /// cannot be persisted.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_automation(
        &self,
        workspace_id: WorkspaceId,
        name: impl Into<String>,
        description: impl Into<String>,
        trigger: AutomationTrigger,
        cron_expr: Option<String>,
        enabled: bool,
        permission_policy: serde_json::Value,
    ) -> Result<Automation, AppError> {
        let now = Utc::now();
        let next_run_at = match trigger {
            AutomationTrigger::Scheduled | AutomationTrigger::ThreadWakeup => {
                let cron = cron_expr.as_deref().ok_or_else(|| AppError::Internal {
                    message: "cron expression is required for scheduled automations".to_owned(),
                })?;
                Self::compute_next_run(cron, now)?
            }
            _ => None,
        };

        let automation = Automation {
            id: AutomationId::new(),
            workspace_id,
            name: name.into(),
            description: description.into(),
            trigger,
            cron_expr,
            enabled,
            permission_policy,
            next_run_at,
            last_run_at: None,
            created_at: now,
            updated_at: now,
        };

        self.storage.create_automation(&automation).await?;
        Ok(automation)
    }

    /// List automations in a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if automations cannot be read.
    pub async fn list_automations(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<Automation>, AppError> {
        self.storage.list_automations(workspace_id).await
    }

    /// Fetch an automation by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the automation is missing.
    pub async fn get_automation(&self, id: AutomationId) -> Result<Automation, AppError> {
        self.storage.get_automation(id).await
    }

    /// Update an automation.
    ///
    /// # Errors
    ///
    /// Returns an error if the automation cannot be persisted.
    pub async fn update_automation(&self, automation: Automation) -> Result<(), AppError> {
        self.storage.update_automation(&automation).await
    }

    /// Delete an automation.
    ///
    /// # Errors
    ///
    /// Returns an error if the automation is missing.
    pub async fn delete_automation(&self, id: AutomationId) -> Result<(), AppError> {
        self.storage.delete_automation(id).await
    }

    /// Execute an automation by creating a thread, starting a run, and
    /// submitting the automation description as a message.
    ///
    /// # Errors
    ///
    /// Returns an error if the automation is missing, disabled, or the runtime
    /// cannot start the run.
    pub async fn execute_automation(
        &self,
        id: AutomationId,
        runtime: &PorticoRuntimeHandle,
    ) -> Result<app_models::AgentRunId, AppError> {
        let automation = self.storage.get_automation(id).await?;
        if !automation.enabled {
            return Err(AppError::Internal {
                message: format!("automation {} is disabled", automation.name),
            });
        }

        let thread = runtime
            .create_thread(
                automation.workspace_id,
                &format!("Automation: {}", automation.name),
            )
            .await?;
        let run = runtime.start_run(automation.workspace_id, thread.id).await?;

        let message = format!("[{}] {}", automation.name, automation.description);
        let runtime_clone = runtime.clone();
        let run_id = run.id;
        tokio::spawn(async move {
            let _ = runtime_clone.submit_message(run_id, &message).await;
        });

        Ok(run_id)
    }

    /// Run an automation immediately.
    ///
    /// # Errors
    ///
    /// Returns an error if the automation is missing or cannot be started.
    pub async fn run_now(
        &self,
        id: AutomationId,
        runtime: &PorticoRuntimeHandle,
    ) -> Result<(), AppError> {
        self.execute_automation(id, runtime).await?;
        Ok(())
    }

    /// Trigger any automations that are due at or before `now`.
    ///
    /// # Errors
    ///
    /// Returns an error if reading, enqueuing, or updating fails.
    pub async fn tick(&self, now: DateTime<Utc>) -> Result<usize, AppError> {
        let due = self.storage.list_due_automations(now).await?;
        let mut scheduled = 0;

        for automation in &due {
            if !matches!(
                automation.trigger,
                AutomationTrigger::Scheduled | AutomationTrigger::ThreadWakeup
            ) {
                continue;
            }

            self.queue
                .enqueue(
                    automation.workspace_id,
                    None,
                    None,
                    TaskKind::ScheduledJob,
                    serde_json::json!({
                        "automation_id": automation.id.0,
                        "trigger": automation.trigger.as_str(),
                    }),
                    0,
                    Some(now),
                    None,
                )
                .await?;

            let mut updated = automation.clone();
            updated.last_run_at = Some(now);
            updated.next_run_at = automation
                .cron_expr
                .as_deref()
                .and_then(|cron| Self::compute_next_run(cron, now).ok())
                .flatten();
            updated.updated_at = now;
            self.storage.update_automation(&updated).await?;

            scheduled += 1;
        }

        Ok(scheduled)
    }

    /// Compute the next run time for a cron expression after the given time.
    ///
    /// # Errors
    ///
    /// Returns an error if the cron expression cannot be parsed.
    pub fn compute_next_run(
        cron_expr: &str,
        after: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, AppError> {
        let schedule = Schedule::from_str(cron_expr).map_err(|e| AppError::Internal {
            message: format!("invalid cron expression: {e}"),
        })?;
        let after_local = after.with_timezone(&Local);
        let next_local = schedule.after(&after_local).next();
        Ok(next_local.map(|dt| dt.with_timezone(&Utc)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_runtime::{BackgroundTaskQueue, SqliteStorage};

    async fn setup() -> (AutomationScheduler, WorkspaceId) {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let workspace = storage
            .create_workspace("test", "/tmp/test", false)
            .await
            .expect("create workspace");
        let queue = BackgroundTaskQueue::new(storage.clone());
        (AutomationScheduler::new(queue, storage), workspace.id)
    }

    #[tokio::test]
    async fn create_scheduled_automation_computes_next_run() {
        let (scheduler, workspace_id) = setup().await;

        let automation = scheduler
            .create_automation(
                workspace_id,
                "daily",
                "runs daily",
                AutomationTrigger::Scheduled,
                Some("0 0 * * * *".to_owned()),
                true,
                serde_json::Value::Null,
            )
            .await
            .expect("create automation");

        assert!(automation.next_run_at.is_some());
        assert!(automation.next_run_at.unwrap() > Utc::now());
    }

    #[tokio::test]
    async fn tick_enqueues_due_automation() {
        let (scheduler, workspace_id) = setup().await;

        let automation = scheduler
            .create_automation(
                workspace_id,
                "every second",
                "runs every second",
                AutomationTrigger::Scheduled,
                Some("* * * * * *".to_owned()),
                true,
                serde_json::Value::Null,
            )
            .await
            .expect("create automation");

        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let scheduled_count = scheduler.tick(Utc::now()).await.expect("tick");
        assert_eq!(scheduled_count, 1);

        let tasks = scheduler
            .storage
            .list_background_tasks(Some(workspace_id), None, 10)
            .await
            .expect("list tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_kind, TaskKind::ScheduledJob);

        let updated = scheduler.get_automation(automation.id).await.expect("get automation");
        assert!(updated.last_run_at.is_some());
        assert!(updated.next_run_at.is_some());
    }

    #[tokio::test]
    async fn manual_trigger_does_not_schedule() {
        let (scheduler, workspace_id) = setup().await;

        let automation = scheduler
            .create_automation(
                workspace_id,
                "manual",
                "manual routine",
                AutomationTrigger::ManualRoutine,
                None,
                true,
                serde_json::Value::Null,
            )
            .await
            .expect("create automation");

        assert!(automation.next_run_at.is_none());

        let scheduled_count = scheduler.tick(Utc::now()).await.expect("tick");
        assert_eq!(scheduled_count, 0);
    }

    #[tokio::test]
    async fn compute_next_run_parses_cron() {
        let now = Utc::now();
        let next = AutomationScheduler::compute_next_run("0 0 * * * *", now)
            .expect("compute next run")
            .expect("some next run");
        assert!(next > now);
        assert_eq!(next.timestamp() % 60, 0);
    }
}
