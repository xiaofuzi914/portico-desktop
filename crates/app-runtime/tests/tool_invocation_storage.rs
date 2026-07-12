use app_runtime::{
    ApprovalRequest, ApprovalRequestId, ApprovalRequestStatus, SqliteStorage, Storage,
    ToolInvocation, ToolInvocationId, ToolInvocationStatus,
};
use chrono::Utc;

async fn fixture() -> (SqliteStorage, ToolInvocation) {
    let storage = SqliteStorage::open_in_memory().await.expect("open db");
    let root = std::env::temp_dir().join(format!("portico-tool-store-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("workspace root");
    let workspace = storage
        .create_workspace("tools", &root.to_string_lossy(), true)
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
        request_hash: "request-hash".to_owned(),
        policy_version: "policy-v1".to_owned(),
        context_revision: workspace.updated_at,
        status: ToolInvocationStatus::WaitingApproval,
        approval_request_id: None,
        result: None,
        error: None,
        recovery: Some(serde_json::json!({"post_hash": "post"})),
        lease_token: None,
        attempts: 0,
        cancel_requested: false,
        correlation_id: uuid::Uuid::new_v4(),
        created_at: now,
        updated_at: now,
        started_at: None,
        completed_at: None,
    };
    (storage, invocation)
}

#[tokio::test]
async fn approval_and_invocation_are_created_and_resolved_atomically() {
    let (storage, invocation) = fixture().await;
    let approval = ApprovalRequest {
        id: ApprovalRequestId(0),
        run_id: invocation.run_id,
        workspace_id: invocation.workspace_id,
        thread_id: invocation.thread_id,
        action: invocation.action.clone(),
        resource: invocation.resource.clone(),
        status: ApprovalRequestStatus::Pending,
        created_at: Utc::now(),
        resolved_at: None,
        resolution_reason: None,
    };

    let (stored, approval) = storage
        .create_tool_invocation_with_approval(&invocation, &approval)
        .await
        .expect("create atomically");
    assert_eq!(stored.approval_request_id, Some(approval.id));
    assert_eq!(stored.status, ToolInvocationStatus::WaitingApproval);

    let (approval, ready) = storage
        .resolve_tool_approval(approval.id, ApprovalRequestStatus::Approved, None)
        .await
        .expect("approve atomically");
    assert_eq!(approval.status, ApprovalRequestStatus::Approved);
    assert_eq!(ready.status, ToolInvocationStatus::Approved);

    let duplicate = storage
        .resolve_tool_approval(approval.id, ApprovalRequestStatus::Approved, None)
        .await
        .expect("same decision is idempotent");
    assert_eq!(duplicate.1.status, ToolInvocationStatus::Approved);
}

#[tokio::test]
async fn an_invocation_can_only_be_claimed_and_completed_once() {
    let (storage, mut invocation) = fixture().await;
    invocation.status = ToolInvocationStatus::Ready;
    let stored = storage.create_tool_invocation(&invocation).await.expect("create");
    let claimed = storage.claim_tool_invocation(stored.id).await.expect("claim");
    assert_eq!(claimed.status, ToolInvocationStatus::Executing);
    assert_eq!(claimed.attempts, 1);
    assert!(storage.claim_tool_invocation(stored.id).await.is_err());

    let completed = storage
        .complete_tool_invocation(
            stored.id,
            claimed.lease_token.expect("lease"),
            serde_json::json!({"ok": true}),
        )
        .await
        .expect("complete");
    assert_eq!(completed.status, ToolInvocationStatus::Succeeded);
    assert!(storage.claim_tool_invocation(stored.id).await.is_err());
}

#[tokio::test]
async fn restart_never_blindly_replays_an_executing_invocation() {
    let (storage, mut invocation) = fixture().await;
    invocation.status = ToolInvocationStatus::Ready;
    storage.create_tool_invocation(&invocation).await.expect("create");
    storage.claim_tool_invocation(invocation.id).await.expect("claim");

    assert_eq!(
        storage.recover_tool_invocations().await.expect("recover"),
        1
    );
    let recovered = storage.get_tool_invocation(invocation.id).await.expect("get");
    assert_eq!(recovered.status, ToolInvocationStatus::NeedsReconciliation);
    assert!(storage.claim_tool_invocation(invocation.id).await.is_err());
}

#[tokio::test]
async fn cancellation_closes_waiting_approval_and_never_mislabels_an_executing_effect() {
    let (storage, waiting) = fixture().await;
    let approval = ApprovalRequest {
        id: ApprovalRequestId(0),
        run_id: waiting.run_id,
        workspace_id: waiting.workspace_id,
        thread_id: waiting.thread_id,
        action: waiting.action.clone(),
        resource: waiting.resource.clone(),
        status: ApprovalRequestStatus::Pending,
        created_at: Utc::now(),
        resolved_at: None,
        resolution_reason: None,
    };
    let (_, approval) = storage
        .create_tool_invocation_with_approval(&waiting, &approval)
        .await
        .expect("waiting");
    assert_eq!(
        storage.cancel_tool_invocations_for_run(waiting.run_id).await.expect("cancel"),
        1
    );
    assert_eq!(
        storage.get_tool_invocation(waiting.id).await.expect("invocation").status,
        ToolInvocationStatus::Cancelled
    );
    assert_eq!(
        storage.get_approval_request(approval.id).await.expect("approval").status,
        ApprovalRequestStatus::Denied
    );

    let (storage, mut executing) = fixture().await;
    executing.status = ToolInvocationStatus::Ready;
    storage.create_tool_invocation(&executing).await.expect("create ready");
    storage.claim_tool_invocation(executing.id).await.expect("claim");
    storage
        .cancel_tool_invocations_for_run(executing.run_id)
        .await
        .expect("request cancel");
    let executing = storage.get_tool_invocation(executing.id).await.expect("executing");
    assert_eq!(executing.status, ToolInvocationStatus::Executing);
    assert!(executing.cancel_requested);
}
