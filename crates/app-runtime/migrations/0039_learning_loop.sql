-- Local intelligent learning loop: experience events, memory candidates,
-- run feedback, RAG document metadata, and pattern/memory scoring fields.

CREATE TABLE IF NOT EXISTS experience_events (
    id BLOB PRIMARY KEY NOT NULL,
    run_id BLOB NOT NULL,
    workspace_id BLOB NOT NULL,
    thread_id BLOB NOT NULL,
    task_kind TEXT NOT NULL,
    execution_mode TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    outcome TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_experience_event_run_version
    ON experience_events(run_id, schema_version);

CREATE INDEX IF NOT EXISTS idx_experience_events_workspace
    ON experience_events(workspace_id, created_at DESC);

CREATE TABLE IF NOT EXISTS memory_candidates (
    id BLOB PRIMARY KEY NOT NULL,
    run_id BLOB NOT NULL,
    workspace_id BLOB,
    thread_id BLOB,
    scope TEXT NOT NULL,
    kind TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    confidence REAL NOT NULL,
    sensitive INTEGER NOT NULL DEFAULT 0,
    evidence_json TEXT NOT NULL,
    status TEXT NOT NULL,
    extractor_version INTEGER NOT NULL,
    created_at DATETIME NOT NULL,
    reviewed_at DATETIME,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_candidate_fingerprint
    ON memory_candidates(scope, fingerprint, status);

CREATE INDEX IF NOT EXISTS idx_memory_candidates_status
    ON memory_candidates(status, created_at DESC);

CREATE TABLE IF NOT EXISTS run_feedback (
    run_id BLOB PRIMARY KEY NOT NULL,
    rating TEXT NOT NULL,
    comment TEXT,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS rag_documents (
    workspace_id BLOB NOT NULL,
    document_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    modified_at DATETIME,
    embedding_provider_id TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    status TEXT NOT NULL,
    indexed_at DATETIME,
    last_error TEXT,
    PRIMARY KEY(workspace_id, document_path)
);

CREATE TABLE IF NOT EXISTS run_context_snapshots (
    run_id BLOB PRIMARY KEY NOT NULL,
    memory_ids_json TEXT NOT NULL DEFAULT '[]',
    pattern_ids_json TEXT NOT NULL DEFAULT '[]',
    behavior_policy_json TEXT NOT NULL DEFAULT '{}',
    outbound_manifest_json TEXT,
    recall_scores_json TEXT NOT NULL DEFAULT '[]',
    created_at DATETIME NOT NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
);

-- Pattern lifecycle scoring fields (fingerprint-based dedup + evidence).
ALTER TABLE workflow_patterns ADD COLUMN fingerprint TEXT;
ALTER TABLE workflow_patterns ADD COLUMN evidence_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE workflow_patterns ADD COLUMN confidence REAL NOT NULL DEFAULT 0.0;
ALTER TABLE workflow_patterns ADD COLUMN last_success_at DATETIME;
ALTER TABLE workflow_patterns ADD COLUMN last_failure_at DATETIME;
ALTER TABLE workflow_patterns ADD COLUMN tool_strategy TEXT NOT NULL DEFAULT '';
ALTER TABLE workflow_patterns ADD COLUMN output_kind TEXT NOT NULL DEFAULT '';
ALTER TABLE workflow_patterns ADD COLUMN task_kind TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_workflow_patterns_fingerprint
    ON workflow_patterns(fingerprint);

-- Memory provenance and usage attribution.
ALTER TABLE memories ADD COLUMN kind TEXT;
ALTER TABLE memories ADD COLUMN encrypted_value BLOB;
ALTER TABLE memories ADD COLUMN encryption_version INTEGER;
ALTER TABLE memories ADD COLUMN source_run_id BLOB;
ALTER TABLE memories ADD COLUMN confidence REAL;
ALTER TABLE memories ADD COLUMN last_used_at DATETIME;
ALTER TABLE memories ADD COLUMN use_count INTEGER NOT NULL DEFAULT 0;
