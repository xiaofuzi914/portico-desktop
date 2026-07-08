CREATE TABLE IF NOT EXISTS memories (
    id BLOB PRIMARY KEY NOT NULL,
    scope TEXT NOT NULL,
    workspace_id BLOB,
    thread_id BLOB,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    sensitive INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memories_scope_workspace ON memories(scope, workspace_id);
CREATE INDEX IF NOT EXISTS idx_memories_scope_thread ON memories(scope, thread_id);
