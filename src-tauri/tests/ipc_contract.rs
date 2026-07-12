use app_runtime::{
    MemoryEventBus, NotificationCenter, PorticoRuntimeHandle, SqliteModelProviderRegistry,
    SqliteStorage, Storage,
};
use app_security::{AuditEvent, PermissionResult};
use app_workflows::AutomationScheduler;
use autoagents_adapter::PorticoToolRegistry;
use portico_tauri::{AppState, commands};
use std::sync::Arc;
use tauri::webview::InvokeRequest;

fn invoke_request(command: &str, body: serde_json::Value) -> InvokeRequest {
    InvokeRequest {
        cmd: command.to_owned(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: "tauri://localhost".parse().expect("url"),
        body: tauri::ipc::InvokeBody::Json(body),
        headers: tauri::http::HeaderMap::new(),
        invoke_key: tauri::test::INVOKE_KEY.to_owned(),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::too_many_lines)]
async fn real_tauri_ipc_deserializes_camel_case_and_returns_product_envelope() {
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("db"));
    let event_bus = Arc::new(MemoryEventBus::default());
    let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
    let security = Arc::new(app_runtime::SecurityContext::new(
        Arc::new(app_security::PolicyPermissionEngine::default_rules()),
        Arc::new(app_security::DefaultCommandPolicy::new()),
        Arc::new(app_security::DefaultNetworkPolicy::new()),
        Arc::new(app_runtime::SqliteAuditLogger::new(storage.clone())),
    ));
    let runtime = Arc::new(
        PorticoRuntimeHandle::new(storage.clone(), event_bus, registry, None, None)
            .await
            .expect("runtime")
            .with_security(security.clone()),
    );
    let root = std::env::temp_dir().join(format!("portico-ipc-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let workspace = runtime
        .create_workspace("IPC", &root.to_string_lossy(), false)
        .await
        .expect("workspace");
    let diagnostics_canary = "portico-diagnostics-canary-b7340e8f";
    runtime
        .audit(AuditEvent {
            workspace_id: workspace.id,
            thread_id: None,
            run_id: None,
            action: "test.canary".to_owned(),
            resource: diagnostics_canary.to_owned(),
            outcome: PermissionResult::Allowed,
        })
        .expect("seed canary audit");
    let scheduler = Arc::new(AutomationScheduler::new(
        runtime.task_queue(),
        storage.clone(),
    ));
    let portico_user_dirs =
        portico_tauri::portico_paths::ensure_portico_user_dirs_at(root.join("portico-home"))
            .expect("portico user dirs");
    let state = AppState::new(
        root.join("app-data"),
        portico_user_dirs,
        runtime,
        Arc::new(NotificationCenter::new(storage)),
        scheduler,
        Arc::new(app_security::InMemorySecretStore::new()),
        Arc::new(PorticoToolRegistry::new()),
        security,
    );
    let app = tauri::test::mock_builder()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::workspace::list_workspaces,
            commands::thread::create_thread,
            commands::thread::delete_thread,
            commands::security::list_pending_approvals,
            commands::model::list_providers,
            commands::model::create_provider,
            commands::secrets::set_provider_secret,
            commands::diagnostics::collect_diagnostics_bundle,
            commands::migration::list_migrations
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .expect("webview");

    let response = tauri::test::get_ipc_response(
        &webview,
        invoke_request(
            "create_thread",
            serde_json::json!({"workspaceId": workspace.id, "title": "IPC thread"}),
        ),
    )
    .expect("create thread IPC")
    .deserialize::<serde_json::Value>()
    .expect("thread envelope");
    assert_eq!(response["success"], true);
    assert_eq!(response["data"]["workspace_id"], workspace.id.0.to_string());

    let response = tauri::test::get_ipc_response(
        &webview,
        invoke_request("list_workspaces", serde_json::json!({})),
    )
    .expect("list workspaces IPC")
    .deserialize::<serde_json::Value>()
    .expect("workspace envelope");
    assert_eq!(response["data"].as_array().map(Vec::len), Some(1));

    let response = tauri::test::get_ipc_response(
        &webview,
        invoke_request(
            "create_provider",
            serde_json::json!({
                "kind": "OpenAI",
                "displayName": "IPC Provider",
                "baseUrl": null,
                "apiKeyReference": "ipc-provider-key"
            }),
        ),
    )
    .expect("create provider IPC")
    .deserialize::<serde_json::Value>()
    .expect("provider envelope");
    assert_eq!(response["success"], true);

    let response = tauri::test::get_ipc_response(
        &webview,
        invoke_request("list_providers", serde_json::json!({})),
    )
    .expect("list providers IPC")
    .deserialize::<serde_json::Value>()
    .expect("provider list envelope");
    assert_eq!(response["data"].as_array().map(Vec::len), Some(1));

    let response = tauri::test::get_ipc_response(
        &webview,
        invoke_request(
            "set_provider_secret",
            serde_json::json!({"apiKeyReference": "", "apiKey": "not-persisted"}),
        ),
    )
    .expect("invalid secret IPC")
    .deserialize::<serde_json::Value>()
    .expect("secret error envelope");
    assert_eq!(response["success"], false);

    let response = tauri::test::get_ipc_response(
        &webview,
        invoke_request("list_pending_approvals", serde_json::json!({"runId": null})),
    )
    .expect("list approval IPC")
    .deserialize::<serde_json::Value>()
    .expect("approval envelope");
    assert_eq!(
        response,
        serde_json::json!({"success": true, "data": [], "error": null})
    );

    let response = tauri::test::get_ipc_response(
        &webview,
        invoke_request("collect_diagnostics_bundle", serde_json::json!({})),
    )
    .expect("collect diagnostics IPC")
    .deserialize::<serde_json::Value>()
    .expect("diagnostics envelope");
    assert_eq!(response["success"], true);
    let log_path = response["data"]["log_path"].as_str().expect("log path");
    let audit_path = response["data"]["audit_summary_path"].as_str().expect("audit path");
    let exported = format!(
        "{}\n{}",
        std::fs::read_to_string(log_path).expect("safe log export"),
        std::fs::read_to_string(audit_path).expect("safe audit export")
    );
    assert!(!exported.contains(diagnostics_canary));
    assert!(exported.contains("\"payloads_included\": false"));

    let response = tauri::test::get_ipc_response(
        &webview,
        invoke_request("list_migrations", serde_json::json!({})),
    )
    .expect("list migrations IPC")
    .deserialize::<serde_json::Value>()
    .expect("migration envelope");
    assert_eq!(response["success"], true);
    assert!(response["data"].as_array().is_some_and(|rows| !rows.is_empty()));
}
