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

    /// Look up a registered tool by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn PorticoTool>> {
        let tools = self.tools.read().expect("registry lock poisoned");
        tools.get(name).cloned()
    }

    /// Whether a tool name is currently registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        let tools = self.tools.read().expect("registry lock poisoned");
        tools.contains_key(name)
    }

    /// Built-in tools handled by [`app_runtime::SafeToolExecutor`] (not `Tool::invoke`).
    #[must_use]
    pub fn is_durable_builtin(name: &str) -> bool {
        matches!(
            name,
            "fs_read"
                | "fs_list"
                | "fs_search"
                | "fs_write"
                | "fs_edit"
                | "git"
                | "web_fetch"
                | "web_search"
                | "shell_exec"
        )
    }

    /// Register the reviewed, product-owned golden-path definitions.
    pub fn register_safe_builtin_definitions(&self) {
        self.register(Arc::new(SafeToolDefinition::fs_read()));
        self.register(Arc::new(SafeToolDefinition::fs_list()));
        self.register(Arc::new(SafeToolDefinition::fs_search()));
        self.register(Arc::new(SafeToolDefinition::fs_write()));
        self.register(Arc::new(SafeToolDefinition::fs_edit()));
        self.register(Arc::new(SafeToolDefinition::git_read()));
        self.register(Arc::new(SafeToolDefinition::web_fetch()));
        self.register(Arc::new(SafeToolDefinition::web_search()));
        self.register(Arc::new(SafeToolDefinition::shell_exec()));
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

    /// Clone a filtered registry view for a multi-agent role allowlist.
    ///
    /// `git:write` is a capability tag (not advertised as a separate tool).
    /// MCP tools are never included unless listed by exact name.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned.
    #[must_use]
    pub fn filtered_for_allowlist(&self, allowed: &[String]) -> Self {
        let allow: HashMap<&str, ()> = allowed
            .iter()
            .filter(|name| *name != "git:write")
            .map(|name| (name.as_str(), ()))
            .collect();
        let allow_git = allowed.iter().any(|t| t == "git" || t == "git:write");
        let source = self.tools.read().expect("registry lock poisoned");
        let mut filtered = HashMap::new();
        for (name, tool) in source.iter() {
            let ok = if name == "git" {
                allow_git
            } else {
                allow.contains_key(name.as_str())
            };
            if ok {
                filtered.insert(name.clone(), Arc::clone(tool));
            }
        }
        Self {
            tools: Arc::new(RwLock::new(filtered)),
        }
    }

    /// List registered tool names (for tests / diagnostics).
    #[must_use]
    pub fn tool_names(&self) -> Vec<String> {
        let tools = self.tools.read().expect("registry lock poisoned");
        let mut names: Vec<_> = tools.keys().cloned().collect();
        names.sort();
        names
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
            description: "Git in the workspace. Read: status, diff. Write (needs user approval): add, commit. repo_path may be absolute or relative; use '.' for the workspace root.",
            schema: json!({
                "type": "object",
                "properties": {
                    "subcommand": {
                        "type": "string",
                        "enum": ["status", "diff", "add", "commit"],
                        "description": "status/diff are read-only; add/commit require approval."
                    },
                    "repo_path": {
                        "type": "string",
                        "description": "Repository path absolute or relative to the workspace root. Use '.' for the project root."
                    },
                    "paths": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Paths to stage for subcommand=add (default: ['.'])."
                    },
                    "message": {
                        "type": "string",
                        "description": "Commit message for subcommand=commit."
                    }
                },
                "required": ["subcommand", "repo_path"]
            }),
        }
    }

    fn shell_exec() -> Self {
        Self {
            name: "shell_exec",
            description: "Run a shell command in the trusted project root (requires user approval). Prefer fs_* tools for reading/editing files. Blocked: sudo, rm -rf, curl|sh, force-push. Use for builds, tests, package managers when needed.",
            schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command line, e.g. 'pnpm test' or 'cargo check'."
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Optional working directory absolute or relative to the workspace root (default: project root)."
                    }
                },
                "required": ["command"]
            }),
        }
    }

    fn web_fetch() -> Self {
        Self {
            name: "web_fetch",
            description: "Fetch a public HTTP(S) URL and return readable text (HTML stripped). Use for live documentation, blogs, GitHub pages, or APIs. Localhost and private IPs are blocked. Prefer this when the user asks for current web information.",
            schema: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Absolute http:// or https:// URL to fetch."
                    },
                    "max_chars": {
                        "type": "integer",
                        "description": "Optional cap on returned text length (default 24000, max 80000)."
                    }
                },
                "required": ["url"]
            }),
        }
    }

    fn web_search() -> Self {
        Self {
            name: "web_search",
            description: "Search the public web for recent pages. Returns titles, URLs, and snippets. Follow up with web_fetch on the best URLs for full page text.",
            schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query in the user's language when appropriate."
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "How many results to return (default 5, max 10)."
                    }
                },
                "required": ["query"]
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
            [
                "fs_edit",
                "fs_list",
                "fs_read",
                "fs_search",
                "fs_write",
                "git",
                "shell_exec",
                "web_fetch",
                "web_search"
            ]
        );

        registry.retain(|name, _| name != "git");
        assert_eq!(registry.llm_tools().len(), 8);
    }

    #[test]
    fn filtered_allowlist_hides_write_tools_for_explorer() {
        let registry = PorticoToolRegistry::new();
        registry.register_safe_builtin_definitions();
        let filtered = registry.filtered_for_allowlist(&[
            "fs_list".to_owned(),
            "fs_read".to_owned(),
            "fs_search".to_owned(),
            "git".to_owned(),
            "web_search".to_owned(),
            "web_fetch".to_owned(),
        ]);
        let names = filtered.tool_names();
        assert!(names.contains(&"fs_read".to_owned()));
        assert!(names.contains(&"git".to_owned()));
        assert!(!names.contains(&"fs_write".to_owned()));
        assert!(!names.contains(&"shell_exec".to_owned()));
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
