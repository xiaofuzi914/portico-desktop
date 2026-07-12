CREATE TABLE IF NOT EXISTS tool_invocations (
    id BLOB PRIMARY KEY NOT NULL,
    run_id BLOB NOT NULL,
    thread_id BLOB NOT NULL,
    workspace_id BLOB NOT NULL,
    model_call_id TEXT,
    tool_name TEXT NOT NULL,
    tool_version TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    context_revision DATETIME NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        'Ready', 'WaitingApproval', 'Approved', 'Executing', 'Succeeded',
        'Failed', 'Denied', 'Cancelled', 'NeedsReconciliation'
    )),
    approval_request_id INTEGER,
    result_json TEXT,
    error TEXT,
    recovery_json TEXT,
    lease_token BLOB,
    attempts INTEGER NOT NULL DEFAULT 0,
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    correlation_id BLOB NOT NULL,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    started_at DATETIME,
    completed_at DATETIME,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (approval_request_id) REFERENCES approval_requests(id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_tool_invocations_run_model_call
    ON tool_invocations(run_id, model_call_id)
    WHERE model_call_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_tool_invocations_approval
    ON tool_invocations(approval_request_id)
    WHERE approval_request_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tool_invocations_run
    ON tool_invocations(run_id, created_at);
CREATE INDEX IF NOT EXISTS idx_tool_invocations_status
    ON tool_invocations(status, updated_at);
