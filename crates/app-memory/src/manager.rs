//! Memory manager trait and SQLite-backed implementation.

use app_models::{AppError, MemoryId, MemoryItem, MemoryScope, ThreadId, WorkspaceId};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;

/// Persistence operations for agent memories.
#[async_trait]
pub trait MemoryManager: Send + Sync {
    /// Create a new memory item.
    ///
    /// # Errors
    ///
    /// Returns an error if persistence fails.
    async fn create_memory(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
        key: &str,
        value: &str,
        sensitive: bool,
    ) -> Result<MemoryItem, AppError>;

    /// List memories matching the given scope filters.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    async fn list_memories(
        &self,
        scope: MemoryScope,
        workspace_id: Option<WorkspaceId>,
        thread_id: Option<ThreadId>,
    ) -> Result<Vec<MemoryItem>, AppError>;

    /// Update the value of an existing memory.
    ///
    /// # Errors
    ///
    /// Returns an error if the memory is missing or cannot be updated.
    async fn update_memory(&self, id: MemoryId, value: &str) -> Result<MemoryItem, AppError>;

    /// Delete a memory by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the memory is missing or cannot be deleted.
    async fn delete_memory(&self, id: MemoryId) -> Result<(), AppError>;
}

/// SQLite-backed [`MemoryManager`] implementation.
#[derive(Debug, Clone)]
pub struct SqliteMemoryManager {
    pool: SqlitePool,
}

impl SqliteMemoryManager {
    /// Create a new memory manager backed by the given connection pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
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
        let id = MemoryId::new();
        let now = Utc::now();
        let sensitive_i64 = i64::from(sensitive);

        sqlx::query(
            "INSERT INTO memories (id, scope, workspace_id, thread_id, key, value, sensitive, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.0)
        .bind(scope.as_str())
        .bind(workspace_id.map(|wid| wid.0))
        .bind(thread_id.map(|tid| tid.0))
        .bind(key)
        .bind(value)
        .bind(sensitive_i64)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("create_memory failed: {e}") })?;

        Ok(MemoryItem {
            id,
            scope,
            workspace_id,
            thread_id,
            key: key.to_owned(),
            value: value.to_owned(),
            sensitive,
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
            "SELECT id, scope, workspace_id, thread_id, key, value, sensitive, created_at, updated_at FROM memories WHERE scope = ?",
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

        Ok(rows.into_iter().map(MemoryRow::into).collect())
    }

    async fn update_memory(&self, id: MemoryId, value: &str) -> Result<MemoryItem, AppError> {
        let now = Utc::now();

        let result = sqlx::query("UPDATE memories SET value = ?, updated_at = ? WHERE id = ?")
            .bind(value)
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

        let row = sqlx::query_as::<_, MemoryRow>(
            "SELECT id, scope, workspace_id, thread_id, key, value, sensitive, created_at, updated_at FROM memories WHERE id = ?",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal { message: format!("fetch updated memory failed: {e}") })?;

        row.map(MemoryRow::into).ok_or_else(|| AppError::NotFound {
            resource: format!("memory {id:?}"),
        })
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
}

impl From<MemoryRow> for MemoryItem {
    fn from(row: MemoryRow) -> Self {
        Self {
            id: MemoryId(row.id),
            scope: row.scope.as_str().try_into().unwrap_or(MemoryScope::Session),
            workspace_id: row.workspace_id.map(WorkspaceId),
            thread_id: row.thread_id.map(ThreadId),
            key: row.key,
            value: row.value,
            sensitive: row.sensitive != 0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn in_memory_manager() -> SqliteMemoryManager {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE memories (
                id BLOB PRIMARY KEY NOT NULL,
                scope TEXT NOT NULL,
                workspace_id BLOB,
                thread_id BLOB,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                sensitive INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        SqliteMemoryManager::new(pool)
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
