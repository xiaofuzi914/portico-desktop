//! Workspace management with security-aware path permissions.

use crate::storage::Storage;
use app_models::{AppError, Workspace, WorkspaceId};
use app_security::{AuditEvent, PermissionRequest, PermissionResult, SecurityContext};
use std::path::Path;
use std::sync::Arc;

/// Manages workspace lifecycle, trust state, and allowed filesystem paths.
pub struct WorkspaceManager {
    storage: Arc<dyn Storage>,
    security: SecurityContext,
}

impl WorkspaceManager {
    /// Create a new workspace manager.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, security: SecurityContext) -> Self {
        Self { storage, security }
    }

    /// Add a new workspace. New workspaces are untrusted by default.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace cannot be persisted.
    pub async fn add_workspace(&self, name: &str, root_path: &str) -> Result<Workspace, AppError> {
        self.storage.create_workspace(name, root_path, false).await
    }

    /// Update whether a workspace is trusted for sensitive operations.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace is missing or cannot be updated.
    pub async fn trust_workspace(
        &self,
        id: WorkspaceId,
        trusted: bool,
    ) -> Result<Workspace, AppError> {
        let workspace = self.storage.update_workspace_trusted(id, trusted).await?;

        let _ = self.security.audit.log(AuditEvent {
            workspace_id: id,
            thread_id: None,
            run_id: None,
            action: "workspace.trust".to_owned(),
            resource: format!("trusted={trusted}"),
            outcome: PermissionResult::Allowed,
        });

        Ok(workspace)
    }

    /// Replace the allowed read and write paths for a workspace.
    ///
    /// Paths must be absolute and reside inside the workspace root.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace is missing, a path is invalid, or the
    /// paths cannot be persisted.
    pub async fn set_allowed_paths(
        &self,
        id: WorkspaceId,
        read_paths: Vec<String>,
        write_paths: Vec<String>,
    ) -> Result<(), AppError> {
        let workspace = self.storage.get_workspace(id).await?;
        let root = Path::new(&workspace.root_path);

        for path in &read_paths {
            Self::validate_allowed_path(root, path)?;
        }
        for path in &write_paths {
            Self::validate_allowed_path(root, path)?;
        }

        self.storage.set_workspace_allowed_paths(id, read_paths, write_paths).await?;

        let _ = self.security.audit.log(AuditEvent {
            workspace_id: id,
            thread_id: None,
            run_id: None,
            action: "workspace.set_allowed_paths".to_owned(),
            resource: workspace.root_path,
            outcome: PermissionResult::Allowed,
        });

        Ok(())
    }

    /// Check whether `path` may be read inside the workspace.
    #[must_use]
    pub async fn can_read(&self, workspace_id: WorkspaceId, path: &str) -> PermissionResult {
        let workspace = match self.storage.get_workspace(workspace_id).await {
            Ok(ws) => ws,
            Err(err) => {
                return PermissionResult::Denied {
                    reason: err.to_string(),
                };
            }
        };

        let request = PermissionRequest {
            workspace_id,
            thread_id: None,
            run_id: None,
            action: "filesystem.read".to_owned(),
            resource: path.to_owned(),
            trusted_workspace: workspace.trusted,
        };
        let engine_result = self.security.permissions.evaluate(request);
        if matches!(engine_result, PermissionResult::Allowed) {
            return PermissionResult::Allowed;
        }

        if path_is_under_any(path, &workspace.allowed_read_paths) {
            return PermissionResult::Allowed;
        }

        // Without a run context an Ask cannot be turned into a resumable approval
        // request, so treat it as a denial.
        if let PermissionResult::Ask { .. } = engine_result {
            return PermissionResult::Denied {
                reason: format!("read access denied for {path}"),
            };
        }

        PermissionResult::Denied {
            reason: format!("read access denied for {path}"),
        }
    }

    /// Check whether `path` may be written inside the workspace.
    #[must_use]
    pub async fn can_write(&self, workspace_id: WorkspaceId, path: &str) -> PermissionResult {
        let workspace = match self.storage.get_workspace(workspace_id).await {
            Ok(ws) => ws,
            Err(err) => {
                return PermissionResult::Denied {
                    reason: err.to_string(),
                };
            }
        };

        let request = PermissionRequest {
            workspace_id,
            thread_id: None,
            run_id: None,
            action: "filesystem.write".to_owned(),
            resource: path.to_owned(),
            trusted_workspace: workspace.trusted,
        };
        let engine_result = self.security.permissions.evaluate(request);
        if matches!(engine_result, PermissionResult::Allowed) {
            return PermissionResult::Allowed;
        }

        if path_is_under_any(path, &workspace.allowed_write_paths) {
            return PermissionResult::Allowed;
        }

        // Without a run context an Ask cannot be turned into a resumable approval
        // request, so treat it as a denial.
        if let PermissionResult::Ask { .. } = engine_result {
            return PermissionResult::Denied {
                reason: format!("write access denied for {path}"),
            };
        }

        PermissionResult::Denied {
            reason: format!("write access denied for {path}"),
        }
    }

    fn validate_allowed_path(root: &Path, path: &str) -> Result<(), AppError> {
        let p = Path::new(path);
        if !p.is_absolute() {
            return Err(AppError::PermissionDenied {
                reason: format!("allowed path must be absolute: {path}"),
            });
        }
        if !p.starts_with(root) {
            return Err(AppError::PermissionDenied {
                reason: format!("allowed path must be inside workspace root: {path}"),
            });
        }
        Ok(())
    }
}

fn path_is_under_any(path: &str, allowed: &[String]) -> bool {
    let p = Path::new(path);
    allowed.iter().any(|allowed_path| p.starts_with(Path::new(allowed_path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStorage;
    use app_security::{
        DefaultCommandPolicy, DefaultNetworkPolicy, MemoryAuditLogger, PermissionEngine,
        PermissionRequest, PermissionResult, PolicyPermissionEngine,
    };

    struct AllowAllEngine;

    impl PermissionEngine for AllowAllEngine {
        fn evaluate(&self, _request: PermissionRequest) -> PermissionResult {
            PermissionResult::Allowed
        }
    }

    fn test_security() -> SecurityContext {
        SecurityContext::new(
            Arc::new(PolicyPermissionEngine::default_rules()),
            Arc::new(DefaultCommandPolicy::new()),
            Arc::new(DefaultNetworkPolicy::new()),
            Arc::new(MemoryAuditLogger::new()),
        )
    }

    async fn setup() -> (WorkspaceManager, Workspace) {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let manager = WorkspaceManager::new(storage.clone(), test_security());
        let workspace = manager
            .add_workspace("test", "/tmp/portico-test")
            .await
            .expect("create workspace");
        (manager, workspace)
    }

    #[tokio::test]
    async fn add_workspace_is_untrusted() {
        let (_manager, workspace) = setup().await;
        assert!(!workspace.trusted);
        assert!(workspace.allowed_read_paths.is_empty());
        assert!(workspace.allowed_write_paths.is_empty());
    }

    #[tokio::test]
    async fn trust_workspace_updates_flag() {
        let (manager, workspace) = setup().await;
        let updated = manager.trust_workspace(workspace.id, true).await.expect("trust workspace");
        assert!(updated.trusted);

        let fetched = manager.storage.get_workspace(workspace.id).await.expect("fetch workspace");
        assert!(fetched.trusted);
    }

    #[tokio::test]
    async fn allowed_paths_control_access() {
        let (manager, workspace) = setup().await;
        let read_path = "/tmp/portico-test/src".to_owned();
        let write_path = "/tmp/portico-test/out".to_owned();

        manager
            .set_allowed_paths(
                workspace.id,
                vec![read_path.clone()],
                vec![write_path.clone()],
            )
            .await
            .expect("set allowed paths");

        assert!(matches!(
            manager.can_read(workspace.id, "/tmp/portico-test/src/main.rs").await,
            PermissionResult::Allowed
        ));
        assert!(matches!(
            manager.can_read(workspace.id, "/tmp/other/file.rs").await,
            PermissionResult::Denied { .. }
        ));
        assert!(matches!(
            manager.can_write(workspace.id, "/tmp/portico-test/out/artifact.txt").await,
            PermissionResult::Allowed
        ));
        assert!(matches!(
            manager.can_write(workspace.id, "/tmp/portico-test/src/main.rs").await,
            PermissionResult::Denied { .. }
        ));
    }

    #[tokio::test]
    async fn allowed_paths_must_be_inside_root() {
        let (manager, workspace) = setup().await;
        let result = manager
            .set_allowed_paths(workspace.id, vec!["/outside".to_owned()], vec![])
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn trust_workspace_is_audited() {
        let audit = Arc::new(MemoryAuditLogger::new());
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let security = SecurityContext::new(
            Arc::new(AllowAllEngine),
            Arc::new(DefaultCommandPolicy::new()),
            Arc::new(DefaultNetworkPolicy::new()),
            audit.clone(),
        );
        let manager = WorkspaceManager::new(storage, security);
        let workspace = manager.add_workspace("test", "/tmp/portico-test").await.unwrap();

        manager.trust_workspace(workspace.id, true).await.unwrap();

        let events = audit.events().expect("read events");
        assert!(events.iter().any(|e| e.action == "workspace.trust"));
    }
}
