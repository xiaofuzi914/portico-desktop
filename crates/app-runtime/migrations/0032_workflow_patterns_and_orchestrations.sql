-- Workflow patterns: user/workspace habits owned by the memory layer.
CREATE TABLE IF NOT EXISTS workflow_patterns (
    id BLOB PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL,
    workspace_id BLOB,
    name TEXT NOT NULL,
    summary TEXT NOT NULL,
    trigger_text TEXT NOT NULL DEFAULT '',
    preferred_roles_json TEXT NOT NULL DEFAULT '[]',
    collaboration_style TEXT NOT NULL DEFAULT '',
    strength REAL NOT NULL DEFAULT 1.0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    last_used_at DATETIME,
    status TEXT NOT NULL DEFAULT 'active',
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workflow_patterns_scope
    ON workflow_patterns (scope, workspace_id, status);

-- Multi-agent orchestration sessions (workflow layer).
CREATE TABLE IF NOT EXISTS orchestrations (
    id BLOB PRIMARY KEY NOT NULL,
    parent_run_id BLOB NOT NULL,
    workspace_id BLOB NOT NULL,
    thread_id BLOB NOT NULL,
    task TEXT NOT NULL,
    status TEXT NOT NULL,
    plan_json TEXT NOT NULL,
    pattern_ids_json TEXT NOT NULL DEFAULT '[]',
    result_summary TEXT,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    completed_at DATETIME,
    FOREIGN KEY (parent_run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_orchestrations_thread
    ON orchestrations (thread_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_orchestrations_parent_run
    ON orchestrations (parent_run_id);
