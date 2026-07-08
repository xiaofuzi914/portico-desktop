//! Tool abstractions and permission hooks for Portico.

pub mod filesystem;
pub mod git;
pub mod terminal;

use app_models::{AgentRunId, AppError, WorkspaceId};
use app_security::{CommandPolicy, NetworkPolicy, PermissionEngine, PermissionResult};
use async_trait::async_trait;
use serde_json::Value;

/// A product-level tool that can be invoked by an agent run.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name of the tool.
    fn name(&self) -> &str;

    /// Human-readable description of what the tool does.
    fn description(&self) -> &str;

    /// Optional JSON Schema describing the tool's arguments.
    fn schema(&self) -> Option<Value> {
        None
    }

    /// Invoke the tool with the given input.
    ///
    /// # Errors
    ///
    /// Returns an error if the tool invocation fails or is denied.
    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, AppError>;
}

/// Input provided to a [`Tool`] invocation.
#[derive(Debug, Clone)]
pub struct ToolInput {
    /// Workspace context for the invocation.
    pub workspace_id: WorkspaceId,
    /// Run context for the invocation.
    pub run_id: AgentRunId,
    /// Structured arguments for the tool.
    pub arguments: Value,
}

/// Output returned by a [`Tool`] invocation.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Structured result of the invocation.
    pub result: Value,
}

/// Registry of available tools with permission-aware dispatch.
pub trait ToolRegistry: Send + Sync {
    /// Return a tool by name if it exists.
    fn get(&self, name: &str) -> Option<Box<dyn Tool>>;

    /// Evaluate whether the current context may invoke the named tool.
    ///
    /// Implementations should delegate to a [`PermissionEngine`] when
    /// `PermissionEngine` integration is wired in.
    fn may_invoke(&self, engine: &dyn PermissionEngine, name: &str) -> PermissionResult;
}

/// Factory for building permission-aware tool wrappers.
pub trait ToolFactory: Send + Sync {
    /// Create the default set of tools for a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if tool initialization fails.
    fn create_tools(
        &self,
        workspace_id: WorkspaceId,
        command_policy: Box<dyn CommandPolicy>,
        network_policy: Box<dyn NetworkPolicy>,
    ) -> Result<Box<dyn ToolRegistry>, AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_output_holds_value() {
        let output = ToolOutput {
            result: serde_json::json!({ "ok": true }),
        };
        assert_eq!(output.result["ok"], true);
    }
}
