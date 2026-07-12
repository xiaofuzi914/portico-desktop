//! Integration tests for run recovery and executor resolution on
//! [`PorticoRuntimeHandle`].
//!
//! These tests exercise only the public API and use deterministic, short
//! polling (no single sleep over 50ms) to observe terminal run states.

use app_runtime::{
    AgentExecutionOutcome, AgentExecutor, AgentExecutorResolver, AgentRun, AgentRunId,
    AgentRunStatus, AppError, MemoryEventBus, ModelId, PorticoRuntimeHandle, ProviderId,
    ResolvedAgentExecutor, RunModelSnapshot, SqliteModelProviderRegistry, SqliteStorage, Storage,
    ThreadId, WorkspaceId,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn ensure_test_workspace_root() {
    std::fs::create_dir_all("/tmp/test").expect("create test workspace root");
}

/// Executor that finishes immediately with a fixed assistant reply.
#[derive(Debug, Default)]
struct FastExecutor;

#[async_trait]
impl AgentExecutor for FastExecutor {
    async fn execute(
        &self,
        _run_id: AgentRunId,
        _thread_id: ThreadId,
        _workspace_id: WorkspaceId,
        _message: &str,
        _storage: Arc<dyn Storage>,
        _event_bus: Arc<dyn app_runtime::EventBus>,
        token: CancellationToken,
    ) -> Result<AgentExecutionOutcome, AppError> {
        if token.is_cancelled() {
            return Ok(AgentExecutionOutcome::Completed(String::new()));
        }
        Ok(AgentExecutionOutcome::Completed("done".to_owned()))
    }
}

/// Resolver that counts how many times it is asked to resolve and always hands
/// back the same fast executor.
#[derive(Clone)]
struct CountingResolver {
    resolves: Arc<AtomicUsize>,
    executor: Arc<dyn AgentExecutor>,
}

#[async_trait]
impl AgentExecutorResolver for CountingResolver {
    async fn resolve(
        &self,
        _workspace_id: WorkspaceId,
        _thread_id: ThreadId,
        run_id: AgentRunId,
    ) -> Result<ResolvedAgentExecutor, AppError> {
        self.resolves.fetch_add(1, Ordering::SeqCst);
        let now = chrono::Utc::now();
        Ok(ResolvedAgentExecutor {
            executor: self.executor.clone(),
            snapshot: RunModelSnapshot {
                run_id,
                provider_id: ProviderId::new(),
                model_id: ModelId::new(),
                provider_name: "test".to_owned(),
                model_name: "test".to_owned(),
                provider_config_updated_at: now,
                created_at: now,
            },
        })
    }
}

/// Poll `get_run` with short sleeps until the run reaches a terminal status.
///
/// The step is 10ms (well under the 50ms ceiling) and the loop is bounded so a
/// regression surfaces as a failed assertion rather than a hung test.
async fn wait_terminal(runtime: &PorticoRuntimeHandle, run_id: AgentRunId) -> AgentRun {
    for _ in 0..100 {
        let run = runtime.get_run(run_id).await.expect("get run");
        if run.status.is_terminal() {
            return run;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("run {run_id:?} did not reach a terminal status in time");
}

async fn build_runtime(
    executor: Option<Arc<dyn AgentExecutor>>,
) -> (PorticoRuntimeHandle, Arc<AtomicUsize>) {
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
    let event_bus = Arc::new(MemoryEventBus::default());
    let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
    let resolve_count = Arc::new(AtomicUsize::new(0));
    let resolver: Arc<dyn AgentExecutorResolver> = Arc::new(CountingResolver {
        resolves: resolve_count.clone(),
        executor: Arc::new(FastExecutor),
    });
    let runtime = PorticoRuntimeHandle::new(storage, event_bus, registry, executor, None)
        .await
        .expect("create runtime")
        .with_executor_resolver(resolver);
    (runtime, resolve_count)
}

/// The executor resolver must be invoked exactly once per submitted run, and
/// every run it drives must reach `Completed`.
#[tokio::test]
async fn resolver_runs_once_per_run_and_both_runs_complete() {
    ensure_test_workspace_root();
    let (runtime, resolves) = build_runtime(None).await;

    let workspace = runtime
        .create_workspace("test", "/tmp/test", false)
        .await
        .expect("create workspace");
    let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");

    let run_one = runtime.start_run(workspace.id, thread.id).await.expect("start run one");
    runtime.submit_message(run_one.id, "hello one").await.expect("submit run one");

    let run_two = runtime.start_run(workspace.id, thread.id).await.expect("start run two");
    runtime.submit_message(run_two.id, "hello two").await.expect("submit run two");

    let done_one = wait_terminal(&runtime, run_one.id).await;
    let done_two = wait_terminal(&runtime, run_two.id).await;

    assert_eq!(done_one.status, AgentRunStatus::Completed);
    assert_eq!(done_two.status, AgentRunStatus::Completed);
    assert!(
        runtime
            .registry()
            .get_run_model_snapshot(run_one.id)
            .await
            .expect("snapshot one")
            .is_some()
    );
    assert!(
        runtime
            .registry()
            .get_run_model_snapshot(run_two.id)
            .await
            .expect("snapshot two")
            .is_some()
    );
    assert_eq!(
        resolves.load(Ordering::SeqCst),
        2,
        "resolver must resolve exactly once per run"
    );
}

/// With neither an executor nor a resolver configured, submitting a message
/// must surface a provider-unavailable error and leave the run `Failed`.
#[tokio::test]
async fn run_without_executor_ends_failed() {
    ensure_test_workspace_root();
    let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
    let event_bus = Arc::new(MemoryEventBus::default());
    let registry = Arc::new(SqliteModelProviderRegistry::new(storage.pool().clone()));
    let runtime = PorticoRuntimeHandle::new(storage, event_bus, registry, None, None)
        .await
        .expect("create runtime");

    let workspace = runtime
        .create_workspace("test", "/tmp/test", false)
        .await
        .expect("create workspace");
    let thread = runtime.create_thread(workspace.id, "thread").await.expect("create thread");
    let run = runtime.start_run(workspace.id, thread.id).await.expect("start run");

    let err = runtime
        .submit_message(run.id, "hello")
        .await
        .expect_err("submit without executor must fail");
    assert!(
        err.to_string().contains("PROVIDER_UNAVAILABLE"),
        "unexpected error: {err}"
    );

    let finished = wait_terminal(&runtime, run.id).await;
    assert_eq!(finished.status, AgentRunStatus::Failed);
}

/// `recover_incomplete_runs` must mark `Queued`, `Running`, and `Paused` runs
/// as `Interrupted` while leaving `WaitingApproval` untouched.
#[tokio::test]
async fn recover_incomplete_runs_interrupts_active_states_but_keeps_waiting_approval() {
    let storage = SqliteStorage::open_in_memory().await.expect("open db");

    let workspace = storage
        .create_workspace("test", "/tmp/test", false)
        .await
        .expect("create workspace");
    let thread = storage.create_thread(workspace.id, "thread").await.expect("create thread");

    // `create_run` persists the run in `Queued`.
    let queued = storage.create_run(workspace.id, thread.id).await.expect("queued run");

    let running = storage.create_run(workspace.id, thread.id).await.expect("running run");
    storage
        .update_run_status(running.id, AgentRunStatus::Running)
        .await
        .expect("set running");

    let paused = storage.create_run(workspace.id, thread.id).await.expect("paused run");
    storage
        .update_run_status(paused.id, AgentRunStatus::Paused)
        .await
        .expect("set paused");

    let waiting = storage.create_run(workspace.id, thread.id).await.expect("waiting run");
    storage
        .update_run_status(waiting.id, AgentRunStatus::WaitingApproval)
        .await
        .expect("set waiting approval");

    let affected = storage.recover_incomplete_runs().await.expect("recover");
    assert_eq!(affected, 3, "exactly the three active runs should change");

    assert_eq!(
        storage.get_run(queued.id).await.expect("get queued").status,
        AgentRunStatus::Interrupted
    );
    assert_eq!(
        storage.get_run(running.id).await.expect("get running").status,
        AgentRunStatus::Interrupted
    );
    assert_eq!(
        storage.get_run(paused.id).await.expect("get paused").status,
        AgentRunStatus::Interrupted
    );
    assert_eq!(
        storage.get_run(waiting.id).await.expect("get waiting").status,
        AgentRunStatus::WaitingApproval,
        "WaitingApproval runs must be preserved across recovery"
    );
}
