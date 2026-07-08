CREATE TABLE IF NOT EXISTS artifacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id BLOB NOT NULL,
    name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    path TEXT,
    content_preview TEXT,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_artifacts_run_id ON artifacts(run_id);
