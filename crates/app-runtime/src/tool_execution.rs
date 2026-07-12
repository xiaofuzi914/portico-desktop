//! Authoritative execution context and central policy gate for product tools.

use crate::Storage;
use app_models::{
    AgentRunId, AppError, ApprovalRequest, ApprovalRequestId, ApprovalRequestStatus,
    ExecutionContext, ToolInvocation, ToolInvocationId, ToolInvocationStatus,
};
use app_security::{PermissionRequest, PermissionResult, SecurityContext};
use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Current policy snapshot written into every invocation fingerprint.
pub const POLICY_VERSION: &str = "portico-policy-v1";

/// A product-owned tool request after schema validation but before policy.
#[derive(Debug, Clone)]
pub struct PreparedToolRequest {
    /// Stable provider/model tool-call id when one exists.
    pub model_call_id: Option<String>,
    /// Registered product tool name.
    pub tool_name: String,
    /// Tool implementation/version snapshot.
    pub tool_version: String,
    /// Policy action.
    pub action: String,
    /// Requested resource. Filesystem actions must provide an absolute path.
    pub resource: String,
    /// Validated immutable arguments.
    pub arguments: Value,
    /// Tool-specific reconciliation metadata.
    pub recovery: Option<Value>,
}

/// Durable result of preparing and evaluating a tool request.
#[derive(Debug, Clone)]
pub enum ToolGateOutcome {
    /// The persisted invocation may be claimed immediately.
    Ready(ToolInvocation),
    /// The persisted invocation is linked to a pending approval.
    WaitingApproval {
        /// Durable invocation.
        invocation: ToolInvocation,
        /// Durable approval request.
        approval: ApprovalRequest,
    },
    /// Policy denied the persisted invocation.
    Denied(ToolInvocation),
}

/// Single runtime entry point for context resolution and tool policy decisions.
#[derive(Clone)]
pub struct PolicyGate {
    storage: Arc<dyn Storage>,
    security: Arc<SecurityContext>,
}

impl PolicyGate {
    /// Build a gate over backend-owned storage and policy state.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, security: Arc<SecurityContext>) -> Self {
        Self { storage, security }
    }

    /// Resolve an authoritative execution context from a durable run.
    ///
    /// # Errors
    ///
    /// Rejects missing/terminal runs, ownership mismatches, or a workspace root
    /// that can no longer be canonicalized to the persisted path.
    pub async fn execution_context(
        &self,
        run_id: AgentRunId,
    ) -> Result<ExecutionContext, AppError> {
        self.execution_context_inner(run_id, false).await
    }

    /// Resolve context for post-restart tool reconciliation.
    ///
    /// Unlike [`Self::execution_context`], this allows terminal runs. Restart
    /// recovery marks incomplete runs as `Interrupted` before durable tool
    /// receipts are reconciled; reconciliation only verifies ownership and
    /// workspace identity so an already-applied side effect can be receipted.
    ///
    /// # Errors
    ///
    /// Rejects missing runs, ownership mismatches, or a workspace root that can
    /// no longer be canonicalized to the persisted path.
    pub async fn reconciliation_context(
        &self,
        run_id: AgentRunId,
    ) -> Result<ExecutionContext, AppError> {
        self.execution_context_inner(run_id, true).await
    }

    async fn execution_context_inner(
        &self,
        run_id: AgentRunId,
        allow_terminal: bool,
    ) -> Result<ExecutionContext, AppError> {
        let run = self.storage.get_run(run_id).await?;
        if !allow_terminal && run.status.is_terminal() {
            return Err(AppError::PermissionDenied {
                reason: "terminal runs cannot invoke tools".to_owned(),
            });
        }
        let thread = self.storage.get_thread(run.thread_id).await?;
        if thread.workspace_id != run.workspace_id {
            return Err(AppError::PermissionDenied {
                reason: "run, thread, and workspace ownership are inconsistent".to_owned(),
            });
        }
        let mut workspace = self.storage.get_workspace(run.workspace_id).await?;
        // Trusted projects always get project-root read/write when allowlists are
        // still empty (covers workspaces trusted before write defaults existed).
        if workspace.trusted
            && (workspace.allowed_read_paths.is_empty() || workspace.allowed_write_paths.is_empty())
        {
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
            self.storage
                .set_workspace_allowed_paths(workspace.id, read_paths, write_paths)
                .await?;
            workspace = self.storage.get_workspace(run.workspace_id).await?;
        }
        let root = Path::new(&workspace.root_path).canonicalize().map_err(|e| {
            AppError::PermissionDenied {
                reason: format!("workspace root is unavailable: {e}"),
            }
        })?;
        if root != Path::new(&workspace.root_path) {
            return Err(AppError::PermissionDenied {
                reason: "persisted workspace root is no longer canonical".to_owned(),
            });
        }

        Ok(ExecutionContext {
            run_id,
            thread_id: run.thread_id,
            workspace_id: run.workspace_id,
            canonical_workspace_root: workspace.root_path,
            trusted_workspace: workspace.trusted,
            allowed_read_paths: workspace.allowed_read_paths,
            allowed_write_paths: workspace.allowed_write_paths,
            trust_revision: workspace.updated_at,
            correlation_id: uuid::Uuid::new_v4(),
        })
    }

    /// Canonicalize, sandbox, evaluate, and durably persist one tool request.
    ///
    /// # Errors
    ///
    /// Rejects invalid paths, stale ownership, duplicate model call ids, or a
    /// persistence failure. Policy denials are returned as [`ToolGateOutcome::Denied`].
    pub async fn prepare(
        &self,
        run_id: AgentRunId,
        request: PreparedToolRequest,
    ) -> Result<ToolGateOutcome, AppError> {
        let context = self.execution_context(run_id).await?;
        let resource = canonicalize_resource(&context, &request.action, &request.resource)?;
        let recovery = prepare_recovery(&request.action, &resource, &request.arguments)?;
        let permission_request = PermissionRequest {
            workspace_id: context.workspace_id,
            thread_id: Some(context.thread_id),
            run_id: Some(context.run_id),
            action: request.action.clone(),
            resource: resource.clone(),
            trusted_workspace: context.trusted_workspace,
        };
        let decision = self.security.permissions.evaluate(permission_request);
        let now = Utc::now();
        let mut invocation = ToolInvocation {
            id: ToolInvocationId::new(),
            run_id: context.run_id,
            thread_id: context.thread_id,
            workspace_id: context.workspace_id,
            model_call_id: request.model_call_id,
            tool_name: request.tool_name,
            tool_version: request.tool_version,
            action: request.action,
            resource,
            arguments: request.arguments,
            request_hash: String::new(),
            policy_version: POLICY_VERSION.to_owned(),
            context_revision: context.trust_revision,
            status: ToolInvocationStatus::Denied,
            approval_request_id: None,
            result: None,
            error: None,
            recovery: recovery.or(request.recovery),
            lease_token: None,
            attempts: 0,
            cancel_requested: false,
            correlation_id: context.correlation_id,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
        };
        invocation.request_hash = invocation_hash(&context, &invocation)?;

        match decision {
            PermissionResult::Allowed => {
                invocation.status = ToolInvocationStatus::Ready;
                let invocation = self.storage.create_tool_invocation(&invocation).await?;
                self.audit(&invocation, "Allowed", None).await?;
                Ok(ToolGateOutcome::Ready(invocation))
            }
            PermissionResult::Ask { .. } => {
                invocation.status = ToolInvocationStatus::WaitingApproval;
                let approval = ApprovalRequest {
                    id: ApprovalRequestId(0),
                    run_id: context.run_id,
                    workspace_id: context.workspace_id,
                    thread_id: context.thread_id,
                    action: invocation.action.clone(),
                    resource: invocation.resource.clone(),
                    status: ApprovalRequestStatus::Pending,
                    created_at: now,
                    resolved_at: None,
                    resolution_reason: None,
                };
                let (invocation, approval) = self
                    .storage
                    .create_tool_invocation_with_approval(&invocation, &approval)
                    .await?;
                self.audit(&invocation, "Ask", Some("approval required")).await?;
                Ok(ToolGateOutcome::WaitingApproval {
                    invocation,
                    approval,
                })
            }
            PermissionResult::Denied { reason } => {
                invocation.status = ToolInvocationStatus::Denied;
                invocation.error = Some(safe_summary(&reason));
                invocation.completed_at = Some(now);
                let invocation = self.storage.create_tool_invocation(&invocation).await?;
                self.audit(&invocation, "Denied", Some(&reason)).await?;
                Ok(ToolGateOutcome::Denied(invocation))
            }
        }
    }

    async fn audit(
        &self,
        invocation: &ToolInvocation,
        decision: &str,
        reason: Option<&str>,
    ) -> Result<(), AppError> {
        self.storage
            .append_audit_log(
                invocation.workspace_id,
                Some(invocation.thread_id),
                Some(invocation.run_id),
                &invocation.action,
                &invocation.resource,
                decision,
                reason.map(safe_summary).as_deref(),
            )
            .await
    }
}

fn canonicalize_resource(
    context: &ExecutionContext,
    action: &str,
    resource: &str,
) -> Result<String, AppError> {
    if !matches!(action, "filesystem.read" | "filesystem.write" | "git.read") {
        return Ok(resource.to_owned());
    }
    let target = resolve_workspace_path(context, resource);
    let canonical = if action == "filesystem.write" && !target.exists() {
        let parent = target.parent().ok_or_else(|| AppError::PermissionDenied {
            reason: "write resource has no parent".to_owned(),
        })?;
        let canonical_parent = parent.canonicalize().map_err(|e| AppError::PermissionDenied {
            reason: format!("write resource parent is unavailable: {e}"),
        })?;
        canonical_parent.join(
            target.file_name().ok_or_else(|| AppError::PermissionDenied {
                reason: "write resource has no file name".to_owned(),
            })?,
        )
    } else {
        target.canonicalize().map_err(|e| AppError::PermissionDenied {
            reason: format!("tool resource is unavailable: {e}"),
        })?
    };

    let root = Path::new(&context.canonical_workspace_root);
    if !canonical.starts_with(root) {
        return Err(AppError::PermissionDenied {
            reason: "tool resource is outside the owning workspace".to_owned(),
        });
    }
    let allowed = if action == "filesystem.write" {
        &context.allowed_write_paths
    } else {
        &context.allowed_read_paths
    };
    if !allowed.iter().map(PathBuf::from).any(|path| canonical.starts_with(path)) {
        return Err(AppError::PermissionDenied {
            reason: "tool resource is outside the workspace allowlist".to_owned(),
        });
    }
    Ok(canonical.to_string_lossy().into_owned())
}

/// Resolve absolute paths as-is; resolve relative paths against the workspace root.
///
/// Empty paths and `"."` map to the workspace root so list/read tools work without
/// the model inventing an absolute path.
fn resolve_workspace_path(context: &ExecutionContext, resource: &str) -> PathBuf {
    let trimmed = resource.trim();
    let root = PathBuf::from(&context.canonical_workspace_root);
    if trimmed.is_empty() || trimmed == "." {
        return root;
    }
    let target = Path::new(trimmed);
    if target.is_absolute() {
        return target.to_path_buf();
    }
    root.join(target)
}

fn invocation_hash(
    context: &ExecutionContext,
    invocation: &ToolInvocation,
) -> Result<String, AppError> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "run_id": context.run_id.0,
        "thread_id": context.thread_id.0,
        "workspace_id": context.workspace_id.0,
        "trust_revision": context.trust_revision,
        "model_call_id": invocation.model_call_id,
        "tool_name": invocation.tool_name,
        "tool_version": invocation.tool_version,
        "action": invocation.action,
        "resource": invocation.resource,
        "arguments": invocation.arguments,
        "recovery": invocation.recovery,
        "policy_version": invocation.policy_version,
    }))
    .map_err(|e| AppError::Internal {
        message: format!("serialize tool invocation fingerprint failed: {e}"),
    })?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

fn safe_summary(message: &str) -> String {
    message.chars().take(512).collect()
}

fn prepare_recovery(
    action: &str,
    resource: &str,
    arguments: &Value,
) -> Result<Option<Value>, AppError> {
    if action != "filesystem.write" {
        return Ok(None);
    }
    let path = Path::new(resource);

    // Full file replace (fs_write).
    if let Some(content) = arguments.get("content").and_then(Value::as_str) {
        if content.len() > 1_048_576 {
            return Err(AppError::PermissionDenied {
                reason: "filesystem.write content exceeds the 1 MiB limit".to_owned(),
            });
        }
        let pre_hash = if path.exists() {
            sha256_file(path)?
        } else {
            "missing".to_owned()
        };
        let post_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        return Ok(Some(serde_json::json!({
            "kind": "file_replace_v1",
            "pre_hash": pre_hash,
            "post_hash": post_hash,
            "size": content.len(),
        })));
    }

    // Surgical edit (fs_edit): unique old_string → new_string.
    let old_string = arguments
        .get("old_string")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::PermissionDenied {
            reason: "filesystem.write requires content or old_string/new_string".to_owned(),
        })?;
    let new_string = arguments
        .get("new_string")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::PermissionDenied {
            reason: "filesystem.write edit requires new_string".to_owned(),
        })?;
    if old_string.is_empty() {
        return Err(AppError::PermissionDenied {
            reason: "filesystem.write edit old_string must not be empty".to_owned(),
        });
    }
    if !path.exists() || !path.is_file() {
        return Err(AppError::PermissionDenied {
            reason: "filesystem.write edit target must be an existing file".to_owned(),
        });
    }
    let original = std::fs::read_to_string(path).map_err(|e| AppError::Internal {
        message: format!("read file for edit recovery failed: {e}"),
    })?;
    if original.len() > 1_048_576 {
        return Err(AppError::PermissionDenied {
            reason: "filesystem.write edit target exceeds the 1 MiB limit".to_owned(),
        });
    }
    let matches = original.matches(old_string).count();
    if matches != 1 {
        return Err(AppError::PermissionDenied {
            reason: format!(
                "filesystem.write edit old_string must match exactly once (found {matches})"
            ),
        });
    }
    let updated = original.replacen(old_string, new_string, 1);
    if updated.len() > 1_048_576 {
        return Err(AppError::PermissionDenied {
            reason: "filesystem.write edit result exceeds the 1 MiB limit".to_owned(),
        });
    }
    let pre_hash = format!("{:x}", Sha256::digest(original.as_bytes()));
    let post_hash = format!("{:x}", Sha256::digest(updated.as_bytes()));
    Ok(Some(serde_json::json!({
        "kind": "file_edit_v1",
        "pre_hash": pre_hash,
        "post_hash": post_hash,
        "size": updated.len(),
        "old_string": old_string,
        "new_string": new_string,
    })))
}

fn sha256_file(path: &Path) -> Result<String, AppError> {
    let bytes = std::fs::read(path).map_err(|e| AppError::Internal {
        message: format!("read file for recovery fingerprint failed: {e}"),
    })?;
    if bytes.len() > 1_048_576 {
        return Err(AppError::PermissionDenied {
            reason: "filesystem.write target exceeds the 1 MiB reconciliation limit".to_owned(),
        });
    }
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
