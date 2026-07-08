CREATE TABLE IF NOT EXISTS subagents (
    id BLOB PRIMARY KEY NOT NULL,
    parent_run_id BLOB NOT NULL,
    agent_name TEXT NOT NULL,
    status TEXT NOT NULL,
    task_description TEXT NOT NULL,
    output_summary TEXT,
    created_at DATETIME NOT NULL,
    completed_at DATETIME,
    child_run_id BLOB,
    FOREIGN KEY (parent_run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_subagents_parent_run_id ON subagents(parent_run_id);
