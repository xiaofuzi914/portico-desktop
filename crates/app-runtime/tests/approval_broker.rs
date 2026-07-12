use app_runtime::{
    ApprovalBroker, ApprovalExecutionOutcome, ApprovalRequest, ApprovalRequestId,
    ApprovalRequestStatus, SqliteStorage, Storage, ToolInvocation, ToolInvocationId,
    ToolInvocationStatus,
};
use chrono::Utc;
use std::sync::Arc;

async fn waiting_fixture() -> (Arc<SqliteStorage>, ApprovalRequestId, ToolInvocationId) {
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
    let root = std::env::temp_dir().join(format!("portico-broker-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    let workspace = storage
        .create_workspace("broker", &root.to_string_lossy(), true)
        .await
        .expect("workspace");
    let thread = storage.create_thread(workspace.id, "thread").await.expect("thread");
    let run = storage.create_run(workspace.id, thread.id).await.expect("run");
    let now = Utc::now();
    let invocation = ToolInvocation {
        id: ToolInvocationId::new(),
        run_id: run.id,
        thread_id: thread.id,
        workspace_id: workspace.id,
        model_call_id: Some("call-1".to_owned()),
        tool_name: "fs_write".to_owned(),
        tool_version: "1".to_owned(),
        action: "filesystem.write".to_owned(),
        resource: root.join("result.txt").to_string_lossy().into_owned(),
        arguments: serde_json::json!({"content": "done"}),
        request_hash: "hash".to_owned(),
        policy_version: "policy-v1".to_owned(),
        context_revision: workspace.updated_at,
        status: ToolInvocationStatus::WaitingApproval,
        approval_request_id: None,
        result: None,
        error: None,
        recovery: None,
        lease_token: None,
        attempts: 0,
        cancel_requested: false,
        correlation_id: uuid::Uuid::new_v4(),
        created_at: now,
        updated_at: now,
        started_at: None,
        completed_at: None,
    };
    let approval = ApprovalRequest {
        id: ApprovalRequestId(0),
        run_id: run.id,
        workspace_id: workspace.id,
        thread_id: thread.id,
        action: invocation.action.clone(),
        resource: invocation.resource.clone(),
        status: ApprovalRequestStatus::Pending,
        created_at: now,
        resolved_at: None,
        resolution_reason: None,
    };
    let (invocation, approval) = storage
        .create_tool_invocation_with_approval(&invocation, &approval)
        .await
        .expect("create waiting");
    (storage, approval.id, invocation.id)
}

#[tokio::test]
async fn duplicate_approve_yields_one_execution_grant() {
    let (storage, approval_id, invocation_id) = waiting_fixture().await;
    let broker = ApprovalBroker::new(storage.clone());
    let first = broker.approve(approval_id).await.expect("first approve");
    let ApprovalExecutionOutcome::Granted(grant) = first else {
        panic!("first approval must win the claim");
    };

    let second = broker.approve(approval_id).await.expect("duplicate approve");
    assert!(matches!(
        second,
        ApprovalExecutionOutcome::AlreadyHandled(_)
    ));
    let completed =
        broker.complete(grant, serde_json::json!({"ok": true})).await.expect("complete");
    assert_eq!(completed.status, ToolInvocationStatus::Succeeded);

    let third = broker.approve(approval_id).await.expect("approve completed");
    assert!(matches!(third, ApprovalExecutionOutcome::AlreadyHandled(_)));
    assert_eq!(
        storage.get_tool_invocation(invocation_id).await.expect("get").attempts,
        1
    );
}

#[tokio::test]
async fn concurrent_approve_has_exactly_one_winner() {
    let (storage, approval_id, invocation_id) = waiting_fixture().await;
    let broker = ApprovalBroker::new(storage.clone());
    let (left, right) = tokio::join!(broker.approve(approval_id), broker.approve(approval_id));
    let outcomes = [left.expect("left"), right.expect("right")];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, ApprovalExecutionOutcome::Granted(_)))
            .count(),
        1
    );
    assert_eq!(
        storage.get_tool_invocation(invocation_id).await.expect("get").attempts,
        1
    );
}

#[tokio::test]
async fn deny_then_approve_is_a_conflict_and_never_claims() {
    let (storage, approval_id, invocation_id) = waiting_fixture().await;
    let broker = ApprovalBroker::new(storage.clone());
    let (_, denied) = broker.deny(approval_id, Some("no")).await.expect("deny");
    assert_eq!(denied.status, ToolInvocationStatus::Denied);
    assert!(broker.approve(approval_id).await.is_err());
    assert_eq!(
        storage.get_tool_invocation(invocation_id).await.expect("get").attempts,
        0
    );
}
