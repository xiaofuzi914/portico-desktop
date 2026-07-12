//! Workspace management with security-aware path permissions.

use crate::storage::Storage;
use app_models::{AppError, Workspace, WorkspaceId};
use app_security::{AuditEvent, PermissionRequest, PermissionResult, SecurityContext};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
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
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
            return Err(AppError::PermissionDenied {
                reason: "workspace name must contain 1 to 120 printable characters".to_owned(),
            });
        }

        let root = Self::canonical_workspace_root(root_path)?;
        let canonical_root = root.to_str().ok_or_else(|| AppError::PermissionDenied {
            reason: "workspace path must be valid UTF-8".to_owned(),
        })?;

        let duplicate = self
            .storage
            .list_workspaces()
            .await?
            .into_iter()
            .any(|workspace| Path::new(&workspace.root_path) == root);
        if duplicate {
            return Err(AppError::PermissionDenied {
                reason: "this directory is already registered as a workspace".to_owned(),
            });
        }

        self.storage.create_workspace(name, canonical_root, false).await
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
        let workspace = self.storage.get_workspace(id).await?;
        if trusted {
            // Trusted projects default to full read/write under the project root.
            // Only fill missing sides so explicit allowlists are preserved.
            let read_paths = if workspace.allowed_read_paths.is_empty() {
                vec![workspace.root_path.clone()]
            } else {
                workspace.allowed_read_paths.clone()
            };
            let write_paths = if workspace.allowed_write_paths.is_empty() {
                vec![workspace.root_path.clone()]
            } else {
                workspace.allowed_write_paths.clone()
            };
            if workspace.allowed_read_paths.is_empty() || workspace.allowed_write_paths.is_empty() {
                self.set_allowed_paths(id, read_paths, write_paths).await?;
            }
        }
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
        let read_paths = read_paths
            .iter()
            .map(|path| Self::canonical_allowed_path(root, path))
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect();
        let write_paths = write_paths
            .iter()
            .map(|path| Self::canonical_allowed_path(root, path))
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect();

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

        let Ok(canonical) = Path::new(path).canonicalize() else {
            return PermissionResult::Denied {
                reason: format!("read path does not exist or cannot be resolved: {path}"),
            };
        };
        if !path_is_under_any(&canonical, &workspace.allowed_read_paths) {
            return PermissionResult::Denied {
                reason: format!("read access is outside the workspace sandbox: {path}"),
            };
        }

        let request = PermissionRequest {
            workspace_id,
            thread_id: None,
            run_id: None,
            action: "filesystem.read".to_owned(),
            resource: canonical.to_string_lossy().into_owned(),
            trusted_workspace: workspace.trusted,
        };
        let engine_result = self.security.permissions.evaluate(request);
        if matches!(engine_result, PermissionResult::Allowed) {
            PermissionResult::Allowed
        } else {
            PermissionResult::Denied {
                reason: format!("read access denied by workspace policy: {path}"),
            }
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

        let canonical = match canonicalize_write_target(Path::new(path)) {
            Ok(path) => path,
            Err(reason) => return PermissionResult::Denied { reason },
        };
        if !path_is_under_any(&canonical, &workspace.allowed_write_paths) {
            return PermissionResult::Denied {
                reason: format!("write access is outside the workspace sandbox: {path}"),
            };
        }

        let request = PermissionRequest {
            workspace_id,
            thread_id: None,
            run_id: None,
            action: "filesystem.write".to_owned(),
            resource: canonical.to_string_lossy().into_owned(),
            trusted_workspace: workspace.trusted,
        };
        let engine_result = self.security.permissions.evaluate(request);
        if matches!(engine_result, PermissionResult::Allowed) {
            PermissionResult::Allowed
        } else {
            PermissionResult::Denied {
                reason: format!("write access requires an active run approval: {path}"),
            }
        }
    }

    fn canonical_workspace_root(path: &str) -> Result<PathBuf, AppError> {
        let p = Path::new(path);
        if !p.is_absolute() {
            return Err(AppError::PermissionDenied {
                reason: "workspace path must be an absolute directory selected by the user"
                    .to_owned(),
            });
        }
        let canonical = p.canonicalize().map_err(|_| AppError::PermissionDenied {
            reason: format!("workspace directory does not exist or cannot be resolved: {path}"),
        })?;
        let metadata = canonical.metadata().map_err(|_| AppError::PermissionDenied {
            reason: format!("workspace directory cannot be inspected: {path}"),
        })?;
        if !metadata.is_dir() {
            return Err(AppError::PermissionDenied {
                reason: format!("workspace path is not a directory: {path}"),
            });
        }
        std::fs::read_dir(&canonical).map_err(|_| AppError::PermissionDenied {
            reason: format!("workspace directory is not readable: {path}"),
        })?;
        Self::reject_dangerous_root(&canonical)?;
        Ok(canonical)
    }

    fn reject_dangerous_root(root: &Path) -> Result<(), AppError> {
        let is_filesystem_root = root.parent().is_none();
        let contains_home = dirs::home_dir().is_some_and(|home| home.starts_with(root));
        let is_temp_root = std::env::temp_dir().canonicalize().is_ok_and(|temp| temp == root);
        let is_app_data_root = [dirs::data_dir(), dirs::config_dir(), dirs::cache_dir()]
            .into_iter()
            .flatten()
            .filter_map(|path| path.canonicalize().ok())
            .any(|path| path == root);
        let is_sensitive_system_root = [
            "/Applications",
            "/Library",
            "/System",
            "/Users",
            "/bin",
            "/etc",
            "/private",
            "/sbin",
            "/usr",
            "/var",
        ]
        .into_iter()
        .any(|path| root == Path::new(path));

        if is_filesystem_root
            || contains_home
            || is_temp_root
            || is_app_data_root
            || is_sensitive_system_root
        {
            return Err(AppError::PermissionDenied {
                reason: format!(
                    "workspace root is too broad or security-sensitive: {}",
                    root.display()
                ),
            });
        }
        Ok(())
    }

    fn canonical_allowed_path(root: &Path, path: &str) -> Result<String, AppError> {
        let p = Path::new(path);
        if !p.is_absolute() {
            return Err(AppError::PermissionDenied {
                reason: format!("allowed path must be absolute: {path}"),
            });
        }
        let canonical = p.canonicalize().map_err(|_| AppError::PermissionDenied {
            reason: format!("allowed path does not exist or cannot be resolved: {path}"),
        })?;
        if !canonical.starts_with(root) {
            return Err(AppError::PermissionDenied {
                reason: format!("allowed path must be inside workspace root: {path}"),
            });
        }
        if !canonical.is_dir() {
            return Err(AppError::PermissionDenied {
                reason: format!("allowed path must be a directory: {path}"),
            });
        }
        canonical
            .to_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| AppError::PermissionDenied {
                reason: "allowed path must be valid UTF-8".to_owned(),
            })
    }
}

fn canonicalize_write_target(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|_| format!("write path cannot be resolved: {}", path.display()));
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("write path has no parent: {}", path.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("write path has no file name: {}", path.display()))?;
    parent
        .canonicalize()
        .map(|canonical_parent| canonical_parent.join(file_name))
        .map_err(|_| {
            format!(
                "write parent does not exist or cannot be resolved: {}",
                parent.display()
            )
        })
}

fn path_is_under_any(path: &Path, allowed: &[String]) -> bool {
    allowed.iter().any(|allowed_path| path.starts_with(Path::new(allowed_path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStorage;
    use app_security::{
        DefaultCommandPolicy, DefaultNetworkPolicy, MemoryAuditLogger, PermissionEngine,
        PermissionRequest, PermissionResult, PolicyPermissionEngine,
    };
    use tempfile::TempDir;

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

    async fn setup() -> (WorkspaceManager, Workspace, TempDir) {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let manager = WorkspaceManager::new(storage.clone(), test_security());
        let root = tempfile::tempdir().expect("workspace root");
        std::fs::create_dir(root.path().join("src")).expect("src dir");
        std::fs::create_dir(root.path().join("out")).expect("out dir");
        let workspace = manager
            .add_workspace("test", root.path().to_str().expect("utf8 path"))
            .await
            .expect("create workspace");
        (manager, workspace, root)
    }

    #[tokio::test]
    async fn add_workspace_is_untrusted() {
        let (_manager, workspace, _root) = setup().await;
        assert!(!workspace.trusted);
        assert!(workspace.allowed_read_paths.is_empty());
        assert!(workspace.allowed_write_paths.is_empty());
    }

    #[tokio::test]
    async fn trust_workspace_updates_flag() {
        let (manager, workspace, _root) = setup().await;
        let updated = manager.trust_workspace(workspace.id, true).await.expect("trust workspace");
        assert!(updated.trusted);

        let fetched = manager.storage.get_workspace(workspace.id).await.expect("fetch workspace");
        assert!(fetched.trusted);
    }

    #[tokio::test]
    async fn allowed_paths_control_access() {
        let (manager, workspace, root) = setup().await;
        let read_path = root.path().join("src").to_string_lossy().into_owned();
        let write_path = root.path().join("out").to_string_lossy().into_owned();

        manager
            .set_allowed_paths(
                workspace.id,
                vec![read_path.clone()],
                vec![write_path.clone()],
            )
            .await
            .expect("set allowed paths");

        assert!(matches!(
            manager.can_read(workspace.id, &read_path).await,
            PermissionResult::Denied { .. }
        ));
        assert!(matches!(
            manager.can_read(workspace.id, "/tmp/other/file.rs").await,
            PermissionResult::Denied { .. }
        ));
        assert!(matches!(
            manager
                .can_write(
                    workspace.id,
                    &root.path().join("out/artifact.txt").to_string_lossy()
                )
                .await,
            PermissionResult::Denied { .. }
        ));

        manager.trust_workspace(workspace.id, true).await.expect("trust");
        assert!(matches!(
            manager.can_read(workspace.id, &read_path).await,
            PermissionResult::Allowed
        ));
        // Explicit write allowlist to `out/` is preserved after trust.
        assert!(matches!(
            manager
                .can_write(
                    workspace.id,
                    &root.path().join("out/artifact.txt").to_string_lossy()
                )
                .await,
            PermissionResult::Allowed
        ));
    }

    #[tokio::test]
    async fn allowed_paths_must_be_inside_root() {
        let (manager, workspace, _root) = setup().await;
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
        let root = tempfile::tempdir().expect("workspace root");
        let workspace = manager
            .add_workspace("test", root.path().to_str().expect("utf8 path"))
            .await
            .unwrap();

        manager.trust_workspace(workspace.id, true).await.unwrap();

        let events = audit.events().expect("read events");
        assert!(events.iter().any(|e| e.action == "workspace.trust"));
    }

    #[tokio::test]
    async fn trusting_workspace_allows_reading_and_writing_its_root_by_default() {
        let (manager, workspace, root) = setup().await;

        let trusted = manager.trust_workspace(workspace.id, true).await.unwrap();
        let root_path = root.path().canonicalize().unwrap().to_string_lossy().into_owned();

        assert!(trusted.trusted);
        assert_eq!(trusted.allowed_read_paths, vec![root_path.clone()]);
        assert_eq!(trusted.allowed_write_paths, vec![root_path]);
    }

    #[tokio::test]
    async fn add_workspace_validates_and_canonicalizes_the_root() {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let manager = WorkspaceManager::new(storage.clone(), test_security());
        let parent = tempfile::tempdir().expect("temp parent");
        let root = parent.path().join("project");
        std::fs::create_dir(&root).expect("project dir");

        let relative = manager.add_workspace("test", ".").await;
        assert!(matches!(relative, Err(AppError::PermissionDenied { .. })));

        let missing = manager
            .add_workspace("test", &parent.path().join("missing").to_string_lossy())
            .await;
        assert!(matches!(missing, Err(AppError::PermissionDenied { .. })));

        let file = parent.path().join("file.txt");
        std::fs::write(&file, "data").expect("test file");
        let file_result = manager.add_workspace("test", &file.to_string_lossy()).await;
        assert!(matches!(
            file_result,
            Err(AppError::PermissionDenied { .. })
        ));

        let workspace = manager
            .add_workspace("  project  ", &root.to_string_lossy())
            .await
            .expect("valid workspace");
        assert_eq!(workspace.name, "project");
        assert_eq!(
            workspace.root_path,
            root.canonicalize().unwrap().to_string_lossy()
        );
        assert!(!workspace.trusted);

        let duplicate = manager.add_workspace("duplicate", &root.to_string_lossy()).await;
        assert!(matches!(duplicate, Err(AppError::PermissionDenied { .. })));
        assert_eq!(storage.list_workspaces().await.expect("list").len(), 1);
    }

    #[tokio::test]
    async fn add_workspace_rejects_dangerous_roots() {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let manager = WorkspaceManager::new(storage, test_security());

        let filesystem_root = manager.add_workspace("root", "/").await;
        assert!(matches!(
            filesystem_root,
            Err(AppError::PermissionDenied { .. })
        ));

        if let Some(home) = dirs::home_dir() {
            let home_result = manager.add_workspace("home", &home.to_string_lossy()).await;
            assert!(matches!(
                home_result,
                Err(AppError::PermissionDenied { .. })
            ));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn allowed_paths_reject_symlink_escape() {
        use std::os::unix::fs::symlink;

        let (manager, workspace, root) = setup().await;
        let outside = tempfile::tempdir().expect("outside root");
        let escape = root.path().join("escape");
        symlink(outside.path(), &escape).expect("create symlink");

        let result = manager
            .set_allowed_paths(
                workspace.id,
                vec![escape.to_string_lossy().into_owned()],
                vec![],
            )
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }
}
