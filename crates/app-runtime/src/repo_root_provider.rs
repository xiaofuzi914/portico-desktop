//! Dynamic repository root provider backed by workspace storage.

use crate::storage::Storage;
use app_tools::git::RepoRootProvider;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

/// Returns the current workspace root paths from storage on every call.
///
/// This provider is intentionally simple: it queries storage directly so that
/// newly created workspaces are immediately visible, avoiding stale caches and
/// the complexity of cache invalidation hooks. The `list_workspaces` query is
/// cheap and git operations are infrequent enough for this to be acceptable.
pub struct StorageRepoRootProvider {
    storage: Arc<dyn Storage>,
}

impl StorageRepoRootProvider {
    /// Create a provider backed by `storage`.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl RepoRootProvider for StorageRepoRootProvider {
    async fn allowed_repo_roots(&self) -> Vec<PathBuf> {
        self.storage
            .list_workspaces()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|ws| PathBuf::from(ws.root_path))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStorage;

    #[tokio::test]
    async fn returns_workspace_roots_dynamically() {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let provider = StorageRepoRootProvider::new(storage.clone());

        assert!(provider.allowed_repo_roots().await.is_empty());

        storage
            .create_workspace("test", "/tmp/portico-provider-test", false)
            .await
            .expect("create workspace");

        let roots = provider.allowed_repo_roots().await;
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], PathBuf::from("/tmp/portico-provider-test"));
    }
}
