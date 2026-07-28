//! Small, product-owned tool executor for the first safe golden path.

use crate::{ExecutionGrant, PolicyGate};
use app_models::{AppError, ToolInvocation};
use app_security::{CommandPolicy, DefaultCommandPolicy};
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
            ("git", "git.write") => git_write(invocation).await,
            ("shell_exec", "shell.exec") => shell_exec(invocation).await,
            ("web_fetch", "network.fetch") => web_fetch(invocation).await,
            ("web_search", "network.search") => web_search(invocation).await,
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
    let args: Vec<String> = match subcommand {
        "status" => vec!["status".into(), "--short".into()],
        "diff" => vec!["diff".into(), "--no-ext-diff".into()],
        _ => {
            return Err(AppError::PermissionDenied {
                reason: "git.read only supports status and diff".to_owned(),
            });
        }
    };
    run_git(&repo, &args).await
}

async fn git_write(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let repo = revalidate_existing_resource(invocation)?;
    if !repo.is_dir() || !repo.join(".git").exists() {
        return Err(AppError::PermissionDenied {
            reason: "git.write resource is not a repository root".to_owned(),
        });
    }
    let subcommand =
        invocation.arguments.get("subcommand").and_then(Value::as_str).ok_or_else(|| {
            AppError::PermissionDenied {
                reason: "git.write requires a subcommand".to_owned(),
            }
        })?;
    let args: Vec<String> = match subcommand {
        "add" => {
            let mut args = vec!["add".into(), "--".into()];
            let paths = invocation
                .arguments
                .get("paths")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![".".into()]);
            if paths.is_empty() {
                args.push(".".into());
            } else {
                args.extend(paths);
            }
            args
        }
        "commit" => {
            let message = invocation
                .arguments
                .get("message")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::PermissionDenied {
                    reason: "git commit requires a non-empty message".to_owned(),
                })?;
            // Prevent option injection via message: pass as single -m arg value.
            vec![
                "commit".into(),
                "-m".into(),
                message.to_owned(),
                "--no-gpg-sign".into(),
            ]
        }
        _ => {
            return Err(AppError::PermissionDenied {
                reason: "git.write only supports add and commit".to_owned(),
            });
        }
    };
    run_git(&repo, &args).await
}

async fn run_git(repo: &Path, args: &[String]) -> Result<Value, AppError> {
    let output = tokio::process::Command::new("git")
        .arg("--no-pager")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_EXTERNAL_DIFF")
        .output()
        .await
        .map_err(|e| AppError::Internal {
            message: format!("git command failed to start: {e}"),
        })?;
    if output.stdout.len().saturating_add(output.stderr.len()) > OUTPUT_LIMIT {
        return Err(AppError::PermissionDenied {
            reason: "git output exceeds 1 MiB".to_owned(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(AppError::Internal {
            message: format!(
                "git failed: {}",
                stderr.chars().take(512).collect::<String>()
            ),
        });
    }
    Ok(json!({
        "ok": true,
        "output": stdout,
        "stderr": stderr,
    }))
}

async fn shell_exec(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let command = invocation
        .arguments
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::PermissionDenied {
            reason: "shell_exec requires a non-empty command".to_owned(),
        })?;

    // Hard denylist even after approval (matches DefaultCommandPolicy intent).
    let policy = DefaultCommandPolicy::new();
    if let app_security::PermissionResult::Denied { reason } = policy.allow_command_line(command) {
        return Err(AppError::PermissionDenied { reason });
    }

    let context = {
        // Resolve workspace root for cwd defaults via the approved resource path
        // is wrong (resource is the command). Prefer explicit cwd arg, else
        // parent of any path the gate stored — use arguments only.
        invocation.arguments.clone()
    };
    let cwd_arg = context
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // Workspace root is not on ToolInvocation; recover from allowed path by
    // re-reading execution context through resource canonicalization is N/A.
    // Use absolute cwd when provided; otherwise current_dir of the process is
    // wrong. Store workspace root on invocation.resource for shell? prepared_request
    // sets resource = command. Fix: pass cwd absolute from gate...
    // For shell, re-resolve via PolicyGate is not available here.
    // Convention: if cwd is relative/empty, require the runtime to set absolute
    // in arguments during prepare. We fix prepare later to inject workspace root.
    // Temporary: look for _workspace_root in arguments (injected by prepared path).
    let workspace_root = invocation
        .arguments
        .get("_workspace_root")
        .and_then(Value::as_str)
        .map(str::to_owned);

    let cwd = match (cwd_arg, workspace_root.as_deref()) {
        (Some(cwd), Some(root)) => {
            let p = Path::new(cwd);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                Path::new(root).join(p)
            }
        }
        (Some(cwd), None) if Path::new(cwd).is_absolute() => PathBuf::from(cwd),
        (_, Some(root)) => PathBuf::from(root),
        _ => {
            return Err(AppError::PermissionDenied {
                reason: "shell_exec missing workspace root context".to_owned(),
            });
        }
    };

    if !cwd.is_dir() {
        return Err(AppError::PermissionDenied {
            reason: format!("shell_exec cwd is not a directory: {}", cwd.display()),
        });
    }

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&cwd)
        .output()
        .await
        .map_err(|e| AppError::Internal {
            message: format!("shell_exec failed to start: {e}"),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stdout.len().saturating_add(stderr.len()) > OUTPUT_LIMIT {
        return Err(AppError::PermissionDenied {
            reason: "shell_exec output exceeds 1 MiB".to_owned(),
        });
    }

    let (stdout, stdout_trunc) = truncate_chars(&stdout, 40_000);
    let (stderr, stderr_trunc) = truncate_chars(&stderr, 20_000);
    Ok(json!({
        "ok": output.status.success(),
        "exit_code": output.status.code(),
        "cwd": cwd.to_string_lossy(),
        "stdout": stdout,
        "stderr": stderr,
        "truncated": stdout_trunc || stderr_trunc,
    }))
}

const WEB_FETCH_MAX_BODY: usize = 2 * 1024 * 1024;
const WEB_FETCH_DEFAULT_CHARS: usize = 24_000;
const WEB_FETCH_MAX_CHARS: usize = 80_000;
const WEB_SEARCH_DEFAULT_RESULTS: usize = 5;
const WEB_SEARCH_MAX_RESULTS: usize = 10;
const WEB_USER_AGENT: &str =
    "PorticoDesktop/0.1 (+https://github.com/portico-ai/portico-desktop; local agent tool)";

async fn web_fetch(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let url_raw = invocation
        .arguments
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::PermissionDenied {
            reason: "web_fetch requires a non-empty url".to_owned(),
        })?;
    // Prefer approved resource (canonicalized at policy time) when present.
    let url_str = if invocation.resource.trim().is_empty() {
        url_raw
    } else {
        invocation.resource.trim()
    };
    let parsed = parse_public_http_url(url_str)?;
    let max_chars = invocation
        .arguments
        .get("max_chars")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(WEB_FETCH_DEFAULT_CHARS)
        .clamp(1_000, WEB_FETCH_MAX_CHARS);

    let client = web_http_client()?;
    let response = client
        .get(parsed.clone())
        .send()
        .await
        .map_err(|e| AppError::Internal {
            message: format!("web_fetch request failed: {e}"),
        })?;
    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    // Re-check redirect target against private-network rules.
    parse_public_http_url(&final_url)?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let bytes = response.bytes().await.map_err(|e| AppError::Internal {
        message: format!("web_fetch read body failed: {e}"),
    })?;
    if bytes.len() > WEB_FETCH_MAX_BODY {
        return Err(AppError::PermissionDenied {
            reason: format!(
                "web_fetch response exceeds {} bytes limit",
                WEB_FETCH_MAX_BODY
            ),
        });
    }
    let raw = String::from_utf8_lossy(&bytes);
    let is_html = content_type.contains("html")
        || raw.trim_start().starts_with("<!DOCTYPE")
        || raw.trim_start().starts_with("<html")
        || raw.trim_start().starts_with("<HTML");
    let title = if is_html {
        extract_html_title(&raw)
    } else {
        None
    };
    let text_source = if is_html {
        html_to_text(&raw)
    } else {
        raw.to_string()
    };
    let (text, truncated) = truncate_chars(&text_source, max_chars);
    Ok(json!({
        "ok": status < 400,
        "url": url_str,
        "final_url": final_url,
        "status": status,
        "content_type": content_type,
        "title": title,
        "text": text,
        "truncated": truncated,
        "char_count": text_source.chars().count(),
    }))
}

async fn web_search(invocation: &ToolInvocation) -> Result<Value, AppError> {
    let query = invocation
        .arguments
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::PermissionDenied {
            reason: "web_search requires a non-empty query".to_owned(),
        })?;
    let max_results = invocation
        .arguments
        .get("max_results")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(WEB_SEARCH_DEFAULT_RESULTS)
        .clamp(1, WEB_SEARCH_MAX_RESULTS);

    // DuckDuckGo HTML endpoint — no API key; results are public SERP cards.
    let search_url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding_encode(query)
    );
    parse_public_http_url(&search_url)?;
    let client = web_http_client()?;
    let response = client
        .get(&search_url)
        .send()
        .await
        .map_err(|e| AppError::Internal {
            message: format!("web_search request failed: {e}"),
        })?;
    let status = response.status().as_u16();
    if status >= 400 {
        return Err(AppError::Internal {
            message: format!("web_search upstream returned HTTP {status}"),
        });
    }
    let body = response.text().await.map_err(|e| AppError::Internal {
        message: format!("web_search read body failed: {e}"),
    })?;
    let results = parse_duckduckgo_html_results(&body, max_results);
    Ok(json!({
        "ok": true,
        "query": query,
        "result_count": results.len(),
        "results": results,
        "hint": "Use web_fetch on promising URLs to load full page text.",
    }))
}

fn web_http_client() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent(WEB_USER_AGENT)
        .build()
        .map_err(|e| AppError::Internal {
            message: format!("web http client build failed: {e}"),
        })
}

fn parse_public_http_url(raw: &str) -> Result<url::Url, AppError> {
    let parsed = url::Url::parse(raw).map_err(|e| AppError::PermissionDenied {
        reason: format!("invalid URL: {e}"),
    })?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::PermissionDenied {
            reason: "only http and https URLs are allowed".to_owned(),
        });
    }
    let host = parsed.host_str().ok_or_else(|| AppError::PermissionDenied {
        reason: "URL is missing a host".to_owned(),
    })?;
    if is_blocked_network_host(host) {
        return Err(AppError::PermissionDenied {
            reason: format!("{host} is a private/local address and is blocked"),
        });
    }
    Ok(parsed)
}

fn is_blocked_network_host(host: &str) -> bool {
    let host_lower = host.to_lowercase();
    if host_lower == "localhost"
        || host_lower == "127.0.0.1"
        || host_lower == "::1"
        || host_lower.ends_with(".local")
        || host_lower.ends_with(".localhost")
    {
        return true;
    }
    if let Ok(addr) = host.parse::<std::net::IpAddr>() {
        return match addr {
            std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
            std::net::IpAddr::V6(v6) => v6.is_loopback(),
        };
    }
    false
}

fn urlencoding_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(*b));
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0xf) as usize]));
            }
        }
    }
    out
}

fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let after = &html[start..];
    let gt = after.find('>')?;
    let rest = &after[gt + 1..];
    let end_rel = rest.to_lowercase().find("</title>")?;
    let title = rest[..end_rel].trim();
    if title.is_empty() {
        None
    } else {
        Some(decode_basic_entities(title))
    }
}

fn html_to_text(html: &str) -> String {
    let mut s = html.to_owned();
    // Drop script/style blocks (case-insensitive light pass).
    for tag in ["script", "style", "noscript"] {
        s = strip_tag_blocks(&s, tag);
    }
    // Line breaks for common block tags.
    for tag in [
        "br", "p", "div", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "section", "article",
    ] {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        s = s.replace(&close, "\n");
        // Self-closing / open tags → newline via crude replace of tag openings later.
        let _ = open;
    }
    s = s.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n");
    // Strip remaining tags.
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let decoded = decode_basic_entities(&out);
    // Collapse whitespace runs while preserving paragraph breaks.
    let mut cleaned = String::with_capacity(decoded.len());
    let mut prev_nl = 0u8;
    let mut prev_space = false;
    for ch in decoded.chars() {
        if ch == '\r' {
            continue;
        }
        if ch == '\n' {
            if prev_nl < 2 {
                cleaned.push('\n');
                prev_nl += 1;
            }
            prev_space = false;
            continue;
        }
        prev_nl = 0;
        if ch.is_whitespace() {
            if !prev_space && !cleaned.is_empty() {
                cleaned.push(' ');
                prev_space = true;
            }
            continue;
        }
        cleaned.push(ch);
        prev_space = false;
    }
    cleaned.trim().to_owned()
}

fn strip_tag_blocks(html: &str, tag: &str) -> String {
    let lower = html.to_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    let bytes = html.as_bytes();
    let lower_bytes = lower.as_bytes();
    while i < bytes.len() {
        if let Some(rel) = find_substr_from(lower_bytes, open.as_bytes(), i) {
            out.push_str(&html[i..rel]);
            if let Some(close_at) = find_substr_from(lower_bytes, close.as_bytes(), rel) {
                i = close_at + close.len();
            } else {
                // Unclosed — drop rest of open tag content by skipping to end.
                break;
            }
        } else {
            out.push_str(&html[i..]);
            break;
        }
    }
    out
}

fn find_substr_from(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from >= hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

fn decode_basic_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    let count = text.chars().count();
    if count <= max_chars {
        return (text.to_owned(), false);
    }
    let truncated: String = text.chars().take(max_chars).collect();
    (format!("{truncated}\n…[truncated]"), true)
}

/// Best-effort parse of DuckDuckGo HTML SERP cards.
fn parse_duckduckgo_html_results(html: &str, max: usize) -> Vec<Value> {
    let mut results = Vec::new();
    // Result links look like: class="result__a" href="https://...">Title</a>
    let lower = html.to_lowercase();
    let marker = "result__a";
    let mut search_from = 0;
    while results.len() < max {
        let Some(rel) = find_substr_from(lower.as_bytes(), marker.as_bytes(), search_from) else {
            break;
        };
        // Walk back to the opening <a
        let chunk_start = html[..rel].rfind("<a").unwrap_or(rel);
        let chunk = &html[chunk_start..];
        let Some(tag_end) = chunk.find('>') else {
            search_from = rel + marker.len();
            continue;
        };
        let open_tag = &chunk[..=tag_end];
        let rest = &chunk[tag_end + 1..];
        let Some(close) = rest.to_lowercase().find("</a>") else {
            search_from = rel + marker.len();
            continue;
        };
        let title = decode_basic_entities(rest[..close].trim());
        let href = extract_attr(open_tag, "href").unwrap_or_default();
        let url = normalize_ddg_href(&href);
        search_from = chunk_start + tag_end + 1 + close + 4;
        if url.is_empty() || title.is_empty() {
            continue;
        }
        if is_blocked_network_host(url::Url::parse(&url).ok().and_then(|u| u.host_str().map(str::to_owned)).as_deref().unwrap_or("")) {
            continue;
        }
        // Snippet: nearby result__snippet
        let snippet = extract_nearby_snippet(html, search_from);
        results.push(json!({
            "title": title,
            "url": url,
            "snippet": snippet,
        }));
    }
    results
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let pattern = format!("{name}=\"");
    let lower = tag.to_lowercase();
    let pat_lower = pattern.to_lowercase();
    let idx = lower.find(&pat_lower)?;
    let rest = &tag[idx + pattern.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn normalize_ddg_href(href: &str) -> String {
    // DDG often wraps as //duckduckgo.com/l/?uddg=<urlencoded>
    if let Ok(parsed) = url::Url::parse(href) {
        if parsed.host_str() == Some("duckduckgo.com") || parsed.host_str() == Some("html.duckduckgo.com") {
            if let Some(pairs) = parsed.query() {
                for pair in pairs.split('&') {
                    if let Some(v) = pair.strip_prefix("uddg=") {
                        return urlencoding_decode(v);
                    }
                }
            }
        }
        return href.to_owned();
    }
    if href.starts_with("//") {
        return format!("https:{href}");
    }
    if href.starts_with('/') {
        return String::new();
    }
    href.to_owned()
}

fn urlencoding_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = || {
                    let a = (bytes[i + 1] as char).to_digit(16)?;
                    let b = (bytes[i + 2] as char).to_digit(16)?;
                    Some(((a << 4) | b) as u8)
                };
                if let Some(byte) = h() {
                    out.push(byte);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn extract_nearby_snippet(html: &str, from: usize) -> String {
    let window_end = (from + 1200).min(html.len());
    let window = &html[from..window_end];
    let lower = window.to_lowercase();
    if let Some(idx) = lower.find("result__snippet") {
        let after = &window[idx..];
        if let Some(gt) = after.find('>') {
            let rest = &after[gt + 1..];
            if let Some(end) = rest.to_lowercase().find("</") {
                let snip = rest[..end].trim();
                let text = html_to_text(snip);
                return text.chars().take(240).collect();
            }
        }
    }
    String::new()
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
