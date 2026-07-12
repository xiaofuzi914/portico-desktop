CREATE TABLE IF NOT EXISTS agent_checkpoints (
    run_id BLOB PRIMARY KEY NOT NULL,
    messages_json TEXT NOT NULL,
    step INTEGER NOT NULL DEFAULT 0,
    updated_at DATETIME NOT NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);
