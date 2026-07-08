//! Git operations with command policy, permission engine, and audit logging hooks.

use app_models::{AppError, WorkspaceId};
use app_security::{AuditEvent, PermissionRequest, PermissionResult, SecurityContext};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Provides the set of filesystem roots under which git repositories are
/// allowed to operate. Implementations are typically backed by the current
/// set of workspace roots.
#[async_trait]
pub trait RepoRootProvider: Send + Sync {
    /// Return the currently allowed repository root paths.
    async fn allowed_repo_roots(&self) -> Vec<PathBuf>;
}

struct StaticRepoRootProvider {
    roots: Vec<PathBuf>,
}

#[async_trait]
impl RepoRootProvider for StaticRepoRootProvider {
    async fn allowed_repo_roots(&self) -> Vec<PathBuf> {
        self.roots.clone()
    }
}

/// Product-level git tool guarded by security policies.
#[derive(Clone)]
pub struct GitTool {
    security: Arc<SecurityContext>,
    repo_root_provider: Arc<dyn RepoRootProvider>,
}

impl GitTool {
    /// Create a new git tool backed by a security context and a repository root
    /// provider.
    ///
    /// Production callers must supply a real provider (e.g. one backed by the
    /// workspace storage). The empty-list case is treated as "no allowed roots",
    /// which denies all repository paths.
    #[must_use]
    pub fn new(security: Arc<SecurityContext>, provider: Arc<dyn RepoRootProvider>) -> Self {
        Self {
            security,
            repo_root_provider: provider,
        }
    }

    /// Restrict git operations to repositories inside the given static root
    /// paths. This is intended for tests; production code should use a dynamic
    /// [`RepoRootProvider`].
    #[must_use]
    pub fn with_allowed_repo_roots(self, roots: Vec<PathBuf>) -> Self {
        Self {
            security: self.security,
            repo_root_provider: Arc::new(StaticRepoRootProvider { roots }),
        }
    }

    /// Run a git subcommand after policy checks.
    async fn run_git(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        args: &[String],
        permission_action: &str,
    ) -> Result<String, AppError> {
        self.validate_repo_path(repo_path).await?;

        let command_result = self.security.command_policy().allow_command("git", args);
        if let PermissionResult::Denied { ref reason } = command_result {
            self.audit(workspace_id, repo_path, permission_action, &command_result);
            return Err(AppError::PermissionDenied {
                reason: reason.clone(),
            });
        }

        let permission_result = self.check_permission(workspace_id, repo_path, permission_action);
        if !matches!(permission_result, PermissionResult::Allowed) {
            self.audit(
                workspace_id,
                repo_path,
                permission_action,
                &permission_result,
            );
            let reason = match permission_result {
                PermissionResult::Allowed => unreachable!(),
                PermissionResult::Ask { ref request } => {
                    format!(
                        "approval required for {} on {}",
                        request.action, request.resource
                    )
                }
                PermissionResult::Denied { ref reason } => reason.clone(),
            };
            return Err(AppError::PermissionDenied { reason });
        }

        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(args)
            .output()
            .await
            .map_err(|e| AppError::Internal {
                message: format!("git command failed: {e}"),
            })?;

        let result = if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(AppError::Internal {
                message: format!("git error: {stderr}"),
            })
        };

        let outcome = match &result {
            Ok(_) => PermissionResult::Allowed,
            Err(err) => PermissionResult::Denied {
                reason: err.to_string(),
            },
        };
        self.audit(workspace_id, repo_path, permission_action, &outcome);

        result
    }

    async fn validate_repo_path(&self, repo_path: &str) -> Result<(), AppError> {
        let allowed_roots = self.repo_root_provider.allowed_repo_roots().await;

        let canonical = Path::new(repo_path)
            .canonicalize()
            .map_err(|e| AppError::PermissionDenied {
                reason: format!("invalid git repository path {repo_path}: {e}"),
            })?;

        let allowed = allowed_roots.iter().any(|root| {
            root.canonicalize()
                .is_ok_and(|root| canonical.starts_with(&root))
        });

        if !allowed {
            return Err(AppError::PermissionDenied {
                reason: format!(
                    "git repository path {} is outside allowed workspace roots",
                    canonical.display()
                ),
            });
        }

        Ok(())
    }

    fn check_permission(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        action: &str,
    ) -> PermissionResult {
        let request = PermissionRequest {
            workspace_id,
            thread_id: None,
            run_id: None,
            action: action.to_owned(),
            resource: repo_path.to_owned(),
            trusted_workspace: true,
        };
        self.security.permission_engine().evaluate(request)
    }

    fn audit(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        action: &str,
        outcome: &PermissionResult,
    ) {
        let _ = self.security.audit_logger().log(AuditEvent {
            workspace_id,
            thread_id: None,
            run_id: None,
            action: action.to_owned(),
            resource: repo_path.to_owned(),
            outcome: outcome.clone(),
        });
    }
}

/// Git operations exposed to callers.
#[async_trait]
pub trait GitOperations: Send + Sync {
    /// Return `git status` output.
    async fn status(&self, workspace_id: WorkspaceId, repo_path: &str) -> Result<String, AppError>;
    /// Return `git diff` output.
    async fn diff(&self, workspace_id: WorkspaceId, repo_path: &str) -> Result<String, AppError>;
    /// Stage the given paths.
    async fn stage(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        paths: &[String],
    ) -> Result<(), AppError>;
    /// Unstage the given paths.
    async fn unstage(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        paths: &[String],
    ) -> Result<(), AppError>;
    /// Commit with the given message.
    async fn commit(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        message: &str,
    ) -> Result<String, AppError>;
    /// Create a branch or show the current branch when `name` is `None`.
    async fn branch(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        name: Option<&str>,
    ) -> Result<String, AppError>;
    /// Push to a remote.
    async fn push(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        remote: Option<&str>,
    ) -> Result<String, AppError>;
}

#[async_trait]
impl GitOperations for GitTool {
    async fn status(&self, workspace_id: WorkspaceId, repo_path: &str) -> Result<String, AppError> {
        self.run_git(
            workspace_id,
            repo_path,
            &["status".to_owned(), "--short".to_owned()],
            "filesystem.read",
        )
        .await
    }

    async fn diff(&self, workspace_id: WorkspaceId, repo_path: &str) -> Result<String, AppError> {
        self.run_git(
            workspace_id,
            repo_path,
            &["diff".to_owned()],
            "filesystem.read",
        )
        .await
    }

    async fn stage(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        paths: &[String],
    ) -> Result<(), AppError> {
        let mut args = vec!["add".to_owned()];
        args.extend_from_slice(paths);
        self.run_git(workspace_id, repo_path, &args, "filesystem.write").await?;
        Ok(())
    }

    async fn unstage(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        paths: &[String],
    ) -> Result<(), AppError> {
        let mut args = vec!["reset".to_owned(), "HEAD".to_owned()];
        args.extend_from_slice(paths);
        self.run_git(workspace_id, repo_path, &args, "filesystem.write").await?;
        Ok(())
    }

    async fn commit(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        message: &str,
    ) -> Result<String, AppError> {
        self.run_git(
            workspace_id,
            repo_path,
            &["commit".to_owned(), "-m".to_owned(), message.to_owned()],
            "git.commit",
        )
        .await
    }

    async fn branch(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        name: Option<&str>,
    ) -> Result<String, AppError> {
        let args = name.map_or_else(
            || vec!["branch".to_owned(), "--show-current".to_owned()],
            |name| vec!["branch".to_owned(), name.to_owned()],
        );
        self.run_git(workspace_id, repo_path, &args, "filesystem.read").await
    }

    async fn push(
        &self,
        workspace_id: WorkspaceId,
        repo_path: &str,
        remote: Option<&str>,
    ) -> Result<String, AppError> {
        let mut args = vec!["push".to_owned()];
        if let Some(remote) = remote {
            args.push(remote.to_owned());
        }
        self.run_git(workspace_id, repo_path, &args, "git.push").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_security::{
        CommandPolicy, DefaultCommandPolicy, DefaultNetworkPolicy, MemoryAuditLogger,
        PermissionEngine, PermissionRequest, PermissionResult,
    };

    struct AllowAllEngine;

    impl PermissionEngine for AllowAllEngine {
        fn evaluate(&self, _request: PermissionRequest) -> PermissionResult {
            PermissionResult::Allowed
        }
    }

    fn test_security() -> Arc<SecurityContext> {
        Arc::new(SecurityContext::new(
            Arc::new(AllowAllEngine),
            Arc::new(DefaultCommandPolicy::new()),
            Arc::new(DefaultNetworkPolicy::new()),
            Arc::new(MemoryAuditLogger::new()),
        ))
    }

    fn test_git_tool(allowed_roots: Vec<PathBuf>) -> GitTool {
        GitTool::new(
            test_security(),
            Arc::new(StaticRepoRootProvider { roots: Vec::new() }),
        )
        .with_allowed_repo_roots(allowed_roots)
    }

    fn init_repo(path: &std::path::Path) {
        let output = std::process::Command::new("git")
            .arg("init")
            .arg(path)
            .output()
            .expect("git init");
        assert!(output.status.success(), "git init failed");

        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["config", "user.email", "test@portico.local"])
            .output()
            .expect("git config email");
        assert!(output.status.success());

        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["config", "user.name", "Portico Test"])
            .output()
            .expect("git config name");
        assert!(output.status.success());
    }

    #[tokio::test]
    async fn status_in_empty_repo() {
        let tmp = std::env::temp_dir().join(format!("portico-git-status-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        init_repo(&tmp);

        let git = test_git_tool(vec![tmp.clone()]);
        let status = git
            .status(WorkspaceId::default(), &tmp.to_string_lossy())
            .await
            .expect("status");
        assert!(status.is_empty());
    }

    #[tokio::test]
    async fn stage_commit_and_branch() {
        let tmp = std::env::temp_dir().join(format!("portico-git-commit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        init_repo(&tmp);

        let file_path = tmp.join("hello.txt");
        std::fs::write(&file_path, "world").unwrap();

        let git = test_git_tool(vec![tmp.clone()]);
        git.stage(
            WorkspaceId::default(),
            &tmp.to_string_lossy(),
            &[file_path.to_string_lossy().to_string()],
        )
        .await
        .expect("stage");
        git.commit(WorkspaceId::default(), &tmp.to_string_lossy(), "init")
            .await
            .expect("commit");

        let branch = git
            .branch(WorkspaceId::default(), &tmp.to_string_lossy(), None)
            .await
            .expect("branch");
        assert!(!branch.is_empty());
    }

    #[tokio::test]
    async fn denies_force_push() {
        let policy = DefaultCommandPolicy::new();
        let result = policy.allow_command(
            "git",
            &["push".to_owned(), "origin".to_owned(), "--force".to_owned()],
        );
        assert!(matches!(result, PermissionResult::Denied { .. }));
    }

    #[tokio::test]
    async fn commit_is_audited() {
        let audit = Arc::new(MemoryAuditLogger::new());
        let security = Arc::new(SecurityContext::new(
            Arc::new(AllowAllEngine),
            Arc::new(DefaultCommandPolicy::new()),
            Arc::new(DefaultNetworkPolicy::new()),
            audit.clone(),
        ));
        let tmp = std::env::temp_dir().join(format!("portico-git-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        init_repo(&tmp);
        std::fs::write(tmp.join("f.txt"), "x").unwrap();

        let git = GitTool::new(
            security,
            Arc::new(StaticRepoRootProvider { roots: Vec::new() }),
        )
        .with_allowed_repo_roots(vec![tmp.clone()]);
        git.stage(
            WorkspaceId::default(),
            &tmp.to_string_lossy(),
            &["f.txt".to_owned()],
        )
        .await
        .unwrap();
        git.commit(WorkspaceId::default(), &tmp.to_string_lossy(), "audit test")
            .await
            .unwrap();

        let events = audit.events().expect("read events");
        assert!(events.iter().any(|e| e.action == "git.commit"));
    }

    #[tokio::test]
    async fn denies_repo_path_outside_allowed_roots() {
        let allowed = std::env::temp_dir().join(format!(
            "portico-git-allowed-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&allowed).unwrap();
        init_repo(&allowed);

        let outside = std::env::temp_dir().join(format!(
            "portico-git-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&outside).unwrap();
        init_repo(&outside);

        let git = test_git_tool(vec![allowed.clone()]);
        let result = git
            .status(WorkspaceId::default(), &outside.to_string_lossy())
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }
}
