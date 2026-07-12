//! Product-owned tool definitions exposed to model providers.
//!
//! This registry intentionally contains schemas only. Tool effects are never
//! dispatched through the legacy `AutoAgents` wrapper path; the durable runtime
//! owns policy evaluation, approval, leasing, execution, and reconciliation.

use app_models::AppError;
use app_tools::{Tool as PorticoTool, ToolInput, ToolOutput};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, sync::RwLock};

/// Registry of product-level tool definitions safe to advertise to a model.
#[derive(Clone, Default)]
pub struct PorticoToolRegistry {
    tools: Arc<RwLock<HashMap<String, Arc<dyn PorticoTool>>>>,
}

impl std::fmt::Debug for PorticoToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tools = self.tools.read().expect("registry lock poisoned");
        f.debug_struct("PorticoToolRegistry")
            .field("tools", &tools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl PorticoToolRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a schema-bearing product tool.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn register(&self, tool: Arc<dyn PorticoTool>) {
        let mut tools = self.tools.write().expect("registry lock poisoned");
        tools.insert(tool.name().to_owned(), tool);
    }

    /// Register the reviewed, product-owned golden-path definitions.
    pub fn register_safe_builtin_definitions(&self) {
        self.register(Arc::new(SafeToolDefinition::fs_read()));
        self.register(Arc::new(SafeToolDefinition::fs_list()));
        self.register(Arc::new(SafeToolDefinition::fs_search()));
        self.register(Arc::new(SafeToolDefinition::fs_write()));
        self.register(Arc::new(SafeToolDefinition::fs_edit()));
        self.register(Arc::new(SafeToolDefinition::git_read()));
    }

    /// Convert registered schemas to provider-independent LLM tools.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn llm_tools(&self) -> Vec<autoagents_llm::chat::Tool> {
        let tools = self.tools.read().expect("registry lock poisoned");
        tools
            .values()
            .map(|tool| autoagents_llm::chat::Tool {
                tool_type: "function".to_owned(),
                function: autoagents_llm::chat::FunctionTool {
                    name: tool.name().to_owned(),
                    description: tool.description().to_owned(),
                    parameters: tool.schema().unwrap_or_else(|| json!({"type": "object"})),
                },
            })
            .collect()
    }

    /// Retain only definitions matching the supplied predicate.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    pub fn retain<F>(&self, predicate: F)
    where
        F: Fn(&str, &Arc<dyn PorticoTool>) -> bool,
    {
        let mut tools = self.tools.write().expect("registry lock poisoned");
        tools.retain(|name, tool| predicate(name, tool));
    }
}

#[derive(Debug, Clone)]
struct SafeToolDefinition {
    name: &'static str,
    description: &'static str,
    schema: Value,
}

impl SafeToolDefinition {
    fn fs_read() -> Self {
        Self {
            name: "fs_read",
            description: "Read a single UTF-8 file from the current trusted workspace. Path may be absolute or relative to the workspace root. For directories, use fs_list instead.",
            schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path absolute or relative to the workspace root (for example README.md)."
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn fs_list() -> Self {
        Self {
            name: "fs_list",
            description: "List files and subdirectories inside the current trusted workspace. Path may be absolute or relative to the workspace root. Use '.' for the project root.",
            schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path absolute or relative to the workspace root. Use '.' for the project root."
                    }
                },
                "required": ["path"]
            }),
        }
    }

    fn fs_write() -> Self {
        Self {
            name: "fs_write",
            description: "Create or fully replace a file after policy/approval. Prefer fs_edit for surgical changes. Path may be absolute or relative to the workspace root.",
            schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path absolute or relative to the workspace root."
                    },
                    "content": {"type": "string"}
                },
                "required": ["path", "content"]
            }),
        }
    }

    fn fs_edit() -> Self {
        Self {
            name: "fs_edit",
            description: "Apply a unique string replacement inside an existing workspace file (surgical edit). old_string must match exactly once. Prefer this over fs_write for local code changes.",
            schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path absolute or relative to the workspace root."
                    },
                    "old_string": {
                        "type": "string",
                        "description": "Exact text to find (must occur exactly once)."
                    },
                    "new_string": {
                        "type": "string",
                        "description": "Replacement text."
                    }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        }
    }

    fn fs_search() -> Self {
        Self {
            name: "fs_search",
            description: "Search the workspace for files by glob name or text content. Use before reading large trees.",
            schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path absolute or relative to the workspace root. Use '.' for the project root."
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Glob (e.g. *.rs) when mode=glob, or substring when mode=content."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["glob", "content"],
                        "description": "glob matches file names; content matches UTF-8 file text."
                    }
                },
                "required": ["path", "pattern", "mode"]
            }),
        }
    }

    fn git_read() -> Self {
        Self {
            name: "git",
            description: "Inspect git status or diff in the current workspace. repo_path may be absolute or relative; use '.' for the workspace root.",
            schema: json!({
                "type": "object",
                "properties": {
                    "subcommand": {"type": "string", "enum": ["status", "diff"]},
                    "repo_path": {
                        "type": "string",
                        "description": "Repository path absolute or relative to the workspace root. Use '.' for the project root."
                    }
                },
                "required": ["subcommand", "repo_path"]
            }),
        }
    }
}

#[async_trait]
impl PorticoTool for SafeToolDefinition {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn schema(&self) -> Option<Value> {
        Some(self.schema.clone())
    }

    async fn invoke(&self, _input: ToolInput) -> Result<ToolOutput, AppError> {
        Err(AppError::PermissionDenied {
            reason: "safe built-in tools require durable runtime dispatch".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::{AgentRunId, WorkspaceId};

    #[test]
    fn safe_registry_advertises_only_the_reviewed_definitions() {
        let registry = PorticoToolRegistry::new();
        registry.register_safe_builtin_definitions();
        let mut names = registry
            .llm_tools()
            .into_iter()
            .map(|tool| tool.function.name)
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            ["fs_edit", "fs_list", "fs_read", "fs_search", "fs_write", "git"]
        );

        registry.retain(|name, _| name != "git");
        assert_eq!(registry.llm_tools().len(), 5);
    }

    #[tokio::test]
    async fn direct_invocation_is_always_denied() {
        let definition = SafeToolDefinition::fs_write();
        let result = definition
            .invoke(ToolInput {
                workspace_id: WorkspaceId::new(),
                run_id: AgentRunId::new(),
                arguments: json!({"path": "x", "content": "y"}),
            })
            .await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }
}
