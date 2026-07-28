-- Editable multi-stage workflow templates (bundled seeds + user DAG edits).
CREATE TABLE IF NOT EXISTS workflow_templates (
    id TEXT PRIMARY KEY NOT NULL,
    catalog_key TEXT,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    stages_json TEXT NOT NULL,
    builtin INTEGER NOT NULL DEFAULT 0,
    workspace_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_workflow_templates_catalog_key
    ON workflow_templates(catalog_key)
    WHERE catalog_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_workflow_templates_workspace
    ON workflow_templates(workspace_id);
