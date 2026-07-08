//! Security, permission, and audit abstractions for Portico.
//!
//! All trait skeletons in this crate intentionally leave extension points for
//! `PermissionEngine` integration in later phases.

use app_models::{AgentRunId, ThreadId, WorkspaceId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub mod audit;
pub mod command;
pub mod network;
pub mod permission;
pub mod redaction;
pub mod secrets;

pub use audit::MemoryAuditLogger;
pub use command::DefaultCommandPolicy;
pub use network::DefaultNetworkPolicy;
pub use permission::PolicyPermissionEngine;
pub use redaction::DefaultSecretRedactor;
pub use secrets::{InMemorySecretStore, KeyringSecretStore, SecretStore};

/// Decides whether a requested action is allowed by policy.
pub trait PermissionEngine: Send + Sync {
    /// Evaluate whether the action described by `request` is permitted.
    fn evaluate(&self, request: PermissionRequest) -> PermissionResult;
}

/// Request payload presented to a [`PermissionEngine`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    /// Workspace that owns the resource being accessed.
    pub workspace_id: WorkspaceId,
    /// Optional thread scope for the request.
    pub thread_id: Option<ThreadId>,
    /// Optional agent run scope for the request.
    pub run_id: Option<AgentRunId>,
    /// Action the caller wants to perform.
    pub action: String,
    /// Resource the action targets.
    pub resource: String,
    /// Whether the workspace is trusted for sensitive operations.
    pub trusted_workspace: bool,
}

/// Outcome of a permission evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionResult {
    /// The action is explicitly allowed.
    Allowed,
    /// The action requires user approval before proceeding.
    Ask {
        request: app_models::ApprovalRequest,
    },
    /// The action is denied with a human-readable reason.
    Denied { reason: String },
}

/// Scope at which a permission decision applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionScope {
    /// Apply to a single request.
    Once,
    /// Apply for the remainder of the current run.
    Run,
    /// Apply for the remainder of the current thread.
    Thread,
    /// Apply for the remainder of the current workspace.
    Workspace,
    /// Apply globally until revoked.
    Global,
}

/// A policy rule used by [`PolicyPermissionEngine`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Glob pattern matching the action (e.g. `filesystem.*`).
    pub action_pattern: String,
    /// Glob pattern matching the resource.
    pub resource_pattern: String,
    /// Decision to apply when the rule matches.
    pub decision: PermissionDecision,
    /// Scope at which the decision should be cached.
    pub scope: PermissionScope,
}

/// Decision produced by a [`PermissionRule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionDecision {
    /// Allow the action.
    Allow,
    /// Ask the user for approval before proceeding.
    Ask,
    /// Deny the action.
    Deny,
}

/// Policy applied to local command execution.
pub trait CommandPolicy: Send + Sync {
    /// Check whether a tokenized command may be executed.
    fn allow_command(&self, command: &str, arguments: &[String]) -> PermissionResult;

    /// Check whether a raw command line may be executed.
    ///
    /// The default implementation splits the line by pipes, tokenizes each
    /// segment, and delegates to [`Self::allow_command`].
    fn allow_command_line(&self, command_line: &str) -> PermissionResult {
        for segment in command_line.split('|') {
            let tokens = tokenize_command_line(segment.trim());
            if let Some(command) = tokens.first() {
                let result = self.allow_command(command, &tokens[1..]);
                if !matches!(result, PermissionResult::Allowed) {
                    return result;
                }
            }
        }
        PermissionResult::Allowed
    }
}

/// Simple tokenizer respecting double quotes.
fn tokenize_command_line(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Policy applied to outbound network access.
pub trait NetworkPolicy: Send + Sync {
    /// Check whether a network request may be issued.
    fn allow_request(&self, host: &str, port: u16) -> PermissionResult;
}

/// Persistent audit log for security-relevant events.
pub trait AuditLogger: Send + Sync {
    /// Record a single audit event.
    ///
    /// # Errors
    ///
    /// Returns an error if the audit event could not be persisted.
    fn log(&self, event: AuditEvent) -> Result<(), app_models::AppError>;
}

/// Bundled security policies used by tools and the runtime.
#[derive(Clone)]
pub struct SecurityContext {
    /// Permission engine used for action-level decisions.
    pub permissions: Arc<dyn PermissionEngine>,
    /// Command policy used for local shell execution.
    pub commands: Arc<dyn CommandPolicy>,
    /// Network policy used for outbound requests.
    pub network: Arc<dyn NetworkPolicy>,
    /// Audit logger used for security-relevant events.
    pub audit: Arc<dyn AuditLogger>,
}

impl SecurityContext {
    /// Create a new security context from the four required policies.
    #[must_use]
    pub fn new(
        permissions: Arc<dyn PermissionEngine>,
        commands: Arc<dyn CommandPolicy>,
        network: Arc<dyn NetworkPolicy>,
        audit: Arc<dyn AuditLogger>,
    ) -> Self {
        Self {
            permissions,
            commands,
            network,
            audit,
        }
    }

    /// Access the permission engine.
    #[must_use]
    pub fn permission_engine(&self) -> &Arc<dyn PermissionEngine> {
        &self.permissions
    }

    /// Access the command policy.
    #[must_use]
    pub fn command_policy(&self) -> &Arc<dyn CommandPolicy> {
        &self.commands
    }

    /// Access the network policy.
    #[must_use]
    pub fn network_policy(&self) -> &Arc<dyn NetworkPolicy> {
        &self.network
    }

    /// Access the audit logger.
    #[must_use]
    pub fn audit_logger(&self) -> &Arc<dyn AuditLogger> {
        &self.audit
    }
}

/// A security-relevant event suitable for the audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Workspace that owns the action.
    pub workspace_id: WorkspaceId,
    /// Optional thread scope.
    pub thread_id: Option<ThreadId>,
    /// Optional run scope.
    pub run_id: Option<AgentRunId>,
    /// Action that was attempted.
    pub action: String,
    /// Resource the action targeted.
    pub resource: String,
    /// Outcome of the action.
    pub outcome: PermissionResult,
}

/// Redacts secrets from strings before they are logged or returned to callers.
pub trait SecretRedactor: Send + Sync {
    /// Return a version of `text` with sensitive values replaced.
    fn redact(&self, text: &str) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyPolicy;

    impl CommandPolicy for DummyPolicy {
        fn allow_command(&self, _command: &str, _arguments: &[String]) -> PermissionResult {
            PermissionResult::Allowed
        }
    }

    #[test]
    fn dummy_policy_allows() {
        let policy = DummyPolicy;
        assert!(matches!(
            policy.allow_command("ls", &[]),
            PermissionResult::Allowed
        ));
    }
}
