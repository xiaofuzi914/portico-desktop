//! Memory manager trait and SQLite-backed implementation.

use crate::cipher::{
    ENCRYPTED_VALUE_PLACEHOLDER, MEMORY_ENCRYPTION_VERSION, MemoryCipher,
};
use app_models::{
    AgentRunId, AppError, MemoryId, MemoryItem, MemoryKind, MemoryScope, ThreadId, WorkspaceId,
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::{Arc, RwLock};

/// Persistence operations for agent memories.
#[async_trait]
pub trait MemoryManager: Send + Sync {
    /// Create a new memory item.
    async fn create_memory(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        key: &str,
        value: &str,
        sensitive: bool,
    ) -> Result<MemoryItem, AppError>;

    /// Create a memory with learning metadata (kind, source run, confidence).
    #[allow(clippy::too_many_arguments)]
    async fn create_memory_with_meta(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        key: &str,
        value: &str,
        sensitive: bool,
        kind: Option<MemoryKind>,
        source_run_id: Option<AgentRunId>,
        confidence: Option<f64>,
    ) -> Result<MemoryItem, AppError>;

    /// List memories matching the given scope filters.
    async fn list_memories(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
    ) -> Result<Vec<MemoryItem>, AppError>;

    /// List all non-session memories relevant to a workspace/thread for recall.
    async fn list_for_recall(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<Vec<MemoryItem>, AppError>;

    /// Update the value of an existing memory.
    async fn update_memory(&self, id: MemoryId, value: &str) -> Result<MemoryItem, AppError>;

    /// Record that a memory was injected into a prompt.
    async fn record_memory_use(&self, id: MemoryId) -> Result<(), AppError>;

    /// Delete a memory by id.
    async fn delete_memory(&self, id: MemoryId) -> Result<(), AppError>;
}

/// SQLite-backed [`MemoryManager`] implementation.
#[derive(Clone)]
pub struct SqliteMemoryManager {
    pool: SqlitePool,
    cipher: Arc<RwLock<Option<Arc<dyn MemoryCipher>>>>,
}

impl std::fmt::Debug for SqliteMemoryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteMemoryManager")
            .field("cipher", &self.cipher.read().ok().map(|g| g.is_some()))
            .finish_non_exhaustive()
    }
}

impl SqliteMemoryManager {
    /// Create a new memory manager backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            cipher: Arc::new(RwLock::new(None)),
        }
    }

    /// Inject a cipher for sensitive memory encryption (composition root).
    pub fn set_cipher(&self, cipher: Arc<dyn MemoryCipher>) {
        if let Ok(mut guard) = self.cipher.write() {
            *guard = Some(cipher);
        }
    }

    fn cipher(&self) -> Option<Arc<dyn MemoryCipher>> {
        self.cipher.read().ok().and_then(|g| g.clone())
    }

    fn encrypt_if_needed(
        &self,
        value: &str,
        sensitive: bool,
    ) -> Result<(String, Option<Vec<u8>>, Option<i64>), AppError> {
        if !sensitive {
            return Ok((value.to_owned(), None, None));
        }
        let Some(cipher) = self.cipher() else {
            // Fail open for write only when no cipher is configured (tests / early boot).
            return Ok((value.to_owned(), None, None));
        };
        let blob = cipher.encrypt(value.as_bytes())?;
        Ok((
            ENCRYPTED_VALUE_PLACEHOLDER.to_owned(),
            Some(blob),
            Some(MEMORY_ENCRYPTION_VERSION),
        ))
    }

    fn decrypt_row_value(
        &self,
        value: String,
        sensitive: bool,
        encrypted_value: Option<Vec<u8>>,
    ) -> Result<String, AppError> {
        if !sensitive {
            return Ok(value);
        }
        if let Some(blob) = encrypted_value {
            let Some(cipher) = self.cipher() else {
                // Fail closed for prompt use: return empty so it cannot leak.
                return Ok(String::new());
            };
            let plain = cipher.decrypt(&blob)?;
            return String::from_utf8(plain).map_err(|e| AppError::Internal {
                message: format!("sensitive memory is not valid UTF-8: {e}"),
            });
        }
        // Legacy plaintext sensitive row — still return value, migration can re-encrypt later.
        Ok(value)
    }

    /// Re-encrypt legacy plaintext sensitive rows (best-effort background migration).
    pub async fn migrate_sensitive_plaintext(&self) -> Result<u64, AppError> {
        let Some(cipher) = self.cipher() else {
            return Ok(0);
        };
        let rows = sqlx::query_as::<_, LegacySensitiveRow>(
            r"
            SELECT id, value FROM memories
            WHERE sensitive = 1
              AND (encrypted_value IS NULL OR length(encrypted_value) = 0)
              AND value IS NOT NULL AND length(value) > 0
            ",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("list legacy sensitive memories failed: {e}"),
        })?;

        let mut migrated = 0u64;
        for row in rows {
            let blob = match cipher.encrypt(row.value.as_bytes()) {
                Ok(b) => b,
                Err(err) => {
                    tracing::warn!(id = %row.id, error = %err, "skip sensitive memory migration");
                    continue;
                }
            };
            let result = sqlx::query(
                r"
                UPDATE memories
                SET encrypted_value = ?, encryption_version = ?, value = ?
                WHERE id = ?
                ",
            )
            .bind(blob)
            .bind(MEMORY_ENCRYPTION_VERSION)
            .bind(ENCRYPTED_VALUE_PLACEHOLDER)
            .bind(row.id)
            .execute(&self.pool)
            .await;
            if result.is_ok() {
                migrated += 1;
            }
        }
        Ok(migrated)
    }
}

#[async_trait]
impl MemoryManager for SqliteMemoryManager {
    async fn create_memory(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        key: &str,
        value: &str,
        sensitive: bool,
    ) -> Result<MemoryItem, AppError> {
        self.create_memory_with_meta(
            scope,
            workspace_id,
            thread_id,
            key,
            value,
            sensitive,
            None,
            None,
            None,
        )
        .await
    }

    async fn create_memory_with_meta(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        key: &str,
        value: &str,
        sensitive: bool,
        kind: Option<MemoryKind>,
        source_run_id: Option<AgentRunId>,
        confidence: Option<f64>,
    ) -> Result<MemoryItem, AppError> {
        let id = MemoryId::new();
        let now = Utc::now();
        let sensitive_i64 = i64::from(sensitive);
        let (stored_value, encrypted, enc_ver) = self.encrypt_if_needed(value, sensitive)?;

        sqlx::query(
            r"
            INSERT INTO memories (
                id, scope, workspace_id, thread_id, key, value, sensitive,
                created_at, updated_at, kind, source_run_id, confidence, use_count,
                encrypted_value, encryption_version
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)
            ",
        )
        .bind(id.0)
        .bind(scope.as_str())
        .bind(workspace_id.map(|wid| wid.0))
        .bind(thread_id.map(|tid| tid.0))
        .bind(key)
        .bind(&stored_value)
        .bind(sensitive_i64)
        .bind(now)
        .bind(now)
        .bind(kind.map(|k| k.as_str().to_owned()))
        .bind(source_run_id.map(|r| r.0))
        .bind(confidence)
        .bind(encrypted)
        .bind(enc_ver)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("create_memory failed: {e}"),
        })?;

        Ok(MemoryItem {
            id,
            scope,
            workspace_id,
            thread_id,
            key: key.to_owned(),
            // Return plaintext to the caller; DB holds ciphertext when cipher is set.
            value: value.to_owned(),
            sensitive,
            kind,
            source_run_id,
            confidence,
            last_used_at: None,
            use_count: 0,
            created_at: now,
            updated_at: now,
        })
    }

    async fn list_memories(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
    ) -> Result<Vec<MemoryItem>, AppError> {
        let mut sql = String::from(
            r"
            SELECT id, scope, workspace_id, thread_id, key, value, sensitive,
                   created_at, updated_at, kind, source_run_id, confidence,
                   last_used_at, use_count, encrypted_value
            FROM memories WHERE scope = ?
            ",
        );

        match scope {
            MemoryScope::Thread => {
                if thread_id.is_some() {
                    sql.push_str(" AND thread_id = ?");
                }
            }
            MemoryScope::Workspace => {
                if workspace_id.is_some() {
                    sql.push_str(" AND workspace_id = ?");
                }
            }
            MemoryScope::Session | MemoryScope::User => {}
        }
        sql.push_str(" ORDER BY updated_at DESC, id DESC");

        let mut query = sqlx::query_as::<_, MemoryRow>(&sql).bind(scope.as_str());
        match scope {
            MemoryScope::Thread => {
                if let Some(id) = thread_id {
                    query = query.bind(id.0);
                }
            }
            MemoryScope::Workspace => {
                if let Some(id) = workspace_id {
                    query = query.bind(id.0);
                }
            }
            _ => {}
        }

        let rows = query.fetch_all(&self.pool).await.map_err(|e| AppError::Internal {
            message: format!("list_memories failed: {e}"),
        })?;

        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            items.push(self.row_to_item(row)?);
        }
        Ok(items)
    }

    async fn list_for_recall(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
    ) -> Result<Vec<MemoryItem>, AppError> {
        let mut all = Vec::new();
        all.extend(self.list_memories(MemoryScope::User, None, None).await?);
        all.extend(
            self.list_memories(MemoryScope::Workspace, Some(workspace_id), None)
                .await?,
        );
        all.extend(
            self.list_memories(MemoryScope::Thread, Some(workspace_id), Some(thread_id))
                .await?,
        );
        Ok(all)
    }

    async fn update_memory(&self, id: MemoryId, value: &str) -> Result<MemoryItem, AppError> {
        let now = Utc::now();
        let existing = self.fetch_one_raw(id).await?;
        let sensitive = existing.sensitive != 0;
        let (stored_value, encrypted, enc_ver) = self.encrypt_if_needed(value, sensitive)?;

        let result = sqlx::query(
            r"
            UPDATE memories
            SET value = ?, encrypted_value = ?, encryption_version = ?, updated_at = ?
            WHERE id = ?
            ",
        )
        .bind(&stored_value)
        .bind(encrypted)
        .bind(enc_ver)
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("update_memory failed: {e}"),
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("memory {id:?}"),
            });
        }

        let mut item = self.fetch_one(id).await?;
        // Ensure caller sees the new plaintext.
        item.value = value.to_owned();
        Ok(item)
    }

    async fn record_memory_use(&self, id: MemoryId) -> Result<(), AppError> {
        let now = Utc::now();
        let _ = sqlx::query(
            r"
            UPDATE memories
            SET use_count = COALESCE(use_count, 0) + 1,
                last_used_at = ?,
                updated_at = updated_at
            WHERE id = ?
            ",
        )
        .bind(now)
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("record_memory_use failed: {e}"),
        })?;
        Ok(())
    }

    async fn delete_memory(&self, id: MemoryId) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("delete_memory failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("memory {id:?}"),
            });
        }
        Ok(())
    }
}

impl SqliteMemoryManager {
    fn row_to_item(&self, row: MemoryRow) -> Result<MemoryItem, AppError> {
        let sensitive = row.sensitive != 0;
        let value = self.decrypt_row_value(row.value, sensitive, row.encrypted_value)?;
        Ok(MemoryItem {
            id: MemoryId(row.id),
            scope: row.scope.as_str().try_into().unwrap_or(MemoryScope::Session),
            workspace_id: row.workspace_id.map(WorkspaceId),
            thread_id: row.thread_id.map(ThreadId),
            key: row.key,
            value,
            sensitive,
            kind: row.kind.as_deref().and_then(|k| MemoryKind::try_from(k).ok()),
            source_run_id: row.source_run_id.map(AgentRunId),
            confidence: row.confidence,
            last_used_at: row.last_used_at,
            use_count: row.use_count.unwrap_or(0),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    async fn fetch_one_raw(&self, id: MemoryId) -> Result<MemoryRow, AppError> {
        sqlx::query_as::<_, MemoryRow>(
            r"
            SELECT id, scope, workspace_id, thread_id, key, value, sensitive,
                   created_at, updated_at, kind, source_run_id, confidence,
                   last_used_at, use_count, encrypted_value
            FROM memories WHERE id = ?
            ",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("fetch memory failed: {e}"),
        })?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory {id:?}"),
        })
    }

    async fn fetch_one(&self, id: MemoryId) -> Result<MemoryItem, AppError> {
        let row = self.fetch_one_raw(id).await?;
        self.row_to_item(row)
    }
}

#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: uuid::Uuid,
    scope: String,
    workspace_id: Option<uuid::Uuid>,
    thread_id: Option<uuid::Uuid>,
    key: String,
    value: String,
    sensitive: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    kind: Option<String>,
    source_run_id: Option<uuid::Uuid>,
    confidence: Option<f64>,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    use_count: Option<i64>,
    encrypted_value: Option<Vec<u8>>,
}

#[derive(sqlx::FromRow)]
struct LegacySensitiveRow {
    id: uuid::Uuid,
    value: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cipher::NoopMemoryCipher;

    async fn in_memory_manager() -> SqliteMemoryManager {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(
            r"
            CREATE TABLE memories (
                id BLOB PRIMARY KEY NOT NULL,
                scope TEXT NOT NULL,
                workspace_id BLOB,
                thread_id BLOB,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                sensitive INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                kind TEXT,
                encrypted_value BLOB,
                encryption_version INTEGER,
                source_run_id BLOB,
                confidence REAL,
                last_used_at TEXT,
                use_count INTEGER NOT NULL DEFAULT 0
            )
            ",
        )
        .execute(&pool)
        .await
        .unwrap();
        let manager = SqliteMemoryManager::new(pool);
        manager.set_cipher(Arc::new(NoopMemoryCipher));
        manager
    }

    #[tokio::test]
    async fn create_and_list_memory() {
        let manager = in_memory_manager().await;
        let workspace_id = WorkspaceId::new();
        let thread_id = ThreadId::new();

        let created = manager
            .create_memory(
                MemoryScope::Thread,
                Some(workspace_id),
                Some(thread_id),
                "preference",
                "dark mode",
                false,
            )
            .await
            .unwrap();

        assert_eq!(created.key, "preference");
        assert_eq!(created.value, "dark mode");
        assert!(!created.sensitive);

        let memories = manager
            .list_memories(MemoryScope::Thread, Some(workspace_id), Some(thread_id))
            .await
            .unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].id, created.id);
    }

    #[tokio::test]
    async fn sensitive_uses_cipher_storage() {
        let manager = in_memory_manager().await;
        let created = manager
            .create_memory(MemoryScope::User, None, None, "token", "sk-secret", true)
            .await
            .unwrap();
        assert_eq!(created.value, "sk-secret");

        // DB value column should be placeholder when cipher is set.
        let row = sqlx::query_as::<_, (String, Option<Vec<u8>>)>(
            "SELECT value, encrypted_value FROM memories WHERE id = ?",
        )
        .bind(created.id.0)
        .fetch_one(&manager.pool)
        .await
        .unwrap();
        assert_eq!(row.0, ENCRYPTED_VALUE_PLACEHOLDER);
        assert!(row.1.is_some());

        let listed = manager.list_memories(MemoryScope::User, None, None).await.unwrap();
        assert_eq!(listed[0].value, "sk-secret");
    }

    #[tokio::test]
    async fn update_memory_changes_value() {
        let manager = in_memory_manager().await;
        let created = manager
            .create_memory(MemoryScope::Workspace, None, None, "key", "old", false)
            .await
            .unwrap();

        let updated = manager.update_memory(created.id, "new").await.unwrap();
        assert_eq!(updated.value, "new");

        let memories = manager.list_memories(MemoryScope::Workspace, None, None).await.unwrap();
        assert_eq!(memories[0].value, "new");
    }

    #[tokio::test]
    async fn delete_memory_removes_row() {
        let manager = in_memory_manager().await;
        let created = manager
            .create_memory(MemoryScope::User, None, None, "key", "value", false)
            .await
            .unwrap();

        manager.delete_memory(created.id).await.unwrap();
        let memories = manager.list_memories(MemoryScope::User, None, None).await.unwrap();
        assert!(memories.is_empty());
    }

    #[tokio::test]
    async fn missing_memory_returns_not_found() {
        let manager = in_memory_manager().await;
        let result = manager.update_memory(MemoryId::new(), "value").await;
        assert!(matches!(result, Err(AppError::NotFound { .. })));
    }
}
