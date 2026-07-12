CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_run_assistant
    ON messages(run_id, role)
    WHERE role = 'Assistant';
