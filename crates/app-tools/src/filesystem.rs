//! Filesystem operations with path sandboxing, permission checks, and audit hooks.

use crate::git::RepoRootProvider;
use app_models::{AppError, WorkspaceId};
use app_security::{AuditEvent, PermissionRequest, PermissionResult, SecurityContext};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Maximum file size that may be read by [`FilesystemTool::read_file`].
const READ_LIMIT_BYTES: usize = 1_048_576;

/// Maximum directory entries returned by [`FilesystemTool::list_dir`].
const LIST_LIMIT: usize = 500;

/// Maximum search hits returned by [`FilesystemTool::search_files`].
const SEARCH_LIMIT: usize = 100;

/// Product-level filesystem tool guarded by path sandboxing and security policies.
#[derive(Clone)]
pub struct FilesystemTool {
    security: Arc<SecurityContext>,
    root_provider: Arc<dyn RepoRootProvider>,
}

impl FilesystemTool {
    /// Create a new filesystem tool backed by a security context and a root
    /// provider.
    #[must_use]
    pub fn new(security: Arc<SecurityContext>, root_provider: Arc<dyn RepoRootProvider>) -> Self {
        Self {
            security,
            root_provider,
        }
    }

    /// Restrict filesystem operations to the given static root paths. This is
    /// intended for tests; production code should use a dynamic
    /// [`RepoRootProvider`].
    #[must_use]
    pub fn with_allowed_roots(self, roots: Vec<PathBuf>) -> Self {
        Self {
            security: self.security,
            root_provider: Arc::new(StaticRepoRootProvider { roots }),
        }
    }

    /// Read the contents of a file within the allowed roots.
    ///
    /// Returns an error if the path is outside the allowed roots, the file does
    /// not exist, or the file exceeds [`READ_LIMIT_BYTES`].
    ///
    /// # Errors
    ///
    /// Returns an error if the path is outside the allowed roots, is not a file,
    /// cannot be read, or exceeds the read size limit.
    pub async fn read_file(
        &self,
        _workspace_id: WorkspaceId,
        path: &str,
    ) -> Result<String, AppError> {
        let canonical = self.validate_existing_path(path).await?;

        let metadata = tokio::fs::metadata(&canonical).await.map_err(|e| AppError::Internal {
            message: format!("failed to read metadata for {}: {e}", canonical.display()),
        })?;

        if !metadata.is_file() {
            return Err(AppError::Internal {
                message: format!("path is not a file: {path}"),
            });
        }

        if metadata.len() > READ_LIMIT_BYTES as u64 {
            return Err(AppError::Internal {
                message: format!("file exceeds {READ_LIMIT_BYTES} byte read limit"),
            });
        }

        tokio::fs::read_to_string(&canonical).await.map_err(|e| AppError::Internal {
            message: format!("failed to read file {}: {e}", canonical.display()),
        })
    }

    /// Write `content` to a file within the allowed roots.
    ///
    /// If the file does not exist, the parent directory is canonicalized and
    /// validated instead. Write operations are gated by the permission engine
    /// and audited.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is outside the allowed roots, permission is
    /// denied, or the write fails.
    pub async fn write_file(
        &self,
        workspace_id: WorkspaceId,
        path: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let (target, validated_parent) = self.validate_write_path(path).await?;
        self.require_write_permission(workspace_id, path)?;

        tokio::fs::write(&target, content).await.map_err(|e| AppError::Internal {
            message: format!("failed to write file {}: {e}", target.display()),
        })?;

        self.audit(
            workspace_id,
            &validated_parent.to_string_lossy(),
            "filesystem.write",
            &PermissionResult::Allowed,
        );
        Ok(())
    }

    /// Replace the single occurrence of `old_string` with `new_string` in a
    /// file within the allowed roots.
    ///
    /// Returns an error if `old_string` occurs zero times or more than once.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is outside the allowed roots, permission is
    /// denied, `old_string` is not unique, or the write fails.
    pub async fn edit_file(
        &self,
        workspace_id: WorkspaceId,
        path: &str,
        old_string: &str,
        new_string: &str,
    ) -> Result<(), AppError> {
        let (target, validated_parent) = self.validate_write_path(path).await?;
        self.require_write_permission(workspace_id, path)?;

        // Read existing content. If the file does not exist yet, treat it as
        // empty (replacing an empty string is still required to be unique).
        let content = if target.exists() {
            tokio::fs::read_to_string(&target).await.map_err(|e| AppError::Internal {
                message: format!("failed to read file for edit {}: {e}", target.display()),
            })?
        } else {
            String::new()
        };

        let occurrences = content.matches(old_string).count();
        if occurrences == 0 {
            return Err(AppError::Internal {
                message: "old_string not found in file".to_owned(),
            });
        }
        if occurrences > 1 {
            return Err(AppError::Internal {
                message: "old_string is not unique in file".to_owned(),
            });
        }

        let new_content = content.replacen(old_string, new_string, 1);
        tokio::fs::write(&target, new_content).await.map_err(|e| AppError::Internal {
            message: format!("failed to write file after edit {}: {e}", target.display()),
        })?;

        self.audit(
            workspace_id,
            &validated_parent.to_string_lossy(),
            "filesystem.write",
            &PermissionResult::Allowed,
        );
        Ok(())
    }

    /// List the entries of a directory within the allowed roots.
    ///
    /// Returns at most [`LIST_LIMIT`] entries.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is outside the allowed roots or is not a
    /// directory.
    pub async fn list_dir(
        &self,
        _workspace_id: WorkspaceId,
        path: &str,
    ) -> Result<Vec<DirEntry>, AppError> {
        let canonical = self.validate_existing_path(path).await?;

        let metadata = tokio::fs::metadata(&canonical).await.map_err(|e| AppError::Internal {
            message: format!("failed to read metadata for {}: {e}", canonical.display()),
        })?;
        if !metadata.is_dir() {
            return Err(AppError::Internal {
                message: format!("path is not a directory: {path}"),
            });
        }

        let mut entries = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&canonical).await.map_err(|e| AppError::Internal {
            message: format!("failed to read directory {}: {e}", canonical.display()),
        })?;

        while let Some(entry) = read_dir.next_entry().await.map_err(|e| AppError::Internal {
            message: format!("failed to read directory entry: {e}"),
        })? {
            if entries.len() >= LIST_LIMIT {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let kind = match entry.file_type().await {
                Ok(ft) if ft.is_dir() => EntryKind::Directory,
                Ok(ft) if ft.is_symlink() => EntryKind::Symlink,
                Ok(_) => EntryKind::File,
                Err(_) => EntryKind::Unknown,
            };
            entries.push(DirEntry { name, kind });
        }

        Ok(entries)
    }

    /// Search files under `root` using either filename glob matching or content
    /// substring matching.
    ///
    /// Returns at most [`SEARCH_LIMIT`] hits.
    ///
    /// # Errors
    ///
    /// Returns an error if the root path is outside the allowed roots or is not
    /// a directory.
    pub async fn search_files(
        &self,
        _workspace_id: WorkspaceId,
        root: &str,
        pattern: &str,
        mode: SearchMode,
    ) -> Result<Vec<SearchResult>, AppError> {
        let canonical_root = self.validate_existing_path(root).await?;

        let metadata = tokio::fs::metadata(&canonical_root).await.map_err(|e| AppError::Internal {
            message: format!("failed to read metadata for {}: {e}", canonical_root.display()),
        })?;
        if !metadata.is_dir() {
            return Err(AppError::Internal {
                message: format!("search root is not a directory: {root}"),
            });
        }

        let mut results = Vec::new();
        self.search_recursive(&canonical_root, &canonical_root, pattern, mode, &mut results)
            .await?;
        results.truncate(SEARCH_LIMIT);
        Ok(results)
    }

    async fn allowed_roots(&self) -> Vec<PathBuf> {
        self.root_provider.allowed_repo_roots().await
    }

    fn is_path_allowed(canonical: &Path, roots: &[PathBuf]) -> bool {
        roots.iter().any(|root| {
            root.canonicalize()
                .is_ok_and(|root| canonical.starts_with(&root))
        })
    }

    async fn validate_existing_path(&self, path: &str) -> Result<PathBuf, AppError> {
        let allowed_roots = self.allowed_roots().await;
        let canonical = Path::new(path).canonicalize().map_err(|e| AppError::PermissionDenied {
            reason: format!("invalid filesystem path {path}: {e}"),
        })?;

        if !Self::is_path_allowed(&canonical, &allowed_roots) {
            return Err(AppError::PermissionDenied {
                reason: format!(
                    "filesystem path {} is outside allowed workspace roots",
                    canonical.display()
                ),
            });
        }

        Ok(canonical)
    }

    /// Validate a path for a write operation.
    ///
    /// If the target exists, it is canonicalized and validated. If it does not
    /// exist, the parent directory is canonicalized and validated, and the
    /// returned target path is resolved relative to that canonical parent.
    async fn validate_write_path(&self, path: &str) -> Result<(PathBuf, PathBuf), AppError> {
        let allowed_roots = self.allowed_roots().await;
        let target = Path::new(path);

        if target.exists() {
            let canonical = target.canonicalize().map_err(|e| AppError::PermissionDenied {
                reason: format!("invalid filesystem path {path}: {e}"),
            })?;
            if !Self::is_path_allowed(&canonical, &allowed_roots) {
                return Err(AppError::PermissionDenied {
                    reason: format!(
                        "filesystem path {} is outside allowed workspace roots",
                        canonical.display()
                    ),
                });
            }
            return Ok((canonical.clone(), canonical));
        }

        let parent = target.parent().ok_or_else(|| AppError::PermissionDenied {
            reason: format!("filesystem path {path} has no parent directory"),
        })?;

        let canonical_parent = parent.canonicalize().map_err(|e| AppError::PermissionDenied {
            reason: format!("invalid parent directory for {path}: {e}"),
        })?;

        if !Self::is_path_allowed(&canonical_parent, &allowed_roots) {
            return Err(AppError::PermissionDenied {
                reason: format!(
                    "parent directory {} is outside allowed workspace roots",
                    canonical_parent.display()
                ),
            });
        }

        let file_name = target.file_name().ok_or_else(|| AppError::PermissionDenied {
            reason: format!("filesystem path {path} has no file name"),
        })?;
        let resolved_target = canonical_parent.join(file_name);
        Ok((resolved_target, canonical_parent))
    }

    fn check_write_permission(&self, workspace_id: WorkspaceId, resource: &str) -> PermissionResult {
        let request = PermissionRequest {
            workspace_id,
            thread_id: None,
            run_id: None,
            action: "filesystem.write".to_owned(),
            resource: resource.to_owned(),
            trusted_workspace: true,
        };
        self.security.permission_engine().evaluate(request)
    }

    fn require_write_permission(
        &self,
        workspace_id: WorkspaceId,
        resource: &str,
    ) -> Result<(), AppError> {
        let result = self.check_write_permission(workspace_id, resource);
        if !matches!(result, PermissionResult::Allowed) {
            self.audit(workspace_id, resource, "filesystem.write", &result);
            let reason = match result {
                PermissionResult::Allowed => unreachable!(),
                PermissionResult::Ask { ref request } => {
                    format!("approval required for {} on {}", request.action, request.resource)
                }
                PermissionResult::Denied { ref reason } => reason.clone(),
            };
            return Err(AppError::PermissionDenied { reason });
        }
        Ok(())
    }

    fn audit(
        &self,
        workspace_id: WorkspaceId,
        resource: &str,
        action: &str,
        outcome: &PermissionResult,
    ) {
        let _ = self.security.audit_logger().log(AuditEvent {
            workspace_id,
            thread_id: None,
            run_id: None,
            action: action.to_owned(),
            resource: resource.to_owned(),
            outcome: outcome.clone(),
        });
    }

    async fn search_recursive(
        &self,
        root: &Path,
        current: &Path,
        pattern: &str,
        mode: SearchMode,
        results: &mut Vec<SearchResult>,
    ) -> Result<(), AppError> {
        if results.len() >= SEARCH_LIMIT {
            return Ok(());
        }

        let mut read_dir = tokio::fs::read_dir(current).await.map_err(|e| AppError::Internal {
            message: format!("failed to read directory {}: {e}", current.display()),
        })?;

        while let Some(entry) = read_dir.next_entry().await.map_err(|e| AppError::Internal {
            message: format!("failed to read directory entry: {e}"),
        })? {
            if results.len() >= SEARCH_LIMIT {
                break;
            }

            let path = entry.path();
            let file_type = entry.file_type().await.ok();

            if file_type.as_ref().is_some_and(std::fs::FileType::is_dir) {
                Box::pin(self.search_recursive(root, &path, pattern, mode, results)).await?;
                continue;
            }

            if !file_type.as_ref().is_some_and(std::fs::FileType::is_file) {
                continue;
            }

            let relative = path.strip_prefix(root).map_or_else(
                |_| path.to_string_lossy().to_string(),
                |p| p.to_string_lossy().to_string(),
            );

            match mode {
                SearchMode::FilenameGlob => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if matches_glob(pattern, &name) {
                        results.push(SearchResult {
                            path: relative,
                            kind: SearchResultKind::FilenameMatch,
                        });
                    }
                }
                SearchMode::ContentSubstring => {
                    match tokio::fs::read_to_string(&path).await {
                        Ok(content) if content.contains(pattern) => {
                            results.push(SearchResult {
                                path: relative,
                                kind: SearchResultKind::ContentMatch,
                            });
                        }
                        Ok(_) | Err(_) => {
                            // No match or unreadable/binary file; skip silently.
                        }
                    }
                }
            }
        }

        Ok(())
    }
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

/// Kind of directory entry returned by [`FilesystemTool::list_dir`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
    /// Other or unknown.
    Unknown,
}

/// A single entry returned by [`FilesystemTool::list_dir`].
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// File or directory name.
    pub name: String,
    /// Kind of entry.
    pub kind: EntryKind,
}

/// Search mode for [`FilesystemTool::search_files`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Match filenames against a glob pattern.
    FilenameGlob,
    /// Match file contents against a substring.
    ContentSubstring,
}

/// A single search hit.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Path of the matching file, relative to the search root.
    pub path: String,
    /// Kind of match.
    pub kind: SearchResultKind,
}

/// Kind of search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchResultKind {
    /// Filename matched the glob pattern.
    FilenameMatch,
    /// File content contained the substring.
    ContentMatch,
}

impl EntryKind {
    /// Return the string representation of the entry kind.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Unknown => "unknown",
        }
    }
}

impl SearchResultKind {
    /// Return the string representation of the search result kind.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::FilenameMatch => "filename_match",
            Self::ContentMatch => "content_match",
        }
    }
}

/// Simple glob matcher supporting `*` (any characters) and `?` (single
/// character). Character classes `[abc]` are not supported.
fn matches_glob(pattern: &str, value: &str) -> bool {
    let mut chars = value.chars();
    let mut pattern_chars = pattern.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                let next = pattern_chars.peek().copied();
                if next.is_none() {
                    return true;
                }
                // Greedy match up to the next pattern character.
                loop {
                    match chars.next() {
                        Some(c) if c == next.unwrap() => {
                            // Lookahead: ensure the rest matches.
                            let remaining_value: String = std::iter::once(c)
                                .chain(chars.clone())
                                .collect();
                            let remaining_pattern: String = pattern_chars.clone().collect();
                            if matches_glob(&remaining_pattern, &remaining_value) {
                                return true;
                            }
                        }
                        Some(_) => {}
                        None => return false,
                    }
                }
            }
            '?' => {
                if chars.next().is_none() {
                    return false;
                }
            }
            c => {
                if chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    chars.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_security::{
        DefaultCommandPolicy, DefaultNetworkPolicy, MemoryAuditLogger,
        PermissionEngine, PermissionRequest, PermissionResult,
    };

    struct AllowAllEngine;

    impl PermissionEngine for AllowAllEngine {
        fn evaluate(&self, _request: PermissionRequest) -> PermissionResult {
            PermissionResult::Allowed
        }
    }

    struct DenyAllEngine;

    impl PermissionEngine for DenyAllEngine {
        fn evaluate(&self, _request: PermissionRequest) -> PermissionResult {
            PermissionResult::Denied {
                reason: "denied".to_owned(),
            }
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

    fn test_fs(allowed_roots: Vec<PathBuf>) -> FilesystemTool {
        FilesystemTool::new(test_security(), Arc::new(StaticRepoRootProvider { roots: Vec::new() }))
            .with_allowed_roots(allowed_roots)
    }

    #[tokio::test]
    async fn read_file_returns_content() {
        let tmp = std::env::temp_dir().join(format!(
            "portico-fs-read-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("hello.txt");
        std::fs::write(&file, "world").unwrap();

        let fs = test_fs(vec![tmp.clone()]);
        let content = fs
            .read_file(WorkspaceId::default(), &file.to_string_lossy())
            .await
            .expect("read file");
        assert_eq!(content, "world");
    }

    #[tokio::test]
    async fn read_file_rejects_path_outside_roots() {
        let allowed = std::env::temp_dir().join(format!(
            "portico-fs-allowed-{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "portico-fs-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let file = outside.join("secret.txt");
        std::fs::write(&file, "x").unwrap();

        let fs = test_fs(vec![allowed]);
        let result = fs
            .read_file(WorkspaceId::default(), &file.to_string_lossy())
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn write_file_creates_and_updates_content() {
        let tmp = std::env::temp_dir().join(format!(
            "portico-fs-write-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("new.txt");

        let fs = test_fs(vec![tmp.clone()]);
        fs.write_file(WorkspaceId::default(), &file.to_string_lossy(), "first")
            .await
            .expect("write file");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "first");

        fs.write_file(WorkspaceId::default(), &file.to_string_lossy(), "second")
            .await
            .expect("overwrite file");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "second");
    }

    #[tokio::test]
    async fn write_file_rejects_path_outside_roots() {
        let allowed = std::env::temp_dir().join(format!(
            "portico-fs-wallowed-{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "portico-fs-woutside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let file = outside.join("evil.txt");

        let fs = test_fs(vec![allowed]);
        let result = fs
            .write_file(WorkspaceId::default(), &file.to_string_lossy(), "x")
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn edit_file_replaces_unique_string() {
        let tmp = std::env::temp_dir().join(format!(
            "portico-fs-edit-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("doc.txt");
        std::fs::write(&file, "hello world").unwrap();

        let fs = test_fs(vec![tmp.clone()]);
        fs.edit_file(
            WorkspaceId::default(),
            &file.to_string_lossy(),
            "world",
            "Portico",
        )
        .await
        .expect("edit file");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello Portico");
    }

    #[tokio::test]
    async fn edit_file_rejects_non_unique_old_string() {
        let tmp = std::env::temp_dir().join(format!(
            "portico-fs-edit-dup-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("dup.txt");
        std::fs::write(&file, "aaa aaa").unwrap();

        let fs = test_fs(vec![tmp.clone()]);
        let result = fs
            .edit_file(WorkspaceId::default(), &file.to_string_lossy(), "aaa", "b")
            .await;
        assert!(matches!(result, Err(AppError::Internal { .. })));
        assert!(
            result.unwrap_err().to_string().contains("not unique"),
            "expected uniqueness error"
        );
    }

    #[tokio::test]
    async fn edit_file_rejects_path_outside_roots() {
        let allowed = std::env::temp_dir().join(format!(
            "portico-fs-eallowed-{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "portico-fs-eoutside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let file = outside.join("x.txt");
        std::fs::write(&file, "x").unwrap();

        let fs = test_fs(vec![allowed]);
        let result = fs
            .edit_file(WorkspaceId::default(), &file.to_string_lossy(), "x", "y")
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn list_dir_returns_entries() {
        let tmp = std::env::temp_dir().join(format!(
            "portico-fs-list-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("a.txt"), "a").unwrap();
        std::fs::create_dir(tmp.join("sub")).unwrap();

        let fs = test_fs(vec![tmp.clone()]);
        let entries = fs
            .list_dir(WorkspaceId::default(), &tmp.to_string_lossy())
            .await
            .expect("list dir");
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.name == "a.txt" && e.kind == EntryKind::File));
        assert!(entries.iter().any(|e| e.name == "sub" && e.kind == EntryKind::Directory));
    }

    #[tokio::test]
    async fn list_dir_rejects_path_outside_roots() {
        let allowed = std::env::temp_dir().join(format!(
            "portico-fs-lallowed-{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "portico-fs-loutside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let fs = test_fs(vec![allowed]);
        let result = fs.list_dir(WorkspaceId::default(), &outside.to_string_lossy()).await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn search_files_finds_by_glob_and_content() {
        let tmp = std::env::temp_dir().join(format!(
            "portico-fs-search-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("alpha.rs"), "fn alpha() {}").unwrap();
        std::fs::write(tmp.join("beta.txt"), "beta content").unwrap();

        let fs = test_fs(vec![tmp.clone()]);
        let glob_results = fs
            .search_files(
                WorkspaceId::default(),
                &tmp.to_string_lossy(),
                "*.rs",
                SearchMode::FilenameGlob,
            )
            .await
            .expect("search glob");
        assert_eq!(glob_results.len(), 1);
        assert_eq!(glob_results[0].path, "alpha.rs");

        let content_results = fs
            .search_files(
                WorkspaceId::default(),
                &tmp.to_string_lossy(),
                "content",
                SearchMode::ContentSubstring,
            )
            .await
            .expect("search content");
        assert_eq!(content_results.len(), 1);
        assert_eq!(content_results[0].path, "beta.txt");
    }

    #[tokio::test]
    async fn search_files_rejects_path_outside_roots() {
        let allowed = std::env::temp_dir().join(format!(
            "portico-fs-sallowed-{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "portico-fs-soutside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let fs = test_fs(vec![allowed]);
        let result = fs
            .search_files(
                WorkspaceId::default(),
                &outside.to_string_lossy(),
                "*",
                SearchMode::FilenameGlob,
            )
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn write_file_denied_by_permission_engine() {
        let audit = Arc::new(MemoryAuditLogger::new());
        let security = Arc::new(SecurityContext::new(
            Arc::new(DenyAllEngine),
            Arc::new(DefaultCommandPolicy::new()),
            Arc::new(DefaultNetworkPolicy::new()),
            audit.clone(),
        ));
        let tmp = std::env::temp_dir().join(format!(
            "portico-fs-perm-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("denied.txt");

        let fs = FilesystemTool::new(
            security,
            Arc::new(StaticRepoRootProvider { roots: Vec::new() }),
        )
        .with_allowed_roots(vec![tmp.clone()]);

        let result = fs
            .write_file(WorkspaceId::default(), &file.to_string_lossy(), "x")
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));

        let events = audit.events().expect("read events");
        assert!(events.iter().any(|e| e.action == "filesystem.write"));
    }

    #[test]
    fn glob_matcher_works() {
        assert!(matches_glob("*.rs", "main.rs"));
        assert!(!matches_glob("*.rs", "main.txt"));
        assert!(matches_glob("?at", "cat"));
        assert!(!matches_glob("?at", "chat"));
        assert!(matches_glob("*test*", "this_is_a_test_file"));
    }
}
