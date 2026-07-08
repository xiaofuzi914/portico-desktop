//! Worktree management for isolated thread working directories.

use crate::storage::Storage;
use app_models::{AppError, ThreadId, WorkspaceId, Worktree, WorktreeId};
use std::path::PathBuf;
use std::sync::Arc;

/// Manages per-thread worktrees inside workspaces.
pub struct WorktreeManager {
    storage: Arc<dyn Storage>,
}

impl WorktreeManager {
    /// Create a new worktree manager.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Create a new worktree directory for a thread inside a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace does not exist, the directory cannot
    /// be created, or the worktree cannot be persisted.
    pub async fn create_worktree(
        &self,
        workspace_id: WorkspaceId,
        thread_id: ThreadId,
        name: &str,
    ) -> Result<Worktree, AppError> {
        let workspace = self.storage.get_workspace(workspace_id).await?;
        let sanitized = sanitize_name(name);
        if sanitized.is_empty() {
            return Err(AppError::Internal {
                message: "worktree name cannot be empty".to_owned(),
            });
        }

        let path = PathBuf::from(&workspace.root_path)
            .join(".portico")
            .join("worktrees")
            .join(thread_id.0.to_string())
            .join(&sanitized);
        let path_str = path.to_string_lossy().to_string();

        tokio::fs::create_dir_all(&path).await.map_err(|e| AppError::Internal {
            message: format!("failed to create worktree directory: {e}"),
        })?;

        self.storage.create_worktree(workspace_id, thread_id, name, &path_str).await
    }

    /// Delete a worktree row and remove its directory if it still exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the worktree is missing or cannot be deleted.
    pub async fn delete_worktree(&self, id: WorktreeId) -> Result<(), AppError> {
        if let Ok(worktree) = self.storage.get_worktree(id).await {
            let _ = tokio::fs::remove_dir_all(&worktree.path).await;
        }
        self.storage.delete_worktree(id).await
    }

    /// List worktrees in a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if worktrees cannot be read.
    pub async fn list_worktrees(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<Worktree>, AppError> {
        self.storage.list_worktrees(workspace_id).await
    }
}

fn sanitize_name(name: &str) -> String {
    name.replace("..", "-")
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' => '-',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStorage;

    async fn setup() -> (WorktreeManager, WorkspaceId, ThreadId) {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let workspace = storage
            .create_workspace("test", "/tmp/portico-wt-test", false)
            .await
            .expect("create workspace");
        let thread = storage.create_thread(workspace.id, "thread").await.expect("create thread");
        (WorktreeManager::new(storage), workspace.id, thread.id)
    }

    #[tokio::test]
    async fn create_and_list_worktrees() {
        let (manager, workspace_id, thread_id) = setup().await;

        let worktree = manager
            .create_worktree(workspace_id, thread_id, "feature")
            .await
            .expect("create worktree");
        assert_eq!(worktree.workspace_id, workspace_id);
        assert_eq!(worktree.thread_id, thread_id);
        assert!(worktree.path.contains("feature"));
        assert!(tokio::fs::metadata(&worktree.path).await.is_ok());

        let list = manager.list_worktrees(workspace_id).await.expect("list worktrees");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, worktree.id);
    }

    #[tokio::test]
    async fn delete_worktree_removes_row_and_directory() {
        let (manager, workspace_id, thread_id) = setup().await;

        let worktree = manager
            .create_worktree(workspace_id, thread_id, "temp")
            .await
            .expect("create worktree");
        let path = worktree.path.clone();

        manager.delete_worktree(worktree.id).await.expect("delete worktree");

        let list = manager.list_worktrees(workspace_id).await.expect("list after delete");
        assert!(list.is_empty());
        assert!(tokio::fs::metadata(&path).await.is_err());
    }
}
