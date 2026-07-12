//! Tauri 2 backend for Portico.
//!
//! Commands expose product-level APIs only; `AutoAgents` internals never cross
//! this boundary.

use std::sync::Arc;

use app_runtime::{
    AgentRunStatus, MemoryEventBus, ModelProviderRegistry, NotificationCenter,
    PorticoRuntimeHandle, SqliteModelProviderRegistry, SqliteStorage, Storage,
};
use app_workflows::{AgentRegistry, AutomationScheduler, OrchestrationService};
use autoagents_adapter::{PorticoToolRegistry, RegistryExecutorResolver};
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

#[cfg(any(test, feature = "desktop-e2e"))]
const E2E_DATA_DIR_MARKER: &str = ".portico-desktop-e2e";

#[cfg(any(test, feature = "desktop-e2e"))]
fn select_e2e_app_data_dir(
    default: &std::path::Path,
    override_path: Option<std::ffi::OsString>,
) -> Result<std::path::PathBuf, String> {
    let override_path = override_path
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PORTICO_E2E_APP_DATA_DIR is required for desktop-e2e".to_owned())?;
    let override_path = std::path::PathBuf::from(override_path);
    if !override_path.is_absolute() {
        return Err("PORTICO_E2E_APP_DATA_DIR must be an absolute path".to_owned());
    }
    let metadata = std::fs::symlink_metadata(&override_path)
        .map_err(|error| format!("failed to inspect E2E app data directory: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("PORTICO_E2E_APP_DATA_DIR must be a real directory, not a symlink".to_owned());
    }
    let marker = override_path.join(E2E_DATA_DIR_MARKER);
    let marker_value = std::fs::read_to_string(marker)
        .map_err(|_| "E2E app data directory is missing its safety marker".to_owned())?;
    if marker_value.trim() != "portico-desktop-e2e" {
        return Err("E2E app data directory has an invalid safety marker".to_owned());
    }

    let override_path = override_path
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize E2E app data directory: {error}"))?;
    if default.exists() && default.canonicalize().is_ok_and(|default| default == override_path) {
        return Err("desktop-e2e cannot use the production app data directory".to_owned());
    }
    Ok(override_path)
}

pub mod commands;
pub mod embedding_setup;
pub mod error;
mod pattern_ports;
mod plugin_github;
mod plugin_package;
pub mod portico_paths;

/// Shared application state managed by Tauri.
pub struct AppState {
    /// Application data directory used by durable storage and safe exports.
    pub app_data_dir: std::path::PathBuf,
    /// User plugin home layout (`~/.portico` or isolated e2e home).
    pub portico_user_dirs: portico_paths::PorticoUserDirs,
    /// Product-level runtime handle.
    pub runtime: Arc<PorticoRuntimeHandle>,
    /// Multi-agent orchestration service (memory-conditioned).
    pub orchestration: Arc<app_workflows::OrchestrationService>,
    /// Notification center for creating and dismissing notifications.
    pub notification_center: Arc<NotificationCenter>,
    /// Automation scheduler.
    pub scheduler: Arc<AutomationScheduler>,
    /// Secure keychain-backed store for provider API keys.
    pub secret_store: Arc<dyn app_security::SecretStore>,
    /// Product-level tool registry exposed to agent runs.
    pub tool_registry: Arc<PorticoToolRegistry>,
    /// Security context used for permission and audit decisions.
    pub security: Arc<app_security::SecurityContext>,
    /// Optional pattern store (None in e2e when memory ports are no-op only).
    pub pattern_store: Option<Arc<dyn app_memory::PatternStore>>,
}

impl AppState {
    /// Build application state with optional capabilities disabled by default.
    #[must_use]
    pub fn new(
        app_data_dir: std::path::PathBuf,
        portico_user_dirs: portico_paths::PorticoUserDirs,
        runtime: Arc<PorticoRuntimeHandle>,
        notification_center: Arc<NotificationCenter>,
        scheduler: Arc<AutomationScheduler>,
        secret_store: Arc<dyn app_security::SecretStore>,
        tool_registry: Arc<PorticoToolRegistry>,
        security: Arc<app_security::SecurityContext>,
    ) -> Self {
        let orchestration = Arc::new(OrchestrationService::new(
            runtime.clone(),
            AgentRegistry::new(),
        ));
        Self {
            app_data_dir,
            portico_user_dirs,
            runtime,
            orchestration,
            notification_center,
            scheduler,
            secret_store,
            tool_registry,
            security,
            pattern_store: None,
        }
    }

    /// Replace the default orchestration service with the configured service.
    #[must_use]
    pub fn with_orchestration(mut self, orchestration: Arc<OrchestrationService>) -> Self {
        self.orchestration = orchestration;
        self
    }

    /// Expose the pattern store to memory-management commands.
    #[must_use]
    pub fn with_pattern_store(mut self, pattern_store: Arc<dyn app_memory::PatternStore>) -> Self {
        self.pattern_store = Some(pattern_store);
        self
    }
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

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_log::Builder::default().level(log::LevelFilter::Info).build());

    #[cfg(feature = "desktop-e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .setup(|app| {
            let handle = app.handle().clone();

            let default_app_data_dir =
                handle.path().app_data_dir().expect("failed to resolve app data dir");
            #[cfg(feature = "desktop-e2e")]
            let app_data_dir = select_e2e_app_data_dir(
                &default_app_data_dir,
                std::env::var_os("PORTICO_E2E_APP_DATA_DIR"),
            )
            .expect("invalid E2E app data directory");
            #[cfg(not(feature = "desktop-e2e"))]
            let app_data_dir = default_app_data_dir;
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
            let portico_home = {
                #[cfg(feature = "desktop-e2e")]
                {
                    app_data_dir.join("portico-home")
                }
                #[cfg(not(feature = "desktop-e2e"))]
                {
                    portico_paths::default_portico_home()
                        .expect("failed to resolve ~/.portico home")
                }
            };
            let portico_user_dirs = portico_paths::ensure_portico_user_dirs_at(portico_home)
                .expect("failed to create ~/.portico plugins layout");
            // Best-effort: copy monorepo catalog packages into available/ so the
            // UI can one-click install without a folder picker.
            let _ = portico_paths::seed_all_catalog_packages(
                &portico_user_dirs.plugins_available,
            );
            let db_path = app_data_dir.join("portico.sqlite");

            #[cfg(feature = "desktop-e2e")]
            let secret_store: Arc<dyn app_security::SecretStore> =
                Arc::new(app_security::InMemorySecretStore::new());
            // Production: AES-GCM vault under app data + one-shot Keychain import
            // + process cache. Daily chat no longer hits macOS Keychain.
            #[cfg(not(feature = "desktop-e2e"))]
            let secret_store: Arc<dyn app_security::SecretStore> = Arc::new(
                app_security::open_production_secret_store(&app_data_dir)
                    .expect("failed to open encrypted secret vault"),
            );

            let runtime = tauri::async_runtime::block_on(async {
                let storage = Arc::new(SqliteStorage::open(&db_path).await?);
                let event_bus = Arc::new(MemoryEventBus::default());
                let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
                // One-shot: import any legacy Keychain secrets into the local vault
                // before chat/embedding paths need them. After promotion, Keychain
                // is no longer read on the hot path.
                if let Ok(providers) = registry.list_providers().await {
                    for provider in providers {
                        let _ = secret_store.get(&provider.api_key_reference);
                    }
                }
                let security = Arc::new(app_runtime::SecurityContext::new(
                    Arc::new(app_security::PolicyPermissionEngine::default_rules()),
                    Arc::new(app_security::DefaultCommandPolicy::new()),
                    Arc::new(app_security::DefaultNetworkPolicy::new()),
                    Arc::new(app_runtime::SqliteAuditLogger::new(storage.clone())),
                ));

                // Restore only the product-owned safe golden path. Definitions
                // cannot invoke legacy tools directly: every call is checkpointed,
                // gated, leased, and dispatched by the durable runtime.
                // Terminal, Git mutations, MCP, browser, and desktop stay disabled.
                let tools = Arc::new(PorticoToolRegistry::new());
                tools.register_safe_builtin_definitions();

                let executor_resolver = Arc::new(
                    RegistryExecutorResolver::new(
                        registry.clone(),
                        Arc::clone(&tools),
                        secret_store.clone(),
                    )
                    .with_security(security.clone()),
                );

                let embedding =
                    embedding_setup::build_embedding_provider(registry.clone(), &secret_store)
                        .await?;

                let runtime = PorticoRuntimeHandle::new(
                    storage,
                    event_bus,
                    registry,
                    None,
                    Some(embedding),
                )
                .await?
                .with_security(security.clone())
                .with_executor_resolver(executor_resolver);
                runtime.recover_tool_execution().await?;

                // Do not discover or spawn configured MCP servers during startup.
                // MCP will be restored only behind its explicit trust and sandbox flow.

                Ok::<_, app_models::AppError>((runtime, tools, security))
            })
            .expect("failed to initialize Portico runtime");

        let (runtime, tool_registry, security) = runtime;

            let scheduler = Arc::new(AutomationScheduler::new(
                runtime.task_queue(),
                runtime.storage().clone(),
            ));
            // Memory pattern store shares the app DB but is only accessed through
            // PatternSource/Sink ports by orchestration (no hard module coupling).
            let pattern_store: Arc<dyn app_memory::PatternStore> = Arc::new(
                app_memory::SqlitePatternStore::new(runtime.storage().pool().clone()),
            );
            let pattern_adapter =
                Arc::new(pattern_ports::PatternStoreAdapter::new(pattern_store.clone()));
            let pattern_source: Arc<dyn app_workflows::PatternSource> = pattern_adapter.clone();
            let pattern_sink: Arc<dyn app_workflows::PatternSink> = pattern_adapter;
            let orchestration = Arc::new(
                OrchestrationService::new(Arc::new(runtime.clone()), AgentRegistry::new())
                    .with_pattern_ports(pattern_source, pattern_sink),
            );
            let notification_center = Arc::new(NotificationCenter::new(runtime.storage().clone()));

            let notification_center_for_bridge = notification_center.clone();
            let runtime_for_bridge = Arc::new(runtime.clone());

            let scheduler_for_ticker = scheduler.clone();
            let scheduler_for_worker = scheduler.clone();
            let runtime_for_worker = Arc::new(runtime.clone());

            let state = AppState::new(
                app_data_dir,
                portico_user_dirs,
                Arc::new(runtime.clone()),
                notification_center,
                scheduler,
                secret_store,
                tool_registry,
                security,
            )
            .with_orchestration(orchestration)
            .with_pattern_store(pattern_store);
            app.manage(state);

            // Automations are not product-ready: do not start ticker/worker loops.
            // Keep scheduler constructed for future feature-flag enablement only.
            let _ = (scheduler_for_ticker, scheduler_for_worker, runtime_for_worker);

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
            commands::workspace::create_workspace,
            commands::workspace::list_workspaces,
            commands::workspace::get_workspace,
            commands::workspace::trust_workspace,
            commands::workspace::set_workspace_paths,
            commands::workspace::list_workspace_files,
            commands::workspace::open_workspace_folder,
            commands::workspace::preview_workspace_markdown,
            commands::thread::create_thread,
            commands::thread::list_threads,
            commands::thread::update_thread_title,
            commands::thread::get_thread,
            commands::thread::delete_thread,
            commands::run::start_run,
            commands::run::submit_message,
            commands::run::send_message,
            commands::run::list_messages,
            commands::run::list_runs,
            commands::run::cancel_run,
            commands::run::pause_run,
            commands::run::resume_run,
            commands::run::list_run_events,
            commands::run::get_run_token_usage,
            commands::security::list_pending_approvals,
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
            commands::model::set_active_model,
            commands::model::get_active_model,
            commands::model::resolve_active_model,
            commands::model::test_provider_connection,
            commands::model::get_provider_health,
            commands::model::get_run_model_snapshot,
            commands::secrets::set_provider_secret,
            commands::secrets::delete_provider_secret,
            commands::notification::list_notifications,
            commands::notification::mark_notification_read,
            commands::notification::dismiss_notification,
            commands::notification::send_test_notification,
            commands::memory::list_memories,
            commands::memory::create_memory,
            commands::memory::update_memory,
            commands::memory::delete_memory,
            commands::memory::load_instructions,
            commands::memory::inspect_context,
            commands::memory::search_rag,
            commands::memory::rebuild_rag_index,
            commands::memory::get_feature_capabilities,
            commands::orchestrator::list_agents,
            commands::orchestrator::recall_workflow_patterns,
            commands::orchestrator::list_workflow_patterns,
            commands::orchestrator::preview_orchestration_plan,
            commands::orchestrator::start_orchestration,
            commands::orchestrator::get_orchestration,
            commands::orchestrator::list_thread_orchestrations,
            commands::orchestrator::cancel_orchestration,
            commands::orchestrator::mute_workflow_pattern,
            commands::plugins::install_plugin,
            commands::plugins::install_plugin_package,
            commands::plugins::install_available_plugin_package,
            commands::plugins::install_catalog_plugin,
            commands::plugins::install_plugin_from_github,
            commands::plugins::list_plugin_sources,
            commands::plugins::seed_catalog_plugins,
            commands::plugins::list_available_plugin_packages,
            commands::plugins::get_portico_user_dirs,
            commands::plugins::open_portico_plugins_dir,
            commands::plugins::enable_plugin,
            commands::plugins::uninstall_plugin,
            commands::plugins::list_plugins,
            commands::plugins::read_plugin_entrypoint_html,
            commands::plugins::read_plugin_package_text_file,
            commands::plugins::list_skills,
            commands::plugins::list_mcp_servers,
            commands::diagnostics::collect_diagnostics_bundle,
            commands::migration::list_migrations,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{E2E_DATA_DIR_MARKER, select_e2e_app_data_dir};
    use std::ffi::OsString;

    #[test]
    fn e2e_data_dir_fails_closed_without_an_override() {
        let default = tempfile::tempdir().unwrap();

        assert!(select_e2e_app_data_dir(default.path(), None).is_err());
        assert!(select_e2e_app_data_dir(default.path(), Some(OsString::new())).is_err());
    }

    #[test]
    fn e2e_data_dir_requires_a_marker_and_rejects_the_production_directory() {
        let default = tempfile::tempdir().unwrap();
        let e2e = tempfile::tempdir().unwrap();
        assert!(
            select_e2e_app_data_dir(default.path(), Some(e2e.path().as_os_str().to_owned()),)
                .is_err()
        );

        std::fs::write(
            e2e.path().join(E2E_DATA_DIR_MARKER),
            "portico-desktop-e2e\n",
        )
        .unwrap();
        assert_eq!(
            select_e2e_app_data_dir(default.path(), Some(e2e.path().as_os_str().to_owned()),)
                .unwrap(),
            e2e.path().canonicalize().unwrap()
        );

        std::fs::write(
            default.path().join(E2E_DATA_DIR_MARKER),
            "portico-desktop-e2e\n",
        )
        .unwrap();
        assert!(
            select_e2e_app_data_dir(default.path(), Some(default.path().as_os_str().to_owned()),)
                .is_err()
        );
    }
}
