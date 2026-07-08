CREATE TABLE IF NOT EXISTS provider_configs (
    id BLOB PRIMARY KEY,
    kind TEXT NOT NULL,
    display_name TEXT NOT NULL,
    base_url TEXT,
    api_key_reference TEXT NOT NULL,
    organization_id TEXT,
    project_id TEXT,
    default_headers TEXT NOT NULL DEFAULT '{}',
    timeout_ms INTEGER NOT NULL DEFAULT 30000,
    retry_policy TEXT NOT NULL DEFAULT '{}',
    fallback_provider_ids TEXT NOT NULL DEFAULT '[]',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
