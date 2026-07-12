//! Bridge that exposes MCP server tools as Portico agent tools.
//!
//! Each [`McpToolAdapter`] wraps a single [`McpToolInfo`] and delegates
//! invocations to the shared [`McpClientManager`]. The tool's name, description,
//! and input schema are passed through to the LLM unchanged.

use app_plugins::{McpClientManager, McpToolInfo};
use app_security::{AuditEvent, PermissionRequest, PermissionResult, SecurityContext};
use app_tools::{Tool as PorticoTool, ToolInput, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

/// Adapter that exposes an MCP tool as a Portico tool.
#[derive(Clone)]
pub struct McpToolAdapter {
    info: McpToolInfo,
    manager: Arc<McpClientManager>,
    security: Arc<SecurityContext>,
}

impl McpToolAdapter {
    /// Create a new adapter around an MCP tool description.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(
        info: McpToolInfo,
        manager: Arc<McpClientManager>,
        security: Arc<SecurityContext>,
    ) -> Self {
        Self {
            info,
            manager,
            security,
        }
    }
}

#[async_trait]
impl PorticoTool for McpToolAdapter {
    fn name(&self) -> &str {
        &self.info.name
    }

    fn description(&self) -> &str {
        &self.info.description
    }

    fn schema(&self) -> Option<Value> {
        self.info.input_schema.clone()
    }

    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, app_models::AppError> {
        let action = if self.info.side_effects {
            "mcp.invoke.write"
        } else {
            "mcp.invoke.read"
        };

        let permission_result = self.security.permission_engine().evaluate(PermissionRequest {
            workspace_id: input.workspace_id,
            thread_id: None,
            run_id: Some(input.run_id),
            action: action.to_owned(),
            resource: self.info.name.clone(),
            trusted_workspace: true,
        });

        if !matches!(permission_result, PermissionResult::Allowed) {
            self.audit(input.workspace_id, input.run_id, action, &permission_result);
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
            return Err(app_models::AppError::PermissionDenied { reason });
        }

        let result = self
            .manager
            .invoke_tool(
                &self.info.tool_name,
                input.arguments,
                Some(self.info.server_id),
            )
            .await?;

        self.audit(
            input.workspace_id,
            input.run_id,
            action,
            &PermissionResult::Allowed,
        );

        Ok(ToolOutput { result })
    }
}

impl McpToolAdapter {
    fn audit(
        &self,
        workspace_id: app_models::WorkspaceId,
        run_id: app_models::AgentRunId,
        action: &str,
        outcome: &PermissionResult,
    ) {
        let _ = self.security.audit_logger().log(AuditEvent {
            workspace_id,
            thread_id: None,
            run_id: Some(run_id),
            action: action.to_owned(),
            resource: self.info.name.clone(),
            outcome: outcome.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_security::{
        DefaultCommandPolicy, DefaultNetworkPolicy, MemoryAuditLogger, PermissionEngine,
        PermissionRequest, PermissionResult,
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

    fn tool_info_with_schema(schema: Option<Value>) -> McpToolInfo {
        McpToolInfo {
            name: "1_test".to_owned(),
            tool_name: "test".to_owned(),
            description: "A test MCP tool".to_owned(),
            server_id: 1,
            side_effects: false,
            input_schema: schema,
        }
    }

    #[test]
    fn schema_is_passed_through() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "value": { "type": "string" }
            }
        });
        let adapter = McpToolAdapter::new(
            tool_info_with_schema(Some(schema.clone())),
            Arc::new(McpClientManager::new()),
            test_security(),
        );
        assert_eq!(adapter.schema(), Some(schema));
    }

    #[test]
    fn schema_is_none_when_missing() {
        let adapter = McpToolAdapter::new(
            tool_info_with_schema(None),
            Arc::new(McpClientManager::new()),
            test_security(),
        );
        assert!(adapter.schema().is_none());
    }

    #[tokio::test]
    async fn side_effect_tool_is_gated_by_permission_engine() {
        let audit = Arc::new(MemoryAuditLogger::new());
        let security = Arc::new(SecurityContext::new(
            Arc::new(DenyAllEngine),
            Arc::new(DefaultCommandPolicy::new()),
            Arc::new(DefaultNetworkPolicy::new()),
            audit.clone(),
        ));
        let info = McpToolInfo {
            name: "1_write".to_owned(),
            tool_name: "write".to_owned(),
            description: "Write tool".to_owned(),
            server_id: 1,
            side_effects: true,
            input_schema: None,
        };
        let adapter = McpToolAdapter::new(info, Arc::new(McpClientManager::new()), security);

        let result = adapter
            .invoke(ToolInput {
                workspace_id: app_models::WorkspaceId::default(),
                run_id: app_models::AgentRunId::default(),
                arguments: serde_json::json!({}),
            })
            .await;

        assert!(matches!(
            result,
            Err(app_models::AppError::PermissionDenied { .. })
        ));

        let events = audit.events().expect("read events");
        assert!(events.iter().any(|e| e.action == "mcp.invoke.write"));
    }

    #[tokio::test]
    async fn read_only_tool_invokes_without_real_server() {
        // The manager has no configured server, so the actual invoke will fail
        // with NotFound. The test confirms that a read-only tool passes the
        // permission gate and reaches the manager.
        let info = McpToolInfo {
            name: "1_read".to_owned(),
            tool_name: "read".to_owned(),
            description: "Read tool".to_owned(),
            server_id: 1,
            side_effects: false,
            input_schema: None,
        };
        let adapter = McpToolAdapter::new(info, Arc::new(McpClientManager::new()), test_security());

        let result = adapter
            .invoke(ToolInput {
                workspace_id: app_models::WorkspaceId::default(),
                run_id: app_models::AgentRunId::default(),
                arguments: serde_json::json!({}),
            })
            .await;

        assert!(matches!(result, Err(app_models::AppError::NotFound { .. })));
    }
}
