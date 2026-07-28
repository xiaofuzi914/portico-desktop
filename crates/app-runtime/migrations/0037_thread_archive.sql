-- Soft-delete / archive for sessions. Active list uses archived_at IS NULL.
-- Rows with archived_at older than 30 days are purged by the runtime.
ALTER TABLE threads ADD COLUMN archived_at DATETIME;

CREATE INDEX IF NOT EXISTS idx_threads_workspace_active
    ON threads (workspace_id, created_at)
    WHERE archived_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_threads_archived_at
    ON threads (archived_at)
    WHERE archived_at IS NOT NULL;
