CREATE TABLE IF NOT EXISTS automations (
    id BLOB PRIMARY KEY NOT NULL,
    workspace_id BLOB NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    trigger TEXT NOT NULL,
    cron_expr TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    permission_policy TEXT NOT NULL DEFAULT '{}',
    next_run_at DATETIME,
    last_run_at DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_automations_workspace_id ON automations(workspace_id);
CREATE INDEX IF NOT EXISTS idx_automations_next_run_at ON automations(next_run_at);
