CREATE TABLE IF NOT EXISTS workspace_paths (
    workspace_id BLOB NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('read', 'write')),
    path TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (workspace_id, kind, path),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_workspace_paths_workspace_id ON workspace_paths(workspace_id);
