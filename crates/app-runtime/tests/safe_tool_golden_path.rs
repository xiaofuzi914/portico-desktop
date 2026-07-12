use app_runtime::{
    AgentRunStatus, ApprovalBroker, MemoryEventBus, MockAgentExecutor, PolicyGate,
    PorticoRuntimeHandle, PreparedToolRequest, SafeToolExecutor, SqliteModelProviderRegistry,
    SqliteStorage, Storage, ToolGateOutcome, ToolInvocationStatus,
};
use app_security::{
    DefaultCommandPolicy, DefaultNetworkPolicy, MemoryAuditLogger, PolicyPermissionEngine,
    SecurityContext,
};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

fn security() -> Arc<SecurityContext> {
    Arc::new(SecurityContext::new(
        Arc::new(PolicyPermissionEngine::default_rules()),
        Arc::new(DefaultCommandPolicy::new()),
        Arc::new(DefaultNetworkPolicy::new()),
        Arc::new(MemoryAuditLogger::new()),
    ))
}

#[tokio::test]
async fn restart_after_file_effect_reconciles_receipt_without_replaying_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("workspace");
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    let database = temp.path().join("portico.sqlite");
    let target = root.join("result.txt");
    std::fs::write(&target, "before\n").expect("baseline");

    let invocation_id = {
        let storage = Arc::new(SqliteStorage::open(&database).await.expect("open db"));
        let workspace = storage
            .create_workspace("safe", &root.to_string_lossy(), true)
            .await
            .expect("workspace");
        storage
            .set_workspace_allowed_paths(
                workspace.id,
                vec![root.to_string_lossy().into_owned()],
                vec![root.to_string_lossy().into_owned()],
            )
            .await
            .expect("allowed paths");
        let thread = storage.create_thread(workspace.id, "thread").await.expect("thread");
        let run = storage.create_run(workspace.id, thread.id).await.expect("run");
        storage
            .update_run_status(run.id, AgentRunStatus::Running)
            .await
            .expect("running");
        let gate = PolicyGate::new(storage.clone(), security());
        let executor = SafeToolExecutor::new(gate.clone());
        let broker = ApprovalBroker::new(storage.clone());
        let write = gate
            .prepare(
                run.id,
                PreparedToolRequest {
                    model_call_id: Some("write-crash".to_owned()),
                    tool_name: "fs_write".to_owned(),
                    tool_version: "1".to_owned(),
                    action: "filesystem.write".to_owned(),
                    resource: target.to_string_lossy().into_owned(),
                    arguments: serde_json::json!({"path": target, "content": "after\n"}),
                    recovery: None,
                },
            )
            .await
            .expect("prepare write");
        let ToolGateOutcome::Ready(ready) = write else {
            panic!("trusted write should be ready");
        };
        let grant = broker.claim_ready(ready.id).await.expect("claim write");
        let invocation_id = grant.invocation().id;
        executor.execute(&grant).await.expect("apply effect");
        assert_eq!(std::fs::read_to_string(&target).expect("effect"), "after\n");
        // Deliberately drop the grant and pool before recording completion.
        invocation_id
    };

    let storage = Arc::new(SqliteStorage::open(&database).await.expect("reopen db"));
    let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
    let runtime = PorticoRuntimeHandle::new(
        storage.clone(),
        Arc::new(MemoryEventBus::default()),
        registry,
        Some(Arc::new(MockAgentExecutor::new())),
        None,
    )
    .await
    .expect("runtime");
    assert_eq!(
        storage
            .get_tool_invocation(invocation_id)
            .await
            .expect("needs reconciliation")
            .status,
        ToolInvocationStatus::NeedsReconciliation
    );
    assert_eq!(runtime.recover_tool_execution().await.expect("recover"), 1);
    let completed = storage.get_tool_invocation(invocation_id).await.expect("completed");
    assert_eq!(completed.status, ToolInvocationStatus::Succeeded);
    assert_eq!(
        completed.attempts, 1,
        "reconciliation must not claim or replay"
    );
    assert_eq!(std::fs::read_to_string(&target).expect("target"), "after\n");
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn read_approved_atomic_write_and_git_diff_form_one_safe_golden_path() {
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
    let root = std::env::temp_dir().join(format!("portico-safe-tools-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    git(&root, &["init"]);
    git(&root, &["config", "user.email", "test@portico.local"]);
    git(&root, &["config", "user.name", "Portico Test"]);
    let source = root.join("source.txt");
    let target = root.join("result.txt");
    std::fs::write(&source, "source").expect("source");
    std::fs::write(&target, "before\n").expect("target baseline");
    git(&root, &["add", "source.txt", "result.txt"]);
    git(&root, &["commit", "-m", "baseline"]);

    let workspace = storage
        .create_workspace("safe", &root.to_string_lossy(), true)
        .await
        .expect("workspace");
    storage
        .set_workspace_allowed_paths(
            workspace.id,
            vec![root.to_string_lossy().into_owned()],
            vec![root.to_string_lossy().into_owned()],
        )
        .await
        .expect("allowed paths");
    let thread = storage.create_thread(workspace.id, "thread").await.expect("thread");
    let run = storage.create_run(workspace.id, thread.id).await.expect("run");
    storage
        .update_run_status(run.id, AgentRunStatus::Running)
        .await
        .expect("running");

    let gate = PolicyGate::new(storage.clone(), security());
    let executor = SafeToolExecutor::new(gate.clone());
    let broker = ApprovalBroker::new(storage.clone());

    let list = gate
        .prepare(
            run.id,
            PreparedToolRequest {
                model_call_id: Some("list-1".to_owned()),
                tool_name: "fs_list".to_owned(),
                tool_version: "1".to_owned(),
                action: "filesystem.read".to_owned(),
                resource: ".".to_owned(),
                arguments: serde_json::json!({"path": "."}),
                recovery: None,
            },
        )
        .await
        .expect("prepare list");
    let ToolGateOutcome::Ready(list) = list else {
        panic!("list should be ready");
    };
    let list_grant = broker.claim_ready(list.id).await.expect("claim list");
    let list_result = executor.execute(&list_grant).await.expect("execute list");
    let entries = list_result["entries"].as_array().expect("entries");
    assert!(
        entries.iter().any(|entry| entry["name"] == "source.txt"),
        "list must include source.txt: {list_result}"
    );
    broker.complete(list_grant, list_result).await.expect("complete list");

    let read = gate
        .prepare(
            run.id,
            PreparedToolRequest {
                model_call_id: Some("read-1".to_owned()),
                tool_name: "fs_read".to_owned(),
                tool_version: "1".to_owned(),
                action: "filesystem.read".to_owned(),
                resource: "source.txt".to_owned(),
                arguments: serde_json::json!({"path": "source.txt"}),
                recovery: None,
            },
        )
        .await
        .expect("prepare read");
    let ToolGateOutcome::Ready(read) = read else {
        panic!("read should be ready");
    };
    let read_grant = broker.claim_ready(read.id).await.expect("claim read");
    let read_result = executor.execute(&read_grant).await.expect("execute read");
    assert_eq!(read_result["content"], "source");
    broker.complete(read_grant, read_result).await.expect("complete read");

    let write = gate
        .prepare(
            run.id,
            PreparedToolRequest {
                model_call_id: Some("write-1".to_owned()),
                tool_name: "fs_write".to_owned(),
                tool_version: "1".to_owned(),
                action: "filesystem.write".to_owned(),
                resource: target.to_string_lossy().into_owned(),
                arguments: serde_json::json!({"path": target, "content": "approved\n"}),
                recovery: None,
            },
        )
        .await
        .expect("prepare write");
    let ToolGateOutcome::Ready(invocation) = write else {
        panic!("trusted write should be ready");
    };
    assert_eq!(
        std::fs::read_to_string(&target).expect("pre-execution target"),
        "before\n",
        "prepare must have zero side effect"
    );
    assert!(
        invocation.recovery.is_some(),
        "gate must own the recovery receipt"
    );

    let write_grant = broker.claim_ready(invocation.id).await.expect("claim write");
    let write_result = executor.execute(&write_grant).await.expect("execute write");
    let completed = broker.complete(write_grant, write_result).await.expect("complete write");
    assert_eq!(completed.status, ToolInvocationStatus::Succeeded);
    assert_eq!(
        std::fs::read_to_string(&target).expect("target"),
        "approved\n"
    );

    let git_diff = gate
        .prepare(
            run.id,
            PreparedToolRequest {
                model_call_id: Some("git-diff-1".to_owned()),
                tool_name: "git".to_owned(),
                tool_version: "1".to_owned(),
                action: "git.read".to_owned(),
                resource: root.to_string_lossy().into_owned(),
                arguments: serde_json::json!({"subcommand": "diff"}),
                recovery: None,
            },
        )
        .await
        .expect("prepare git diff");
    let ToolGateOutcome::Ready(git_diff) = git_diff else {
        panic!("git diff should be ready");
    };
    let git_grant = broker.claim_ready(git_diff.id).await.expect("claim git");
    let diff = executor.execute(&git_grant).await.expect("execute git diff");
    broker.complete(git_grant, diff.clone()).await.expect("complete git");
    assert!(
        diff["output"].as_str().is_some_and(|output| output.contains("result.txt")),
        "git diff must expose the approved file change: {diff}"
    );
}
