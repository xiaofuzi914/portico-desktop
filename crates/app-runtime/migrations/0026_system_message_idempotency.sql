CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_run_system
    ON messages(run_id, role)
    WHERE role = 'System';
