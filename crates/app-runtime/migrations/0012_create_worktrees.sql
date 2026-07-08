CREATE TABLE IF NOT EXISTS worktrees (
    id BLOB PRIMARY KEY NOT NULL,
    workspace_id BLOB NOT NULL,
    thread_id BLOB NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_worktrees_workspace_id ON worktrees(workspace_id);
CREATE INDEX IF NOT EXISTS idx_worktrees_thread_id ON worktrees(thread_id);
