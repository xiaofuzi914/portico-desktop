//! Adapters between Portico's product-level tools and `AutoAgents` tool traits.

use app_models::{AgentRunId, AppError, ThreadId, WorkspaceId};
use app_security::{PermissionEngine, PermissionResult};
use app_tools::{
    Tool as PorticoTool, ToolInput, ToolOutput, ToolRegistry,
    filesystem::{FilesystemTool, SearchMode},
    git::{GitOperations, GitTool},
    terminal::TerminalManager,
};
use async_trait::async_trait;
use autoagents_core::tool::{ToolCallError, ToolRuntime, ToolT};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, sync::RwLock};

/// Product-level tool registry that can be converted into `AutoAgents` tools.
///
/// The internal map is wrapped in a [`RwLock`] so tools can be registered
/// dynamically (e.g. when MCP server configurations change) through an
/// [`Arc`] without requiring `&mut self`.
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

    /// Register a Portico tool.
    ///
    /// # Panics
    ///
    /// Panics if the registry's internal lock is poisoned.
    pub fn register(&self, tool: Arc<dyn PorticoTool>) {
        let mut tools = self.tools.write().expect("registry lock poisoned");
        tools.insert(tool.name().to_owned(), tool);
    }

    /// Remove all tools for which `predicate` returns `false`.
    ///
    /// # Panics
    ///
    /// Panics if the registry's internal lock is poisoned.
    pub fn retain<F>(&self, predicate: F)
    where
        F: Fn(&str, &Arc<dyn PorticoTool>) -> bool,
    {
        let mut tools = self.tools.write().expect("registry lock poisoned");
        tools.retain(|name, tool| predicate(name, tool));
    }

    /// Convert all registered tools into `AutoAgents` wrappers for a specific run.
    ///
    /// # Panics
    ///
    /// Panics if the registry's internal lock is poisoned.
    #[must_use]
    pub fn autoagents_tools(
        &self,
        workspace_id: WorkspaceId,
        run_id: AgentRunId,
    ) -> Vec<Box<dyn ToolT>> {
        let tools = self.tools.read().expect("registry lock poisoned");
        tools
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
        let tools = self.tools.read().expect("registry lock poisoned");
        tools
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

/// Adapter that exposes filesystem operations as Portico agent tools.
///
/// The adapter itself is a factory for five individual tools:
/// `fs_read`, `fs_write`, `fs_edit`, `fs_list`, and `fs_search`.
#[derive(Clone)]
pub struct FilesystemToolAdapter {
    fs: Arc<FilesystemTool>,
}

impl FilesystemToolAdapter {
    /// Create a new adapter around a [`FilesystemTool`].
    #[must_use]
    pub const fn new(fs: Arc<FilesystemTool>) -> Self {
        Self { fs }
    }

    /// Return the `fs_read` tool.
    #[must_use]
    pub fn read_tool(&self) -> Arc<dyn PorticoTool> {
        Arc::new(FsReadTool {
            fs: Arc::clone(&self.fs),
        })
    }

    /// Return the `fs_write` tool.
    #[must_use]
    pub fn write_tool(&self) -> Arc<dyn PorticoTool> {
        Arc::new(FsWriteTool {
            fs: Arc::clone(&self.fs),
        })
    }

    /// Return the `fs_edit` tool.
    #[must_use]
    pub fn edit_tool(&self) -> Arc<dyn PorticoTool> {
        Arc::new(FsEditTool {
            fs: Arc::clone(&self.fs),
        })
    }

    /// Return the `fs_list` tool.
    #[must_use]
    pub fn list_tool(&self) -> Arc<dyn PorticoTool> {
        Arc::new(FsListTool {
            fs: Arc::clone(&self.fs),
        })
    }

    /// Return the `fs_search` tool.
    #[must_use]
    pub fn search_tool(&self) -> Arc<dyn PorticoTool> {
        Arc::new(FsSearchTool {
            fs: Arc::clone(&self.fs),
        })
    }
}

#[derive(Clone)]
struct FsReadTool {
    fs: Arc<FilesystemTool>,
}

#[async_trait]
impl PorticoTool for FsReadTool {
    fn name(&self) -> &'static str {
        "fs_read"
    }

    fn description(&self) -> &'static str {
        "Read the contents of a file within the workspace"
    }

    fn schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                }
            },
            "required": ["path"]
        }))
    }

    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, AppError> {
        let path = input.arguments["path"].as_str().ok_or_else(|| AppError::Internal {
            message: "fs_read path must be a string".to_owned(),
        })?;
        let content = self.fs.read_file(input.workspace_id, path).await?;
        Ok(ToolOutput {
            result: json!({ "content": content }),
        })
    }
}

#[derive(Clone)]
struct FsWriteTool {
    fs: Arc<FilesystemTool>,
}

#[async_trait]
impl PorticoTool for FsWriteTool {
    fn name(&self) -> &'static str {
        "fs_write"
    }

    fn description(&self) -> &'static str {
        "Write content to a file within the workspace"
    }

    fn schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            },
            "required": ["path", "content"]
        }))
    }

    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, AppError> {
        let path = input.arguments["path"].as_str().ok_or_else(|| AppError::Internal {
            message: "fs_write path must be a string".to_owned(),
        })?;
        let content = input.arguments["content"].as_str().unwrap_or("");
        self.fs.write_file(input.workspace_id, path, content).await?;
        Ok(ToolOutput {
            result: json!({ "ok": true }),
        })
    }
}

#[derive(Clone)]
struct FsEditTool {
    fs: Arc<FilesystemTool>,
}

#[async_trait]
impl PorticoTool for FsEditTool {
    fn name(&self) -> &'static str {
        "fs_edit"
    }

    fn description(&self) -> &'static str {
        "Replace a unique occurrence of a string in a file within the workspace"
    }

    fn schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact string to replace (must occur exactly once)"
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement string"
                }
            },
            "required": ["path", "old_string", "new_string"]
        }))
    }

    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, AppError> {
        let path = input.arguments["path"].as_str().ok_or_else(|| AppError::Internal {
            message: "fs_edit path must be a string".to_owned(),
        })?;
        let old_string = input.arguments["old_string"].as_str().ok_or_else(|| AppError::Internal {
            message: "fs_edit old_string must be a string".to_owned(),
        })?;
        let new_string = input.arguments["new_string"].as_str().unwrap_or("");
        self.fs
            .edit_file(input.workspace_id, path, old_string, new_string)
            .await?;
        Ok(ToolOutput {
            result: json!({ "ok": true }),
        })
    }
}

#[derive(Clone)]
struct FsListTool {
    fs: Arc<FilesystemTool>,
}

#[async_trait]
impl PorticoTool for FsListTool {
    fn name(&self) -> &'static str {
        "fs_list"
    }

    fn description(&self) -> &'static str {
        "List the entries of a directory within the workspace"
    }

    fn schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the directory to list"
                }
            },
            "required": ["path"]
        }))
    }

    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, AppError> {
        let path = input.arguments["path"].as_str().ok_or_else(|| AppError::Internal {
            message: "fs_list path must be a string".to_owned(),
        })?;
        let entries = self.fs.list_dir(input.workspace_id, path).await?;
        let items: Vec<Value> = entries
            .into_iter()
            .map(|entry| {
                json!({
                    "name": entry.name,
                    "kind": entry.kind.as_str(),
                })
            })
            .collect();
        Ok(ToolOutput {
            result: json!({ "entries": items }),
        })
    }
}

#[derive(Clone)]
struct FsSearchTool {
    fs: Arc<FilesystemTool>,
}

#[async_trait]
impl PorticoTool for FsSearchTool {
    fn name(&self) -> &'static str {
        "fs_search"
    }

    fn description(&self) -> &'static str {
        "Search files under a workspace directory by filename glob or content substring"
    }

    fn schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "root": {
                    "type": "string",
                    "description": "Absolute directory path to search under"
                },
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern for filenames or substring for content search"
                },
                "mode": {
                    "type": "string",
                    "enum": ["filename_glob", "content_substring"],
                    "description": "Whether to match filenames or file contents"
                }
            },
            "required": ["root", "pattern", "mode"]
        }))
    }

    async fn invoke(&self, input: ToolInput) -> Result<ToolOutput, AppError> {
        let root = input.arguments["root"].as_str().ok_or_else(|| AppError::Internal {
            message: "fs_search root must be a string".to_owned(),
        })?;
        let pattern = input.arguments["pattern"].as_str().ok_or_else(|| AppError::Internal {
            message: "fs_search pattern must be a string".to_owned(),
        })?;
        let mode = match input.arguments["mode"].as_str() {
            Some("filename_glob") => SearchMode::FilenameGlob,
            Some("content_substring") => SearchMode::ContentSubstring,
            _ => {
                return Err(AppError::Internal {
                    message: "fs_search mode must be 'filename_glob' or 'content_substring'"
                        .to_owned(),
                })
            }
        };
        let results = self
            .fs
            .search_files(input.workspace_id, root, pattern, mode)
            .await?;
        let items: Vec<Value> = results
            .into_iter()
            .map(|result| {
                json!({
                    "path": result.path,
                    "kind": result.kind.as_str(),
                })
            })
            .collect();
        Ok(ToolOutput {
            result: json!({ "results": items }),
        })
    }
}


