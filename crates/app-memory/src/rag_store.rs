//! Persistent `SQLite` store for RAG chunks and embeddings.

use app_models::{AppError, WorkspaceId};
use sqlx::SqlitePool;
use tracing::warn;

use crate::rag::StoredChunk;

/// SQLite-backed persistence for RAG chunks.
#[derive(Debug, Clone)]
pub struct SqliteRagStore {
    pool: SqlitePool,
}

impl SqliteRagStore {
    /// Create a new RAG store backed by the given connection pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert a batch of chunks for a document.
    ///
    /// # Errors
    ///
    /// Returns an error if the insert fails.
    pub async fn insert_chunks(
        &self,
        workspace_id: WorkspaceId,
        document_path: &str,
        provider_id: &str,
        dimension: usize,
        chunks: Vec<StoredChunk>,
    ) -> Result<(), AppError> {
        if chunks.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await.map_err(|e| AppError::Internal {
            message: format!("rag insert_chunks transaction failed: {e}"),
        })?;

        let dimension_i64 = i64::try_from(dimension).unwrap_or(0);
        let embedding_bytes = |embedding: &[f32]| {
            embedding
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<u8>>()
        };

        for chunk in chunks {
            sqlx::query(
                "INSERT INTO rag_chunks (workspace_id, document_path, chunk_index, content, embedding, embedding_provider_id, dimension, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(workspace_id.0)
            .bind(document_path)
            .bind(i64::try_from(chunk.chunk_index).unwrap_or(0))
            .bind(&chunk.content)
            .bind(embedding_bytes(&chunk.embedding))
            .bind(provider_id)
            .bind(dimension_i64)
            .bind(chrono::Utc::now())
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("rag insert_chunks failed: {e}"),
            })?;
        }

        tx.commit().await.map_err(|e| AppError::Internal {
            message: format!("rag insert_chunks commit failed: {e}"),
        })
    }

    /// Load chunks for a workspace that match the given provider and dimension.
    ///
    /// If mismatched chunks are found, a warning is logged once per workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn load_matching_chunks(
        &self,
        workspace_id: WorkspaceId,
        provider_id: &str,
        dimension: usize,
    ) -> Result<Vec<StoredChunk>, AppError> {
        let rows = sqlx::query_as::<_, RagChunkRow>(
            "SELECT id, document_path, chunk_index, content, embedding, embedding_provider_id, dimension FROM rag_chunks WHERE workspace_id = ? ORDER BY id ASC",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("rag load_matching_chunks failed: {e}"),
        })?;

        let mut matching = Vec::with_capacity(rows.len());
        let mut mismatched = false;

        for row in rows {
            let row_dimension = usize::try_from(row.dimension).unwrap_or(0);
            let row_provider = row.embedding_provider_id.as_deref().unwrap_or("");

            if row_provider != provider_id || row_dimension != dimension {
                mismatched = true;
                continue;
            }

            let embedding = row.embedding.map_or_else(Vec::new, |bytes| blob_to_f32(&bytes));
            matching.push(StoredChunk {
                id: row.id,
                document_path: row.document_path,
                chunk_index: usize::try_from(row.chunk_index).unwrap_or(0),
                content: row.content,
                workspace_id,
                embedding,
            });
        }

        if mismatched {
            warn!(
                workspace_id = ?workspace_id,
                provider_id,
                dimension,
                "rag chunks with mismatched provider/dimension found; run rebuild_rag_index to re-index"
            );
        }

        Ok(matching)
    }

    /// Load all chunk contents for a workspace, regardless of provider.
    ///
    /// Used by rebuild to re-embed existing content.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn load_all_contents(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<(String, usize, String)>, AppError> {
        let rows = sqlx::query_as::<_, RagContentRow>(
            "SELECT document_path, chunk_index, content FROM rag_chunks WHERE workspace_id = ? ORDER BY id ASC",
        )
        .bind(workspace_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("rag load_all_contents failed: {e}"),
        })?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.document_path,
                    usize::try_from(row.chunk_index).unwrap_or(0),
                    row.content,
                )
            })
            .collect())
    }

    /// Delete all chunks for a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if the delete fails.
    pub async fn clear_workspace(&self, workspace_id: WorkspaceId) -> Result<(), AppError> {
        sqlx::query("DELETE FROM rag_chunks WHERE workspace_id = ?")
            .bind(workspace_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("rag clear_workspace failed: {e}"),
            })?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct RagChunkRow {
    id: i64,
    document_path: String,
    chunk_index: i64,
    content: String,
    embedding: Option<Vec<u8>>,
    embedding_provider_id: Option<String>,
    dimension: i64,
}

#[derive(sqlx::FromRow)]
struct RagContentRow {
    document_path: String,
    chunk_index: i64,
    content: String,
}

/// Convert a little-endian byte blob to a vector of `f32`.
fn blob_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().unwrap_or([0; 4]);
            f32::from_le_bytes(arr)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn in_memory_store() -> SqliteRagStore {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE rag_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id BLOB NOT NULL,
                document_path TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                embedding_provider_id TEXT,
                dimension INTEGER,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        SqliteRagStore::new(pool)
    }

    #[tokio::test]
    async fn insert_and_load_matching_chunks() {
        let store = in_memory_store().await;
        let workspace_id = WorkspaceId::new();

        let chunks = vec![StoredChunk {
            id: 0,
            document_path: "doc.md".to_owned(),
            chunk_index: 0,
            content: "hello".to_owned(),
            workspace_id,
            embedding: vec![1.0, 2.0, 3.0],
        }];

        store
            .insert_chunks(workspace_id, "doc.md", "test-provider", 3, chunks)
            .await
            .unwrap();

        let loaded = store
            .load_matching_chunks(workspace_id, "test-provider", 3)
            .await
            .unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "hello");
        assert_eq!(loaded[0].embedding, vec![1.0, 2.0, 3.0]);
    }

    #[tokio::test]
    async fn blob_roundtrip_is_little_endian() {
        let store = in_memory_store().await;
        let workspace_id = WorkspaceId::new();

        let embedding: Vec<f32> = vec![1.5, -2.5, 0.0];
        let chunks = vec![StoredChunk {
            id: 0,
            document_path: "doc.md".to_owned(),
            chunk_index: 0,
            content: "data".to_owned(),
            workspace_id,
            embedding: embedding.clone(),
        }];

        store
            .insert_chunks(workspace_id, "doc.md", "p", 3, chunks)
            .await
            .unwrap();

        let loaded = store.load_matching_chunks(workspace_id, "p", 3).await.unwrap();
        assert_eq!(loaded[0].embedding, embedding);
    }

    #[tokio::test]
    async fn dimension_filtering_excludes_mismatched_chunks() {
        let store = in_memory_store().await;
        let workspace_id = WorkspaceId::new();

        let chunks = vec![StoredChunk {
            id: 0,
            document_path: "doc.md".to_owned(),
            chunk_index: 0,
            content: "old".to_owned(),
            workspace_id,
            embedding: vec![1.0, 2.0],
        }];

        store
            .insert_chunks(workspace_id, "doc.md", "old-provider", 2, chunks)
            .await
            .unwrap();

        let loaded = store
            .load_matching_chunks(workspace_id, "new-provider", 3)
            .await
            .unwrap();
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn clear_workspace_removes_chunks() {
        let store = in_memory_store().await;
        let workspace_id = WorkspaceId::new();

        let chunks = vec![StoredChunk {
            id: 0,
            document_path: "doc.md".to_owned(),
            chunk_index: 0,
            content: "x".to_owned(),
            workspace_id,
            embedding: vec![1.0],
        }];

        store
            .insert_chunks(workspace_id, "doc.md", "p", 1, chunks)
            .await
            .unwrap();
        store.clear_workspace(workspace_id).await.unwrap();

        let loaded = store.load_matching_chunks(workspace_id, "p", 1).await.unwrap();
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn load_all_contents_ignores_provider() {
        let store = in_memory_store().await;
        let workspace_id = WorkspaceId::new();

        let chunks = vec![StoredChunk {
            id: 0,
            document_path: "doc.md".to_owned(),
            chunk_index: 0,
            content: "keep me".to_owned(),
            workspace_id,
            embedding: vec![1.0],
        }];

        store
            .insert_chunks(workspace_id, "doc.md", "p", 1, chunks)
            .await
            .unwrap();

        let contents = store.load_all_contents(workspace_id).await.unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].0, "doc.md");
        assert_eq!(contents[0].1, 0);
        assert_eq!(contents[0].2, "keep me");
    }
}
