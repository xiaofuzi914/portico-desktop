//! Tauri 2 backend for Portico.
//!
//! Commands expose product-level APIs only; `AutoAgents` internals never cross
//! this boundary.

use std::sync::Arc;

use app_runtime::{
    AgentRunStatus, MemoryEventBus, NotificationCenter, PorticoRuntimeHandle,
    SqliteModelProviderRegistry, SqliteStorage, Storage, StorageRepoRootProvider,
};
use app_tools::{filesystem::FilesystemTool, git::GitTool, terminal::TerminalManager};
use app_workflows::{AgentRegistry, AutomationScheduler, Orchestrator};
use autoagents_adapter::{
    FilesystemToolAdapter, GitToolAdapter, McpToolAdapter, PorticoToolRegistry,
    TerminalToolAdapter, build_default_executor,
};
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::commands::browser::BrowserWindowRegistry;

pub mod commands;
pub mod error;

/// Shared application state managed by Tauri.
pub struct AppState {
    /// Product-level runtime handle.
    pub runtime: Arc<PorticoRuntimeHandle>,
    /// Multi-agent orchestrator.
    pub orchestrator: Arc<Orchestrator>,
    /// Notification center for creating and dismissing notifications.
    pub notification_center: Arc<NotificationCenter>,
    /// Automation scheduler.
    pub scheduler: Arc<AutomationScheduler>,
    /// In-app browser window registry.
    pub browser_registry: BrowserWindowRegistry,
    /// Secure keychain-backed store for provider API keys.
    pub secret_store: Arc<dyn app_security::SecretStore>,
    /// Product-level tool registry exposed to agent runs.
    pub tool_registry: Arc<PorticoToolRegistry>,
    /// Security context used for permission and audit decisions.
    pub security: Arc<app_security::SecurityContext>,
}

/// Run the Tauri application.
///
/// # Panics
///
/// Panics if the Tauri application fails to start or runtime initialization fails.
#[allow(clippy::too_many_lines)]
pub fn run() {
    human_panic::setup_panic!(
        human_panic::Metadata::new("Portico", env!("CARGO_PKG_VERSION"))
            .homepage("https://portico.ai")
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_log::Builder::default().build())
        .setup(|app| {
            let handle = app.handle().clone();

            let app_data_dir =
                handle.path().app_data_dir().expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
            let db_path = app_data_dir.join("portico.sqlite");

            let secret_store: Arc<dyn app_security::SecretStore> =
                Arc::new(app_security::KeyringSecretStore);

            let runtime = tauri::async_runtime::block_on(async {
                let storage = Arc::new(SqliteStorage::open(&db_path).await?);
                let event_bus = Arc::new(MemoryEventBus::default());
                let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
                let security = Arc::new(app_runtime::SecurityContext::new(
                    Arc::new(app_security::PolicyPermissionEngine::default_rules()),
                    Arc::new(app_security::DefaultCommandPolicy::new()),
                    Arc::new(app_security::DefaultNetworkPolicy::new()),
                    Arc::new(app_runtime::SqliteAuditLogger::new(storage.clone())),
                ));

                let tools = PorticoToolRegistry::new();
                tools.register(Arc::new(GitToolAdapter::new(Arc::new(GitTool::new(
                    security.clone(),
                    Arc::new(StorageRepoRootProvider::new(storage.clone())),
                )))));
                tools.register(Arc::new(TerminalToolAdapter::new(Arc::new(
                    TerminalManager::new(security.clone()),
                ))));
                let fs_adapter = FilesystemToolAdapter::new(Arc::new(FilesystemTool::new(
                    security.clone(),
                    Arc::new(StorageRepoRootProvider::new(storage.clone())),
                )));
                tools.register(fs_adapter.read_tool());
                tools.register(fs_adapter.write_tool());
                tools.register(fs_adapter.edit_tool());
                tools.register(fs_adapter.list_tool());
                tools.register(fs_adapter.search_tool());
                let tools = Arc::new(tools);

                let executor = build_default_executor(
                    registry.clone(),
                    Arc::clone(&tools),
                    secret_store.clone(),
                )
                .await
                .unwrap_or_else(|err| {
                    eprintln!("failed to build AutoAgents executor, falling back to mock: {err}");
                    None
                })
                .map(|exec| Arc::new(exec) as Arc<dyn app_runtime::AgentExecutor>);

                let runtime = PorticoRuntimeHandle::new(storage, event_bus, registry, executor)
                    .await?
                    .with_security(security.clone());

                // Register MCP tools from configured servers so agents can call them.
                {
                    let mcp_manager = runtime.mcp_manager();
                    let guard = mcp_manager.lock().await;
                    let manager_arc = Arc::new(guard.clone());
                    match guard.list_tools().await {
                        Ok(mcp_tools) => {
                            for tool in mcp_tools {
                                tools.register(Arc::new(McpToolAdapter::new(
                                    tool,
                                    Arc::clone(&manager_arc),
                                    security.clone(),
                                )));
                            }
                        }
                        Err(err) => {
                            eprintln!("failed to list MCP tools at startup: {err}");
                        }
                    }
                }

                Ok::<_, app_models::AppError>((runtime, tools, security))
            })
            .expect("failed to initialize Portico runtime");

        let (runtime, tool_registry, security) = runtime;

            let scheduler = Arc::new(AutomationScheduler::new(
                runtime.task_queue(),
                runtime.storage().clone(),
            ));
            let orchestrator = Arc::new(Orchestrator::new(
                Arc::new(runtime.clone()),
                AgentRegistry::new(),
            ));
            let notification_center = Arc::new(NotificationCenter::new(runtime.storage().clone()));

            let notification_center_for_bridge = notification_center.clone();
            let runtime_for_bridge = Arc::new(runtime.clone());

            let scheduler_for_ticker = scheduler.clone();
            let scheduler_for_worker = scheduler.clone();
            let runtime_for_worker = Arc::new(runtime.clone());

            app.manage(AppState {
                runtime: Arc::new(runtime.clone()),
                orchestrator,
                notification_center,
                scheduler,
                browser_registry: commands::browser::new_registry(),
                secret_store,
                tool_registry,
                security,
            });

            // Automation scheduler ticker: check for due automations every minute.
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    if let Err(err) = scheduler_for_ticker.tick(chrono::Utc::now()).await {
                        eprintln!("automation scheduler tick failed: {err}");
                    }
                }
            });

            // Background task worker: lease and execute scheduled automation tasks.
            tauri::async_runtime::spawn(async move {
                let worker_id = format!("portico-worker-{}", uuid::Uuid::new_v4());
                let queue = runtime_for_worker.task_queue();
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    match queue
                        .lease_next(&worker_id, chrono::Duration::seconds(120))
                        .await
                    {
                        Ok(Some(task)) => {
                            let outcome: Result<String, app_models::AppError> = match task.task_kind {
                                app_models::TaskKind::ScheduledJob => {
                                    match task
                                        .payload
                                        .get("automation_id")
                                        .and_then(|v| v.as_str())
                                        .and_then(|s| uuid::Uuid::parse_str(s).ok())
                                    {
                                        Some(auto_uuid) => {
                                            scheduler_for_worker
                                                .execute_automation(
                                                    app_models::AutomationId(auto_uuid),
                                                    &runtime_for_worker,
                                                )
                                                .await
                                                .map(|run_id| {
                                                    format!("started run {}", run_id.0)
                                                })
                                        }
                                        None => Ok("no automation_id in payload".to_owned()),
                                    }
                                }
                                _ => Ok("task kind not handled by worker".to_owned()),
                            };

                            match outcome {
                                Ok(summary) => {
                                    if let Err(err) = queue.complete(task.id, summary).await {
                                        eprintln!("failed to complete background task: {err}");
                                    }
                                }
                                Err(err) => {
                                    if let Err(err) = queue.fail(task.id, err.to_string()).await {
                                        eprintln!("failed to fail background task: {err}");
                                    }
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(err) => eprintln!("background worker lease failed: {err}"),
                    }
                }
            });

            // Bridge all runtime events to the frontend over the `portico:event` channel.
            tauri::async_runtime::spawn(async move {
                let mut stream = match runtime.subscribe_all_events() {
                    Ok(stream) => stream,
                    Err(err) => {
                        eprintln!("failed to subscribe to runtime events: {err}");
                        return;
                    }
                };

                loop {
                    match stream.next().await {
                        Ok(Some(event)) => {
                            if let Err(err) = handle.emit("portico:event", &event) {
                                eprintln!("failed to emit portico:event: {err}");
                            }

                            if let app_runtime::RuntimeEvent::RunStatusChanged {
                                run_id,
                                thread_id,
                                status,
                                ..
                            } = event
                            {
                                let workspace_id = match runtime_for_bridge.get_run(run_id).await {
                                    Ok(run) => run.workspace_id,
                                    Err(err) => {
                                        eprintln!("failed to resolve run workspace for notification: {err}");
                                        continue;
                                    }
                                };

                                match status {
                                    AgentRunStatus::WaitingApproval => {
                                        if let Err(err) = notification_center_for_bridge
                                            .create(
                                                workspace_id,
                                                Some(thread_id),
                                                Some(run_id),
                                                "Approval required",
                                                "A task is waiting for your approval",
                                                app_runtime::NotificationCategory::ApprovalRequired,
                                            )
                                            .await
                                        {
                                            eprintln!("failed to create approval notification: {err}");
                                        }
                                        if let Err(err) = handle
                                            .notification()
                                            .builder()
                                            .title("Approval required")
                                            .body("A task is waiting for your approval")
                                            .show()
                                        {
                                            eprintln!("failed to show approval notification: {err}");
                                        }
                                    }
                                    AgentRunStatus::Completed => {
                                        if let Err(err) = notification_center_for_bridge
                                            .create(
                                                workspace_id,
                                                Some(thread_id),
                                                Some(run_id),
                                                "Task completed",
                                                "A task has finished",
                                                app_runtime::NotificationCategory::TaskCompleted,
                                            )
                                            .await
                                        {
                                            eprintln!("failed to create completed notification: {err}");
                                        }
                                        if let Err(err) = handle
                                            .notification()
                                            .builder()
                                            .title("Task completed")
                                            .body("A task has finished")
                                            .show()
                                        {
                                            eprintln!("failed to show completed notification: {err}");
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(err) => {
                            eprintln!("runtime event stream error: {err}");
                            break;
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::workspace::create_workspace,
            commands::workspace::list_workspaces,
            commands::workspace::get_workspace,
            commands::workspace::trust_workspace,
            commands::workspace::set_workspace_paths,
            commands::thread::create_thread,
            commands::thread::list_threads,
            commands::thread::get_thread,
            commands::run::start_run,
            commands::run::submit_message,
            commands::run::cancel_run,
            commands::run::pause_run,
            commands::run::resume_run,
            commands::run::list_run_events,
            commands::security::evaluate_permission,
            commands::security::approve_request,
            commands::security::deny_request,
            commands::security::list_audit_log,
            commands::model::list_providers,
            commands::model::create_provider,
            commands::model::update_provider,
            commands::model::delete_provider,
            commands::model::list_models,
            commands::model::create_model,
            commands::model::delete_model,
            commands::model::get_usage_summary,
            commands::model::check_budget,
            commands::secrets::set_provider_secret,
            commands::secrets::delete_provider_secret,
            commands::orchestrator::list_agents,
            commands::orchestrator::plan_subagents,
            commands::orchestrator::execute_subagents,
            commands::orchestrator::cancel_subagent,
            commands::notification::list_notifications,
            commands::notification::mark_notification_read,
            commands::notification::dismiss_notification,
            commands::notification::send_test_notification,
            commands::automation::list_automations,
            commands::automation::create_automation,
            commands::automation::update_automation,
            commands::automation::delete_automation,
            commands::automation::run_automation_now,
            commands::background_task::list_background_tasks,
            commands::background_task::cancel_background_task,
            commands::worktree::create_worktree,
            commands::worktree::delete_worktree,
            commands::worktree::list_worktrees,
            commands::git::git_status,
            commands::git::git_diff,
            commands::git::git_stage,
            commands::git::git_unstage,
            commands::git::git_commit,
            commands::git::git_branch,
            commands::git::git_push,
            commands::terminal::create_terminal,
            commands::terminal::execute_terminal_command,
            commands::terminal::read_terminal_history,
            commands::memory::list_memories,
            commands::memory::create_memory,
            commands::memory::update_memory,
            commands::memory::delete_memory,
            commands::memory::load_instructions,
            commands::memory::inspect_context,
            commands::memory::search_rag,
            commands::plugins::install_plugin,
            commands::plugins::list_plugins,
            commands::plugins::enable_plugin,
            commands::plugins::uninstall_plugin,
            commands::plugins::list_skills,
            commands::plugins::list_mcp_servers,
            commands::plugins::add_mcp_server,
            commands::plugins::remove_mcp_server,
            commands::plugins::list_mcp_tools,
            commands::plugins::invoke_mcp_tool,
            commands::plugins::refresh_mcp_tools,
            commands::browser::open_browser_window,
            commands::browser::close_browser_window,
            commands::browser::list_browser_windows,
            commands::browser::browser_use_action,
            commands::desktop::capture_screen,
            commands::desktop::move_mouse,
            commands::desktop::click_mouse,
            commands::desktop::type_text,
            commands::desktop::focus_app,
            commands::diagnostics::collect_diagnostics_bundle,
            commands::diagnostics::upload_diagnostics_bundle,
            commands::migration::list_migrations,
            commands::migration::rollback_last_migration,
            commands::artifact::preview_artifact,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
