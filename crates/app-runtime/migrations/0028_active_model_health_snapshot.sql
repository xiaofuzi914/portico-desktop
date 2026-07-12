CREATE TABLE IF NOT EXISTS active_model_selections (
    scope_key TEXT PRIMARY KEY NOT NULL,
    scope_type TEXT NOT NULL CHECK (scope_type IN ('Global', 'Workspace', 'Thread')),
    workspace_id BLOB,
    thread_id BLOB,
    provider_id BLOB NOT NULL,
    model_id BLOB NOT NULL,
    updated_at DATETIME NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE,
    FOREIGN KEY (provider_id) REFERENCES provider_configs(id) ON DELETE CASCADE,
    FOREIGN KEY (model_id) REFERENCES model_infos(id) ON DELETE CASCADE,
    CHECK (
        (scope_type = 'Global' AND workspace_id IS NULL AND thread_id IS NULL) OR
        (scope_type = 'Workspace' AND workspace_id IS NOT NULL AND thread_id IS NULL) OR
        (scope_type = 'Thread' AND workspace_id IS NOT NULL AND thread_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_active_model_selections_workspace
    ON active_model_selections(workspace_id);
CREATE INDEX IF NOT EXISTS idx_active_model_selections_thread
    ON active_model_selections(thread_id);

CREATE TABLE IF NOT EXISTS provider_health (
    provider_id BLOB NOT NULL,
    model_id BLOB NOT NULL,
    status TEXT NOT NULL,
    error_code TEXT,
    message TEXT,
    checked_at DATETIME NOT NULL,
    PRIMARY KEY (provider_id, model_id),
    FOREIGN KEY (provider_id) REFERENCES provider_configs(id) ON DELETE CASCADE,
    FOREIGN KEY (model_id) REFERENCES model_infos(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS run_model_snapshots (
    run_id BLOB PRIMARY KEY NOT NULL,
    provider_id BLOB NOT NULL,
    model_id BLOB NOT NULL,
    provider_name TEXT NOT NULL,
    model_name TEXT NOT NULL,
    provider_config_updated_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);
