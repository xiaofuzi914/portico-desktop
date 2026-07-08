CREATE TABLE IF NOT EXISTS model_infos (
    id BLOB PRIMARY KEY,
    provider_id BLOB NOT NULL,
    provider_name TEXT NOT NULL,
    model_name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    capabilities TEXT NOT NULL DEFAULT '{}',
    FOREIGN KEY (provider_id) REFERENCES provider_configs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_model_infos_provider_id ON model_infos(provider_id);
