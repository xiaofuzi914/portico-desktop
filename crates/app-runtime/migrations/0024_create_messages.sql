CREATE TABLE IF NOT EXISTS messages (
    id BLOB PRIMARY KEY NOT NULL,
    thread_id BLOB NOT NULL,
    run_id BLOB,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    client_request_id TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_thread_created_at
    ON messages(thread_id, created_at, id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_thread_client_request
    ON messages(thread_id, client_request_id)
    WHERE client_request_id IS NOT NULL;

CREATE TRIGGER IF NOT EXISTS trg_messages_touch_thread
AFTER INSERT ON messages
BEGIN
    UPDATE threads SET updated_at = NEW.created_at WHERE id = NEW.thread_id;
END;

CREATE UNIQUE INDEX IF NOT EXISTS idx_run_events_run_sequence_unique
    ON run_events(run_id, sequence);
