use app_runtime::{
    AgentRunStatus, MemoryEventBus, PorticoRuntimeHandle, SqliteModelProviderRegistry,
    SqliteStorage, Storage, ToolInvocationStatus,
};
use autoagents_adapter::{AutoAgentsExecutor, MockLlmProvider, PorticoToolRegistry};
use autoagents_llm::LLMProvider;
use std::sync::Arc;

#[tokio::test]
async fn model_write_on_trusted_workspace_completes_without_approval() {
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
    let event_bus = Arc::new(MemoryEventBus::default());
    let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
    let tools = Arc::new(PorticoToolRegistry::new());
    tools.register_safe_builtin_definitions();
    let llm: Arc<dyn LLMProvider> = Arc::new(MockLlmProvider::new());
    let executor = Arc::new(AutoAgentsExecutor::new(llm, tools));
    let runtime =
        PorticoRuntimeHandle::new(storage.clone(), event_bus, registry, Some(executor), None)
            .await
            .expect("runtime");

    let root = std::env::temp_dir().join(format!("portico-durable-loop-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    let workspace = runtime
        .create_workspace("durable", &root.to_string_lossy(), false)
        .await
        .expect("workspace");
    runtime
        .workspace_manager()
        .trust_workspace(workspace.id, true)
        .await
        .expect("trust");
    let thread = runtime.create_thread(workspace.id, "thread").await.expect("thread");
    let run = runtime.start_run(workspace.id, thread.id).await.expect("run");
    let target = root.join("approved.txt");

    runtime
        .submit_message(
            run.id,
            &format!("PORTICO_TEST_WRITE:{}", target.to_string_lossy()),
        )
        .await
        .expect("submit write");
    assert_eq!(
        runtime.get_run(run.id).await.expect("completed run").status,
        AgentRunStatus::Completed
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("written target"),
        "approved by durable loop\n"
    );
    let invocations = storage.list_tool_invocations(run.id).await.expect("invocations");
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].status, ToolInvocationStatus::Succeeded);
    assert_eq!(invocations[0].attempts, 1);
    assert!(storage.load_agent_checkpoint(run.id).await.expect("checkpoint").is_some());
}

#[tokio::test]
async fn untrusted_write_is_denied() {
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
    let event_bus = Arc::new(MemoryEventBus::default());
    let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
    let tools = Arc::new(PorticoToolRegistry::new());
    tools.register_safe_builtin_definitions();
    let llm: Arc<dyn LLMProvider> = Arc::new(MockLlmProvider::new());
    let executor = Arc::new(AutoAgentsExecutor::new(llm, tools));
    let runtime =
        PorticoRuntimeHandle::new(storage.clone(), event_bus, registry, Some(executor), None)
            .await
            .expect("runtime");

    let root = std::env::temp_dir().join(format!("portico-untrusted-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    let workspace = runtime
        .create_workspace("untrusted", &root.to_string_lossy(), false)
        .await
        .expect("workspace");
    // Explicit paths but not trusted → write still denied by policy.
    runtime
        .workspace_manager()
        .set_allowed_paths(
            workspace.id,
            vec![root.to_string_lossy().into_owned()],
            vec![root.to_string_lossy().into_owned()],
        )
        .await
        .expect("paths");
    let thread = runtime.create_thread(workspace.id, "thread").await.expect("thread");
    let run = runtime.start_run(workspace.id, thread.id).await.expect("run");
    let target = root.join("denied.txt");

    runtime
        .submit_message(
            run.id,
            &format!("PORTICO_TEST_WRITE:{}", target.to_string_lossy()),
        )
        .await
        .expect("submit");
    // Denied tool results are recovered into the tool loop; run may still complete
    // with an assistant message, but the file must not be written.
    assert!(!target.exists(), "untrusted write must not create the file");
}
