CREATE TABLE IF NOT EXISTS background_tasks (
    id BLOB PRIMARY KEY NOT NULL,
    workspace_id BLOB NOT NULL,
    thread_id BLOB,
    run_id BLOB,
    task_kind TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'Queued',
    priority INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    scheduled_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    leased_at DATETIME,
    leased_by TEXT,
    next_retry_at DATETIME,
    error_message TEXT,
    result_summary TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE SET NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_background_tasks_workspace_id ON background_tasks(workspace_id);
CREATE INDEX IF NOT EXISTS idx_background_tasks_status ON background_tasks(status);
CREATE INDEX IF NOT EXISTS idx_background_tasks_scheduled_at ON background_tasks(scheduled_at);
CREATE INDEX IF NOT EXISTS idx_background_tasks_priority ON background_tasks(priority);
CREATE INDEX IF NOT EXISTS idx_background_tasks_status_scheduled ON background_tasks(status, scheduled_at, priority);
