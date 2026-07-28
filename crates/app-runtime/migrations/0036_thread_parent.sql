-- Session branching: child sessions keep a link to the parent conversation.
ALTER TABLE threads ADD COLUMN parent_thread_id BLOB REFERENCES threads(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_threads_parent_thread_id ON threads(parent_thread_id);
