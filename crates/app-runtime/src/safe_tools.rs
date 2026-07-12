//! Small, product-owned tool executor for the first safe golden path.

use crate::{ExecutionGrant, PolicyGate};
use app_models::{AppError, ToolInvocation};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

const FILE_LIMIT: usize = 1_048_576;
const OUTPUT_LIMIT: usize = 1_048_576;
const LIST_LIMIT: usize = 500;

/// Executes only the explicitly supported, workspace-scoped safe tool set.
#[derive(Clone)]
pub struct SafeToolExecutor {
    gate: PolicyGate,
}

/// Evidence-based restart decision for an interrupted safe tool.
#[derive(Debug, Clone)]
pub enum ToolReconciliation {
    /// The approved post-image is already present; only the DB receipt is missing.
    AlreadyApplied(Value),
    /// The approved pre-image is still present; one retry remains safe.
    SafeToRetry,
    /// External state changed and automatic replay is forbidden.
    Conflict(String),
}

impl SafeToolExecutor {
    /// Create an executor that revalidates authoritative context immediately
    /// before every side effect.
    #[must_use]
    pub const fn new(gate: PolicyGate) -> Self {
        Self { gate }
    }

    /// Execute a claimed invocation.
    ///
    /// # Errors
    ///
    /// Rejects stale workspace revisions, unsupported tools, changed file
    /// preconditions, path escapes, oversized data, or command failures.
    pub async fn execute(&self, grant: &ExecutionGrant) -> Result<Value, AppError> {
        let invocation = grant.invocation();
        let context = self.gate.execution_context(invocation.run_id).await?;
        if !context_matches_invocation(&context, invocation) {
            return Err(AppError::PermissionDenied {
                reason: "tool execution context changed after policy evaluation".to_owned(),
            });
        }

        match (invocation.tool_name.as_str(), invocation.action.as_str()) {
            ("fs_read", "filesystem.read") => read_file(invocation),
            ("fs_list", "filesystem.read") => list_directory(invocation),
            ("fs_search", "filesystem.read") => search_workspace(invocation),
            ("fs_write", "filesystem.write") => write_file_atomically(invocation),
            ("fs_edit", "filesystem.write") => edit_file_atomically(invocation),
            ("git", "git.read") => git_read(invocation).await,
            _ => Err(AppError::PermissionDenied {
                reason: "tool is not in the safe execution allowlist".to_owned(),
            }),
        }
    }

    /// Reconcile a tool abandoned in `Executing` without replaying it.
    ///
    /// # Errors
    ///
    /// Rejects unsupported recovery formats or unavailable resources.
    pub async fn reconcile(
        &self,
        invocation: &ToolInvocation,
    ) -> Result<ToolReconciliation, AppError> {
        let is_write_effect = invocation.action == "filesystem.write"
            && matches!(invocation.tool_name.as_str(), "fs_write" | "fs_edit");
        if !is_write_effect {
            return Ok(ToolReconciliation::Conflict(
                "this tool has no automatic reconciliation protocol".to_owned(),
            ));
        }
        // Restart marks the parent run terminal before receipts are repaired.
        // Reconciliation must still load workspace ownership for hash checks.
        let context = self.gate.reconciliation_context(invocation.run_id).await?;
        if !context_matches_invocation(&context, invocation) {
            return Ok(ToolReconciliation::Conflict(
                "workspace context changed while the tool was interrupted".to_owned(),
            ));
        }
        let (pre_hash, post_hash) = recovery_hashes(invocation)?;
        let target = revalidate_write_resource(invocation)?;
        let current_hash = file_state_hash(&target)?;
        if current_hash == post_hash {
            return Ok(ToolReconciliation::AlreadyApplied(json!({
                "ok": true,
                "post_hash": post_hash,
                "reconciled": true,
            })));
        }
        if current_hash == pre_hash {
            return Ok(ToolReconciliation::SafeToRetry);
        }
        Ok(ToolReconciliation::Conflict(
            "file matches neither the approved pre-image nor post-image".to_owned(),
        ))
    }
}

fn context_matches_invocation(
    context: &app_models::ExecutionContext,
    invocation: &ToolInvocation,
) -> bool {
    let ownership_matches = context.workspace_id == invocation.workspace_id
        && context.thread_id == invocation.thread_id;
    let revision_matches = context.trust_revision == invocation.context_revision;
    ownership_matches && revision_matches
}

fn read_file(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let path = revalidate_existing_resource(invocation)?;
    let metadata = path.metadata().map_err(|e| AppError::Internal {
        message: format!("read file metadata failed: {e}"),
    })?;
    if metadata.is_dir() {
        return Err(AppError::PermissionDenied {
            reason: "read target is a directory; use fs_list to list folder contents".to_owned(),
        });
    }
    if !metadata.is_file() || metadata.len() > FILE_LIMIT as u64 {
        return Err(AppError::PermissionDenied {
            reason: "read target is not a regular file within the 1 MiB limit".to_owned(),
        });
    }
    let content = std::fs::read_to_string(path).map_err(|e| AppError::Internal {
        message: format!("read UTF-8 file failed: {e}"),
    })?;
    Ok(json!({"content": content}))
}

fn list_directory(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let path = revalidate_existing_resource(invocation)?;
    let metadata = path.metadata().map_err(|e| AppError::Internal {
        message: format!("list directory metadata failed: {e}"),
    })?;
    if !metadata.is_dir() {
        return Err(AppError::PermissionDenied {
            reason: "list target is not a directory; use fs_read for files".to_owned(),
        });
    }

    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(&path).map_err(|e| AppError::Internal {
        message: format!("list directory failed: {e}"),
    })?;
    for entry in read_dir {
        if entries.len() >= LIST_LIMIT {
            break;
        }
        let entry = entry.map_err(|e| AppError::Internal {
            message: format!("list directory entry failed: {e}"),
        })?;
        let file_type = entry.file_type().map_err(|e| AppError::Internal {
            message: format!("list directory entry type failed: {e}"),
        })?;
        let kind = if file_type.is_dir() {
            "dir"
        } else if file_type.is_file() {
            "file"
        } else {
            "other"
        };
        entries.push(json!({
            "name": entry.file_name().to_string_lossy(),
            "path": entry.path().to_string_lossy(),
            "kind": kind,
        }));
    }
    entries.sort_by(|left, right| {
        let left_name = left.get("name").and_then(Value::as_str).unwrap_or_default();
        let right_name = right.get("name").and_then(Value::as_str).unwrap_or_default();
        left_name.cmp(right_name)
    });
    let truncated = entries.len() >= LIST_LIMIT;
    Ok(json!({
        "path": path.to_string_lossy(),
        "entries": entries,
        "truncated": truncated,
        "limit": LIST_LIMIT,
    }))
}

fn write_file_atomically(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let content = invocation.arguments.get("content").and_then(Value::as_str).ok_or_else(|| {
        AppError::PermissionDenied {
            reason: "approved filesystem.write payload has no string content".to_owned(),
        }
    })?;
    if content.len() > FILE_LIMIT {
        return Err(AppError::PermissionDenied {
            reason: "approved filesystem.write payload exceeds 1 MiB".to_owned(),
        });
    }
    let (pre_hash, post_hash) = recovery_hashes(invocation)?;
    atomic_replace_with_hashes(invocation, content.as_bytes(), &pre_hash, &post_hash)
}

fn edit_file_atomically(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let old_string = invocation
        .arguments
        .get("old_string")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::PermissionDenied {
            reason: "approved filesystem.write edit has no old_string".to_owned(),
        })?;
    let new_string = invocation
        .arguments
        .get("new_string")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::PermissionDenied {
            reason: "approved filesystem.write edit has no new_string".to_owned(),
        })?;
    let (pre_hash, post_hash) = recovery_hashes(invocation)?;
    let target = revalidate_write_resource(invocation)?;
    let current_hash = file_state_hash(&target)?;
    if current_hash == post_hash {
        return Ok(json!({"ok": true, "post_hash": post_hash, "already_applied": true}));
    }
    if current_hash != pre_hash {
        return Err(AppError::PermissionDenied {
            reason: "edit target changed after approval; a new preview and approval are required"
                .to_owned(),
        });
    }
    let original = std::fs::read_to_string(&target).map_err(|e| AppError::Internal {
        message: format!("read file for approved edit failed: {e}"),
    })?;
    let matches = original.matches(old_string).count();
    if matches != 1 {
        return Err(AppError::PermissionDenied {
            reason: format!("approved edit old_string must match exactly once (found {matches})"),
        });
    }
    let updated = original.replacen(old_string, new_string, 1);
    atomic_replace_with_hashes(invocation, updated.as_bytes(), &pre_hash, &post_hash)
}

fn atomic_replace_with_hashes(
    invocation: &ToolInvocation,
    content: &[u8],
    pre_hash: &str,
    post_hash: &str,
) -> Result<Value, AppError> {
    if content.len() > FILE_LIMIT {
        return Err(AppError::PermissionDenied {
            reason: "approved filesystem.write payload exceeds 1 MiB".to_owned(),
        });
    }
    let target = revalidate_write_resource(invocation)?;
    let current_hash = file_state_hash(&target)?;
    if current_hash == post_hash {
        return Ok(json!({"ok": true, "post_hash": post_hash, "already_applied": true}));
    }
    if current_hash != pre_hash {
        return Err(AppError::PermissionDenied {
            reason: "write target changed after approval; a new preview and approval are required"
                .to_owned(),
        });
    }

    let parent = target.parent().ok_or_else(|| AppError::PermissionDenied {
        reason: "approved write target has no parent".to_owned(),
    })?;
    let temporary = parent.join(format!(".portico-{}.tmp", invocation.id.0.simple()));
    let mut file =
        OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e| {
            AppError::Internal {
                message: format!("create atomic write temporary file failed: {e}"),
            }
        })?;
    let write_result = (|| -> Result<(), AppError> {
        file.write_all(content).map_err(|e| AppError::Internal {
            message: format!("write atomic temporary file failed: {e}"),
        })?;
        file.sync_all().map_err(|e| AppError::Internal {
            message: format!("sync atomic temporary file failed: {e}"),
        })?;
        std::fs::rename(&temporary, &target).map_err(|e| AppError::Internal {
            message: format!("replace approved file atomically failed: {e}"),
        })?;
        OpenOptions::new()
            .read(true)
            .open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| AppError::Internal {
                message: format!("sync approved file parent directory failed: {e}"),
            })?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    write_result?;

    if file_state_hash(&target)? != post_hash {
        return Err(AppError::Internal {
            message: "atomic file write did not produce the approved post-image".to_owned(),
        });
    }
    Ok(json!({"ok": true, "post_hash": post_hash, "already_applied": false}))
}

fn search_workspace(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let root = revalidate_existing_resource(invocation)?;
    let metadata = root.metadata().map_err(|e| AppError::Internal {
        message: format!("search root metadata failed: {e}"),
    })?;
    if !metadata.is_dir() {
        return Err(AppError::PermissionDenied {
            reason: "search root must be a directory".to_owned(),
        });
    }
    let pattern = invocation
        .arguments
        .get("pattern")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if pattern.is_empty() {
        return Err(AppError::PermissionDenied {
            reason: "fs_search requires a non-empty pattern".to_owned(),
        });
    }
    let mode = invocation
        .arguments
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("content");
    let mut hits = Vec::new();
    search_recursive(&root, &root, pattern, mode, 0, &mut hits)?;
    let truncated = hits.len() >= SEARCH_LIMIT;
    Ok(json!({
        "path": root.to_string_lossy(),
        "mode": mode,
        "pattern": pattern,
        "hits": hits,
        "truncated": truncated,
        "limit": SEARCH_LIMIT,
    }))
}

const SEARCH_LIMIT: usize = 50;
const SEARCH_MAX_DEPTH: usize = 10;

fn search_recursive(
    root: &Path,
    current: &Path,
    pattern: &str,
    mode: &str,
    depth: usize,
    hits: &mut Vec<Value>,
) -> Result<(), AppError> {
    if hits.len() >= SEARCH_LIMIT || depth > SEARCH_MAX_DEPTH {
        return Ok(());
    }
    let read_dir = std::fs::read_dir(current).map_err(|e| AppError::Internal {
        message: format!("search directory failed: {e}"),
    })?;
    for entry in read_dir {
        if hits.len() >= SEARCH_LIMIT {
            break;
        }
        let entry = entry.map_err(|e| AppError::Internal {
            message: format!("search entry failed: {e}"),
        })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "node_modules" || name == "target" || name == "dist" {
            continue;
        }
        let file_type = entry.file_type().map_err(|e| AppError::Internal {
            message: format!("search entry type failed: {e}"),
        })?;
        if file_type.is_dir() {
            search_recursive(root, &path, pattern, mode, depth + 1, hits)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        if mode == "glob" {
            if glob_match(pattern, &name) || glob_match(pattern, &relative) {
                hits.push(json!({
                    "path": relative,
                    "kind": "filename",
                }));
            }
        } else {
            let meta = entry.metadata().map_err(|e| AppError::Internal {
                message: format!("search file metadata failed: {e}"),
            })?;
            if meta.len() == 0 || meta.len() > FILE_LIMIT as u64 {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if content.contains(pattern) {
                let line = content
                    .lines()
                    .position(|line| line.contains(pattern))
                    .map(|idx| idx + 1);
                hits.push(json!({
                    "path": relative,
                    "kind": "content",
                    "line": line,
                }));
            }
        }
    }
    Ok(())
}

/// Minimal glob: `*` any chars, `?` one char; case-sensitive.
fn glob_match(pattern: &str, value: &str) -> bool {
    fn rec(p: &[u8], v: &[u8]) -> bool {
        match (p.first(), v.first()) {
            (None, None) => true,
            (Some(b'*'), _) => {
                for i in 0..=v.len() {
                    if rec(&p[1..], &v[i..]) {
                        return true;
                    }
                }
                false
            }
            (Some(b'?'), Some(_)) => rec(&p[1..], &v[1..]),
            (Some(a), Some(b)) if a == b => rec(&p[1..], &v[1..]),
            _ => false,
        }
    }
    rec(pattern.as_bytes(), value.as_bytes())
}

async fn git_read(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let repo = revalidate_existing_resource(invocation)?;
    if !repo.is_dir() || !repo.join(".git").exists() {
        return Err(AppError::PermissionDenied {
            reason: "git.read resource is not a repository root".to_owned(),
        });
    }
    let subcommand =
        invocation.arguments.get("subcommand").and_then(Value::as_str).ok_or_else(|| {
            AppError::PermissionDenied {
                reason: "git.read requires a subcommand".to_owned(),
            }
        })?;
    let args: &[&str] = match subcommand {
        "status" => &["status", "--short"],
        "diff" => &["diff", "--no-ext-diff"],
        _ => {
            return Err(AppError::PermissionDenied {
                reason: "only git status and diff are enabled".to_owned(),
            });
        }
    };
    let output = tokio::process::Command::new("git")
        .arg("--no-pager")
        .arg("-C")
        .arg(&repo)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_EXTERNAL_DIFF")
        .output()
        .await
        .map_err(|e| AppError::Internal {
            message: format!("git read command failed to start: {e}"),
        })?;
    if output.stdout.len().saturating_add(output.stderr.len()) > OUTPUT_LIMIT {
        return Err(AppError::PermissionDenied {
            reason: "git read output exceeds 1 MiB".to_owned(),
        });
    }
    if !output.status.success() {
        return Err(AppError::Internal {
            message: format!(
                "git read failed: {}",
                String::from_utf8_lossy(&output.stderr).chars().take(512).collect::<String>()
            ),
        });
    }
    Ok(json!({"output": String::from_utf8_lossy(&output.stdout)}))
}

fn revalidate_existing_resource(invocation: &ToolInvocation) -> Result<PathBuf, AppError> {
    let canonical =
        Path::new(&invocation.resource)
            .canonicalize()
            .map_err(|e| AppError::PermissionDenied {
                reason: format!("approved resource is unavailable: {e}"),
            })?;
    if canonical != Path::new(&invocation.resource) {
        return Err(AppError::PermissionDenied {
            reason: "approved resource changed after policy evaluation".to_owned(),
        });
    }
    Ok(canonical)
}

fn revalidate_write_resource(invocation: &ToolInvocation) -> Result<PathBuf, AppError> {
    let target = Path::new(&invocation.resource);
    if target.exists() {
        return revalidate_existing_resource(invocation);
    }
    let parent = target.parent().ok_or_else(|| AppError::PermissionDenied {
        reason: "approved write resource has no parent".to_owned(),
    })?;
    let parent = parent.canonicalize().map_err(|e| AppError::PermissionDenied {
        reason: format!("approved write parent is unavailable: {e}"),
    })?;
    let resolved = parent.join(
        target.file_name().ok_or_else(|| AppError::PermissionDenied {
            reason: "approved write resource has no file name".to_owned(),
        })?,
    );
    if resolved != target {
        return Err(AppError::PermissionDenied {
            reason: "approved write resource changed after policy evaluation".to_owned(),
        });
    }
    Ok(resolved)
}

fn recovery_hashes(invocation: &ToolInvocation) -> Result<(String, String), AppError> {
    let recovery = invocation.recovery.as_ref().ok_or_else(|| AppError::PermissionDenied {
        reason: "approved filesystem.write has no recovery receipt".to_owned(),
    })?;
    let kind = recovery.get("kind").and_then(Value::as_str).unwrap_or("");
    if !matches!(kind, "file_replace_v1" | "file_edit_v1") {
        return Err(AppError::PermissionDenied {
            reason: "approved filesystem.write recovery receipt is unsupported".to_owned(),
        });
    }
    let pre_hash = recovery.get("pre_hash").and_then(Value::as_str).ok_or_else(|| {
        AppError::PermissionDenied {
            reason: "approved filesystem.write has no pre-image hash".to_owned(),
        }
    })?;
    let post_hash = recovery.get("post_hash").and_then(Value::as_str).ok_or_else(|| {
        AppError::PermissionDenied {
            reason: "approved filesystem.write has no post-image hash".to_owned(),
        }
    })?;
    Ok((pre_hash.to_owned(), post_hash.to_owned()))
}

fn file_state_hash(path: &Path) -> Result<String, AppError> {
    if !path.exists() {
        return Ok("missing".to_owned());
    }
    let bytes = std::fs::read(path).map_err(|e| AppError::Internal {
        message: format!("read approved file state failed: {e}"),
    })?;
    if bytes.len() > FILE_LIMIT {
        return Err(AppError::PermissionDenied {
            reason: "approved file state exceeds 1 MiB".to_owned(),
        });
    }
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
