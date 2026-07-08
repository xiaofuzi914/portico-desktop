//! Plugin extension abstractions for Portico.

pub mod mcp;
pub mod registry;

pub use mcp::{McpClientManager, McpToolInfo};
pub use registry::{PluginRegistry, SqlitePluginRegistry};

use app_models::WorkspaceId;

/// Context provided when registering a plugin.
#[derive(Debug, Clone)]
pub struct PluginContext {
    /// Workspace the plugin is being registered for.
    pub workspace_id: WorkspaceId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_context_holds_workspace() {
        let workspace_id = WorkspaceId::new();
        let ctx = PluginContext { workspace_id };
        assert_eq!(ctx.workspace_id, workspace_id);
    }
}
