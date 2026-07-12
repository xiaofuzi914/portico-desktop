//! Rule-based [`PermissionEngine`] implementation.

use crate::{PermissionEngine, PermissionRequest, PermissionResult};

/// A [`PermissionEngine`] that evaluates an ordered list of [`PermissionRule`]s.
#[derive(Debug, Clone, Default)]
pub struct PolicyPermissionEngine {
    rules: Vec<crate::PermissionRule>,
}

impl PolicyPermissionEngine {
    /// Create a new engine from a list of rules.
    #[must_use]
    pub const fn new(rules: Vec<crate::PermissionRule>) -> Self {
        Self { rules }
    }

    /// Create a policy with sensible Phase 5 defaults.
    ///
    /// Default rules:
    /// - Deny all `shell.*` actions.
    /// - Allow `filesystem.read` / `filesystem.write` / `git.read` in trusted
    ///   workspaces (path allowlists enforced by the policy gate).
    /// - Allow read-only MCP tool invocations.
    /// - Ask for side-effect MCP tool invocations.
    /// - Allow `network.*` at the action level; private host/port checks are
    ///   delegated to [`crate::NetworkPolicy`].
    #[must_use]
    pub fn default_rules() -> Self {
        let rules = vec![
            crate::PermissionRule {
                action_pattern: "shell.*".to_owned(),
                resource_pattern: "*".to_owned(),
                decision: crate::PermissionDecision::Deny,
                scope: crate::PermissionScope::Global,
            },
            // Untrusted / non-default write handling falls through to
            // apply_default_policy, which Allows only when trusted_workspace is set.
            crate::PermissionRule {
                action_pattern: "mcp.invoke.read".to_owned(),
                resource_pattern: "*".to_owned(),
                decision: crate::PermissionDecision::Allow,
                scope: crate::PermissionScope::Global,
            },
            crate::PermissionRule {
                action_pattern: "mcp.invoke.write".to_owned(),
                resource_pattern: "*".to_owned(),
                decision: crate::PermissionDecision::Ask,
                scope: crate::PermissionScope::Once,
            },
        ];
        Self::new(rules)
    }

    /// Add a rule to the end of the policy.
    pub fn push_rule(&mut self, rule: crate::PermissionRule) {
        self.rules.push(rule);
    }
}

impl PermissionEngine for PolicyPermissionEngine {
    fn evaluate(&self, request: PermissionRequest) -> PermissionResult {
        for rule in &self.rules {
            if matches_glob(&rule.action_pattern, &request.action)
                && matches_glob(&rule.resource_pattern, &request.resource)
            {
                return match rule.decision {
                    crate::PermissionDecision::Allow => PermissionResult::Allowed,
                    crate::PermissionDecision::Deny => PermissionResult::Denied {
                        reason: format!("rule denied {} on {}", request.action, request.resource),
                    },
                    crate::PermissionDecision::Ask => PermissionResult::Ask {
                        request: app_models::ApprovalRequest {
                            id: app_models::ApprovalRequestId(0),
                            run_id: request.run_id.unwrap_or_default(),
                            workspace_id: request.workspace_id,
                            thread_id: request.thread_id.unwrap_or_default(),
                            action: request.action,
                            resource: request.resource,
                            status: app_models::ApprovalRequestStatus::Pending,
                            created_at: chrono::Utc::now(),
                            resolved_at: None,
                            resolution_reason: None,
                        },
                    },
                };
            }
        }

        apply_default_policy(&request)
    }
}

/// Simple glob matcher supporting `*` at the end of a segment.
fn matches_glob(pattern: &str, value: &str) -> bool {
    if pattern == "*" || pattern == value {
        return true;
    }

    // Support patterns like "filesystem.*" by matching the prefix up to the wildcard.
    if let Some(prefix) = pattern.strip_suffix(".*")
        && let Some(value_prefix) = value.rsplit_once('.').map(|(p, _)| p)
    {
        return prefix == value_prefix;
    }

    // Support a single trailing `*` that matches the remainder of the value.
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }

    false
}

fn apply_default_policy(request: &PermissionRequest) -> PermissionResult {
    if request.action.starts_with("shell.") {
        return PermissionResult::Denied {
            reason: "shell commands are blocked by default".to_owned(),
        };
    }

    if request.action == "filesystem.read" && request.trusted_workspace {
        return PermissionResult::Allowed;
    }

    if request.action == "git.read" && request.trusted_workspace {
        return PermissionResult::Allowed;
    }

    // Trusted projects may write inside their allowlist (path checks happen in
    // PolicyGate). Untrusted workspaces still require an explicit approval path.
    if request.action == "filesystem.write" {
        if request.trusted_workspace {
            return PermissionResult::Allowed;
        }
        return PermissionResult::Denied {
            reason: "filesystem write requires a trusted project or approval".to_owned(),
        };
    }

    // Network category decisions are delegated to NetworkPolicy at request time.
    if request.action.starts_with("network.") {
        return PermissionResult::Allowed;
    }

    PermissionResult::Denied {
        reason: format!("no policy for {} on {}", request.action, request.resource),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::WorkspaceId;

    fn request(action: &str, resource: &str, trusted: bool) -> PermissionRequest {
        PermissionRequest {
            workspace_id: WorkspaceId::new(),
            thread_id: None,
            run_id: None,
            action: action.to_owned(),
            resource: resource.to_owned(),
            trusted_workspace: trusted,
        }
    }

    #[test]
    fn shell_actions_are_denied() {
        let engine = PolicyPermissionEngine::default_rules();
        assert!(matches!(
            engine.evaluate(request("shell.exec", "ls", true)),
            PermissionResult::Denied { .. }
        ));
    }

    #[test]
    fn filesystem_read_allowed_in_trusted_workspace() {
        let engine = PolicyPermissionEngine::default_rules();
        assert!(matches!(
            engine.evaluate(request("filesystem.read", "/tmp/foo", true)),
            PermissionResult::Allowed
        ));
    }

    #[test]
    fn filesystem_read_denied_in_untrusted_workspace() {
        let engine = PolicyPermissionEngine::default_rules();
        assert!(matches!(
            engine.evaluate(request("filesystem.read", "/tmp/foo", false)),
            PermissionResult::Denied { .. }
        ));
    }

    #[test]
    fn filesystem_write_allowed_in_trusted_workspace() {
        let engine = PolicyPermissionEngine::default_rules();
        let result = engine.evaluate(request("filesystem.write", "/tmp/foo", true));
        assert!(
            matches!(result, PermissionResult::Allowed),
            "expected Allowed for trusted write, got {result:?}"
        );
    }

    #[test]
    fn filesystem_write_denied_in_untrusted_workspace() {
        let engine = PolicyPermissionEngine::default_rules();
        let result = engine.evaluate(request("filesystem.write", "/tmp/foo", false));
        assert!(
            matches!(result, PermissionResult::Denied { .. }),
            "expected Denied for untrusted write, got {result:?}"
        );
    }

    #[test]
    fn network_allowed_by_permission_engine() {
        let engine = PolicyPermissionEngine::default_rules();
        assert!(matches!(
            engine.evaluate(request("network.fetch", "https://example.com", false)),
            PermissionResult::Allowed
        ));
    }

    #[test]
    fn custom_rule_overrides_default() {
        let mut engine = PolicyPermissionEngine::default_rules();
        engine.push_rule(crate::PermissionRule {
            action_pattern: "custom.action".to_owned(),
            resource_pattern: "*".to_owned(),
            decision: crate::PermissionDecision::Allow,
            scope: crate::PermissionScope::Global,
        });
        assert!(matches!(
            engine.evaluate(request("custom.action", "resource", false)),
            PermissionResult::Allowed
        ));
    }
}
