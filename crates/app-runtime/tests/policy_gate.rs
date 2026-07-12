use app_runtime::{
    AgentRunStatus, PolicyGate, PreparedToolRequest, SqliteStorage, Storage, ToolGateOutcome,
    ToolInvocationStatus,
};
use app_security::{
    DefaultCommandPolicy, DefaultNetworkPolicy, MemoryAuditLogger, PolicyPermissionEngine,
    SecurityContext,
};
use std::path::Path;
use std::sync::Arc;

fn security() -> Arc<SecurityContext> {
    Arc::new(SecurityContext::new(
        Arc::new(PolicyPermissionEngine::default_rules()),
        Arc::new(DefaultCommandPolicy::new()),
        Arc::new(DefaultNetworkPolicy::new()),
        Arc::new(MemoryAuditLogger::new()),
    ))
}

fn request(action: &str, resource: &str, call_id: &str) -> PreparedToolRequest {
    let arguments = if action == "filesystem.write" {
        serde_json::json!({"path": resource, "content": "approved"})
    } else {
        serde_json::json!({"path": resource})
    };
    PreparedToolRequest {
        model_call_id: Some(call_id.to_owned()),
        tool_name: if action == "filesystem.write" {
            "fs_write".to_owned()
        } else {
            "fs_read".to_owned()
        },
        tool_version: "1".to_owned(),
        action: action.to_owned(),
        resource: resource.to_owned(),
        arguments,
        recovery: None,
    }
}

#[tokio::test]
async fn policy_gate_uses_authoritative_workspace_and_persists_allow_ask_deny() {
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
    let root_a = std::env::temp_dir().join(format!("portico-gate-a-{}", uuid::Uuid::new_v4()));
    let root_b = std::env::temp_dir().join(format!("portico-gate-b-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root_a).expect("root a");
    std::fs::create_dir_all(&root_b).expect("root b");
    let root_a = root_a.canonicalize().expect("canonical root a");
    let root_b = root_b.canonicalize().expect("canonical root b");
    let file_a = root_a.join("a.txt");
    let file_b = root_b.join("b.txt");
    std::fs::write(&file_a, "a").expect("file a");
    std::fs::write(&file_b, "b").expect("file b");

    let workspace_a = storage
        .create_workspace("a", &root_a.to_string_lossy(), true)
        .await
        .expect("workspace a");
    let workspace_b = storage
        .create_workspace("b", &root_b.to_string_lossy(), true)
        .await
        .expect("workspace b");
    storage
        .set_workspace_allowed_paths(
            workspace_a.id,
            vec![root_a.to_string_lossy().into_owned()],
            vec![root_a.to_string_lossy().into_owned()],
        )
        .await
        .expect("paths a");
    storage
        .set_workspace_allowed_paths(
            workspace_b.id,
            vec![root_b.to_string_lossy().into_owned()],
            vec![root_b.to_string_lossy().into_owned()],
        )
        .await
        .expect("paths b");
    let thread_a = storage.create_thread(workspace_a.id, "a").await.expect("thread a");
    let run_a = storage.create_run(workspace_a.id, thread_a.id).await.expect("run a");
    storage
        .update_run_status(run_a.id, AgentRunStatus::Running)
        .await
        .expect("running a");

    let gate = PolicyGate::new(storage.clone(), security());

    let cross_workspace = gate
        .prepare(
            run_a.id,
            request("filesystem.read", &file_b.to_string_lossy(), "cross"),
        )
        .await;
    assert!(cross_workspace.is_err());
    assert!(storage.list_tool_invocations(run_a.id).await.expect("list").is_empty());

    let allowed = gate
        .prepare(
            run_a.id,
            request("filesystem.read", &file_a.to_string_lossy(), "read-own"),
        )
        .await
        .expect("allow read");
    let ToolGateOutcome::Ready(allowed) = allowed else {
        panic!("trusted own read should be ready");
    };
    assert_eq!(allowed.status, ToolInvocationStatus::Ready);
    assert_eq!(allowed.workspace_id, workspace_a.id);
    assert_eq!(allowed.thread_id, thread_a.id);

    let write_target = root_a.join("result.txt");
    let write = gate
        .prepare(
            run_a.id,
            request(
                "filesystem.write",
                &write_target.to_string_lossy(),
                "write-own",
            ),
        )
        .await
        .expect("prepare write");
    // Trusted projects allow write under the project root without approval.
    let ToolGateOutcome::Ready(invocation) = write else {
        panic!("trusted write should be ready, got {write:?}");
    };
    assert_eq!(invocation.status, ToolInvocationStatus::Ready);
    assert!(
        !write_target.exists(),
        "policy preparation must have zero side effect"
    );
}

#[tokio::test]
async fn relative_paths_resolve_against_workspace_root() {
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
    let root = std::env::temp_dir().join(format!("portico-gate-rel-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("docs")).expect("root");
    let root = root.canonicalize().expect("canonical root");
    let file = root.join("docs").join("note.txt");
    std::fs::write(&file, "hello").expect("file");

    let workspace = storage
        .create_workspace("rel", &root.to_string_lossy(), true)
        .await
        .expect("workspace");
    storage
        .set_workspace_allowed_paths(
            workspace.id,
            vec![root.to_string_lossy().into_owned()],
            vec![root.to_string_lossy().into_owned()],
        )
        .await
        .expect("paths");
    let thread = storage.create_thread(workspace.id, "t").await.expect("thread");
    let run = storage.create_run(workspace.id, thread.id).await.expect("run");
    storage
        .update_run_status(run.id, AgentRunStatus::Running)
        .await
        .expect("running");

    let gate = PolicyGate::new(storage.clone(), security());
    let relative = gate
        .prepare(
            run.id,
            request("filesystem.read", "docs/note.txt", "rel-read"),
        )
        .await
        .expect("relative read");
    let ToolGateOutcome::Ready(invocation) = relative else {
        panic!("relative trusted read should be ready");
    };
    assert_eq!(
        Path::new(&invocation.resource),
        file.as_path(),
        "relative path must canonicalize under the workspace root"
    );

    let root_list = gate
        .prepare(
            run.id,
            PreparedToolRequest {
                model_call_id: Some("list-root".to_owned()),
                tool_name: "fs_list".to_owned(),
                tool_version: "1".to_owned(),
                action: "filesystem.read".to_owned(),
                resource: ".".to_owned(),
                arguments: serde_json::json!({"path": "."}),
                recovery: None,
            },
        )
        .await
        .expect("list root");
    let ToolGateOutcome::Ready(list_invocation) = root_list else {
        panic!("list root should be ready");
    };
    assert_eq!(Path::new(&list_invocation.resource), root.as_path());
}

#[tokio::test]
async fn untrusted_read_is_durably_denied_without_approval() {
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
    let root = std::env::temp_dir().join(format!("portico-gate-deny-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    let file = root.join("secret.txt");
    std::fs::write(&file, "secret").expect("file");
    let workspace = storage
        .create_workspace("untrusted", &root.to_string_lossy(), false)
        .await
        .expect("workspace");
    storage
        .set_workspace_allowed_paths(
            workspace.id,
            vec![root.to_string_lossy().into_owned()],
            Vec::new(),
        )
        .await
        .expect("paths");
    let thread = storage.create_thread(workspace.id, "thread").await.expect("thread");
    let run = storage.create_run(workspace.id, thread.id).await.expect("run");
    storage
        .update_run_status(run.id, AgentRunStatus::Running)
        .await
        .expect("running");

    let outcome = PolicyGate::new(storage.clone(), security())
        .prepare(
            run.id,
            request("filesystem.read", &file.to_string_lossy(), "denied-read"),
        )
        .await
        .expect("durable deny");
    let ToolGateOutcome::Denied(invocation) = outcome else {
        panic!("untrusted read should be denied");
    };
    assert_eq!(invocation.status, ToolInvocationStatus::Denied);
    assert!(
        storage
            .list_pending_approval_requests(Some(run.id))
            .await
            .expect("pending")
            .is_empty()
    );
}
