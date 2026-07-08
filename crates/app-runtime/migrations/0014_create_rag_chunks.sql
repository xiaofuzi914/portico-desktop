CREATE TABLE IF NOT EXISTS rag_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id BLOB NOT NULL,
    document_path TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rag_chunks_workspace ON rag_chunks(workspace_id);
CREATE INDEX IF NOT EXISTS idx_rag_chunks_document ON rag_chunks(workspace_id, document_path);
