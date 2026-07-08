//! Adapters between Portico's product-level tools and `AutoAgents` tool traits.

use app_models::{AgentRunId, AppError, ThreadId, WorkspaceId};
use app_security::{PermissionEngine, PermissionResult};
use app_tools::{
    Tool as PorticoTool, ToolInput, ToolOutput, ToolRegistry,
    git::{GitOperations, GitTool},
    terminal::TerminalManager,
};
use async_trait::async_trait;
use autoagents_core::tool::{ToolCallError, ToolRuntime, ToolT};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

/// Product-level tool registry that can be converted into `AutoAgents` tools.
#[derive(Clone, Default)]
pub struct PorticoToolRegistry {
    tools: HashMap<String, Arc<dyn PorticoTool>>,
}

impl std::fmt::Debug for PorticoToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PorticoToolRegistry")
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl PorticoToolRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a Portico tool.
    pub fn register(&mut self, tool: Arc<dyn PorticoTool>) {
        self.tools.insert(tool.name().to_owned(), tool);
    }

    /// Convert all registered tools into `AutoAgents` wrappers for a specific run.
    #[must_use]
    pub fn autoagents_tools(
        &self,
        workspace_id: WorkspaceId,
        run_id: AgentRunId,
    ) -> Vec<Box<dyn ToolT>> {
        self.tools
            .values()
            .map(|tool| {
                Box::new(AutoAgentsToolWrapper {
                    workspace_id,
                    run_id,
                    tool: Arc::clone(tool),
                }) as Box<dyn ToolT>
            })
            .collect()
    }
}

impl ToolRegistry for PorticoToolRegistry {
    fn get(&self, name: &str) -> Option<Box<dyn PorticoTool>> {
        self.tools
            .get(name)
            .map(|tool| Box::new(SharedPorticoTool::new(Arc::clone(tool))) as Box<dyn PorticoTool>)
    }

    fn may_invoke(&self, _engine: &dyn PermissionEngine, _name: &str) -> PermissionResult {
        PermissionResult::Allowed
    }
}

/// Wraps an `Arc<dyn PorticoTool>` so it can be returned as `Box<dyn PorticoTool>`.
#[derive(Clone)]
struct SharedPorticoTool {
    inner: Arc<dyn PorticoTool>,
}

impl SharedPorticoTool {
    fn new(inner: Arc<dyn PorticoTool>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl PorticoTool for SharedPorticoTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn schema(&self) -> Option<Value> {
        self.inner.schema()
    }

    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, AppError> {
        self.inner.invoke(input).await
    }
}

/// Wraps a Portico tool so it can be driven by an `AutoAgents` agent.
#[derive(Clone)]
struct AutoAgentsToolWrapper {
    workspace_id: WorkspaceId,
    run_id: AgentRunId,
    tool: Arc<dyn PorticoTool>,
}

impl std::fmt::Debug for AutoAgentsToolWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutoAgentsToolWrapper")
            .field("workspace_id", &self.workspace_id)
            .field("run_id", &self.run_id)
            .field("tool", &self.tool.name())
            .finish()
    }
}

impl ToolT for AutoAgentsToolWrapper {
    fn name(&self) -> &str {
        self.tool.name()
    }

    fn description(&self) -> &str {
        self.tool.description()
    }

    fn args_schema(&self) -> Value {
        self.tool.schema().unwrap_or_else(|| json!({"type": "object"}))
    }
}

#[async_trait]
impl ToolRuntime for AutoAgentsToolWrapper {
    async fn execute(&self, args: Value) -> Result<Value, ToolCallError> {
        let input = ToolInput {
            workspace_id: self.workspace_id,
            run_id: self.run_id,
            arguments: args,
        };

        match self.tool.invoke(input).await {
            Ok(ToolOutput { result }) => Ok(result),
            Err(err) => Err(ToolCallError::RuntimeError(Box::new(err))),
        }
    }
}

/// Adapter that exposes git operations as a Portico tool.
#[derive(Clone)]
pub struct GitToolAdapter {
    git: Arc<GitTool>,
}

impl GitToolAdapter {
    /// Create a new adapter around a [`GitTool`].
    #[must_use]
    pub const fn new(git: Arc<GitTool>) -> Self {
        Self { git }
    }
}

#[async_trait]
impl PorticoTool for GitToolAdapter {
    fn name(&self) -> &'static str {
        "git"
    }

    fn description(&self) -> &'static str {
        "Run git operations in a repository"
    }

    fn schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "subcommand": {
                    "type": "string",
                    "enum": ["status", "diff", "stage", "unstage", "commit", "branch", "push"],
                    "description": "Git subcommand to run"
                },
                "repo_path": {
                    "type": "string",
                    "description": "Path to the git repository"
                },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths for stage/unstage"
                },
                "message": {
                    "type": "string",
                    "description": "Commit message"
                },
                "branch_name": {
                    "type": "string",
                    "description": "Branch name for branch subcommand"
                },
                "remote": {
                    "type": "string",
                    "description": "Remote name for push"
                }
            },
            "required": ["subcommand", "repo_path"]
        }))
    }

    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, AppError> {
        let subcommand =
            input.arguments["subcommand"].as_str().ok_or_else(|| AppError::Internal {
                message: "git subcommand must be a string".to_owned(),
            })?;
        let repo_path =
            input.arguments["repo_path"].as_str().ok_or_else(|| AppError::Internal {
                message: "git repo_path must be a string".to_owned(),
            })?;

        let output = match subcommand {
            "status" => self.git.status(input.workspace_id, repo_path).await?,
            "diff" => self.git.diff(input.workspace_id, repo_path).await?,
            "stage" => {
                let paths: Vec<String> = input.arguments["paths"]
                    .as_array()
                    .map(|arr| {
                        arr.iter().filter_map(|v| v.as_str().map(ToOwned::to_owned)).collect()
                    })
                    .unwrap_or_default();
                self.git.stage(input.workspace_id, repo_path, &paths).await?;
                json!({ "ok": true }).to_string()
            }
            "unstage" => {
                let paths: Vec<String> = input.arguments["paths"]
                    .as_array()
                    .map(|arr| {
                        arr.iter().filter_map(|v| v.as_str().map(ToOwned::to_owned)).collect()
                    })
                    .unwrap_or_default();
                self.git.unstage(input.workspace_id, repo_path, &paths).await?;
                json!({ "ok": true }).to_string()
            }
            "commit" => {
                let message = input.arguments["message"].as_str().unwrap_or("commit");
                self.git.commit(input.workspace_id, repo_path, message).await?
            }
            "branch" => {
                let name = input.arguments["branch_name"].as_str();
                self.git.branch(input.workspace_id, repo_path, name).await?
            }
            "push" => {
                let remote = input.arguments["remote"].as_str();
                self.git.push(input.workspace_id, repo_path, remote).await?
            }
            other => {
                return Err(AppError::Internal {
                    message: format!("unsupported git subcommand: {other}"),
                });
            }
        };

        Ok(ToolOutput {
            result: json!({ "output": output }),
        })
    }
}

/// Adapter that exposes terminal command execution as a Portico tool.
#[derive(Clone)]
pub struct TerminalToolAdapter {
    terminal: Arc<TerminalManager>,
}

impl TerminalToolAdapter {
    /// Create a new adapter around a [`TerminalManager`].
    #[must_use]
    pub const fn new(terminal: Arc<TerminalManager>) -> Self {
        Self { terminal }
    }
}

#[async_trait]
impl PorticoTool for TerminalToolAdapter {
    fn name(&self) -> &'static str {
        "terminal"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command in a working directory"
    }

    fn schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory for the command"
                }
            },
            "required": ["command"]
        }))
    }

    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, AppError> {
        let command = input.arguments["command"].as_str().ok_or_else(|| AppError::Internal {
            message: "terminal command must be a string".to_owned(),
        })?;
        let cwd = input.arguments["cwd"].as_str().unwrap_or(".");

        let thread_id = ThreadId::new();
        let session_id = self.terminal.create_session(thread_id).await;
        let output = self.terminal.execute(session_id, command, cwd).await?;

        Ok(ToolOutput {
            result: serde_json::to_value(output).map_err(|e| AppError::Internal {
                message: format!("failed to serialize terminal output: {e}"),
            })?,
        })
    }
}
