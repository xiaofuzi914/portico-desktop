CREATE TABLE IF NOT EXISTS threads (
    id BLOB PRIMARY KEY NOT NULL,
    workspace_id BLOB NOT NULL,
    title TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_threads_workspace_id ON threads(workspace_id);
