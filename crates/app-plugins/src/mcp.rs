//! Real MCP client manager for Portico.
//!
//! This crate now implements a minimal hand-rolled MCP client that speaks the
//! JSON-RPC initialize / `tools/list` / `tools/call` messages required by the
//! Model Context Protocol. Both stdio (spawned command with JSON-RPC over
//! stdin/stdout) and HTTP (POST-based JSON-RPC) transports are supported.
//!
//! Tool names are exposed to callers as `{server_id}_{tool_name}`. Invocations
//! are routed back to the owning server by parsing that prefix.
//!
//! Tool lists are cached per server and invalidated when configurations change.
//! There is currently no background health-check or automatic reconnection: a
//! server that fails during `list_tools` is skipped and logged, and the next
//! call will attempt to connect again.

use app_models::{AppError, McpServerConfig, McpTransport};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, oneshot};
use tokio::time::{Duration, timeout};

/// Default timeout for a single MCP JSON-RPC request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Pending JSON-RPC response channels keyed by request id.
type PendingMap = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>>;

/// Description of a tool exposed by an MCP server.
#[derive(Debug, Clone)]
pub struct McpToolInfo {
    /// Tool name, prefixed with the owning server id (`{server_id}_{tool_name}`).
    pub name: String,
    /// Bare tool name as reported by the server.
    pub tool_name: String,
    /// Human-readable description.
    pub description: String,
    /// Server that owns the tool.
    pub server_id: i64,
    /// Whether invoking the tool may mutate external state.
    pub side_effects: bool,
    /// JSON schema describing the tool's input arguments.
    pub input_schema: Option<Value>,
}

/// MCP client manager backed by a real JSON-RPC client.
///
/// When constructed with a [`SqlitePool`] it also persists server
/// configurations to the `mcp_servers` table.
#[derive(Clone)]
pub struct McpClientManager {
    inner: Arc<Mutex<McpManagerInner>>,
}

struct McpManagerInner {
    configs: Vec<McpServerConfig>,
    pool: Option<SqlitePool>,
    clients: HashMap<i64, Box<dyn McpClient>>,
    tool_cache: HashMap<i64, Vec<McpToolInfo>>,
}

impl McpClientManager {
    /// Create an empty in-memory manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(McpManagerInner {
                configs: Vec::new(),
                pool: None,
                clients: HashMap::new(),
                tool_cache: HashMap::new(),
            })),
        }
    }

    /// Create a manager backed by `pool`, loading persisted configurations.
    ///
    /// # Errors
    ///
    /// Returns an error if the persisted configurations cannot be read.
    pub async fn new_with_pool(pool: SqlitePool) -> Result<Self, AppError> {
        let configs = Self::load_configs(&pool).await?;
        Ok(Self {
            inner: Arc::new(Mutex::new(McpManagerInner {
                configs,
                pool: Some(pool),
                clients: HashMap::new(),
                tool_cache: HashMap::new(),
            })),
        })
    }

    /// Add a configuration in memory.
    ///
    /// For persisted storage use [`Self::add_config_persisted`].
    pub async fn add_config(&mut self, config: McpServerConfig) {
        let mut inner = self.inner.lock().await;
        inner.tool_cache.remove(&config.id);
        inner.configs.push(config);
    }

    /// Add and persist a server configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if no pool is configured or persistence fails.
    pub async fn add_config_persisted(
        &mut self,
        mut config: McpServerConfig,
    ) -> Result<McpServerConfig, AppError> {
        let pool = {
            let inner = self.inner.lock().await;
            inner.pool.clone().ok_or_else(|| AppError::Internal {
                message: "McpClientManager has no database pool".to_owned(),
            })?
        };

        let args_json = serde_json::to_string(&config.args).map_err(|e| AppError::Internal {
            message: format!("serialize args failed: {e}"),
        })?;
        let env_json = serde_json::to_string(&config.env).map_err(|e| AppError::Internal {
            message: format!("serialize env failed: {e}"),
        })?;

        let id = sqlx::query(
            "INSERT INTO mcp_servers (name, transport, command, args, url, env, enabled)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&config.name)
        .bind(config.transport.as_str())
        .bind(&config.command)
        .bind(args_json)
        .bind(&config.url)
        .bind(env_json)
        .bind(i64::from(config.enabled))
        .execute(&pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("add_mcp_server failed: {e}"),
        })?
        .last_insert_rowid();

        config.id = id;

        let mut inner = self.inner.lock().await;
        inner.tool_cache.remove(&id);
        inner.configs.push(config.clone());
        drop(inner);
        Ok(config)
    }

    /// Remove a persisted server configuration by id.
    ///
    /// # Errors
    ///
    /// Returns an error if no pool is configured, the server is missing, or
    /// deletion fails.
    pub async fn remove_config(&mut self, id: i64) -> Result<(), AppError> {
        let pool = {
            let inner = self.inner.lock().await;
            inner.pool.clone().ok_or_else(|| AppError::Internal {
                message: "McpClientManager has no database pool".to_owned(),
            })?
        };

        let result = sqlx::query("DELETE FROM mcp_servers WHERE id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("remove_mcp_server failed: {e}"),
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                resource: format!("mcp server {id}"),
            });
        }

        let mut inner = self.inner.lock().await;
        inner.configs.retain(|c| c.id != id);
        inner.clients.remove(&id);
        inner.tool_cache.remove(&id);
        drop(inner);
        Ok(())
    }

    /// Return all recorded server configurations.
    #[must_use]
    pub async fn list_configs(&self) -> Vec<McpServerConfig> {
        self.inner.lock().await.configs.clone()
    }

    /// Clear cached tool lists so the next `list_tools` fetches fresh data.
    pub async fn refresh_tools(&self) {
        self.inner.lock().await.tool_cache.clear();
    }

    /// Return real tools from every enabled server.
    ///
    /// Tool lists are cached per server. Servers that cannot be reached are
    /// skipped and logged instead of failing the whole call.
    ///
    /// # Errors
    ///
    /// Returns an error only for unexpected internal failures.
    pub async fn list_tools(&self) -> Result<Vec<McpToolInfo>, AppError> {
        let mut inner = self.inner.lock().await;
        let mut tools = Vec::new();

        for config in inner.configs.clone() {
            if !config.enabled {
                continue;
            }

            if let Some(cached) = inner.tool_cache.get(&config.id) {
                tools.extend(cached.clone());
                continue;
            }

            match Self::client_for_server(&mut inner.clients, &config).await {
                Ok(client) => match client.list_tools().await {
                    Ok(server_tools) => {
                        let infos: Vec<McpToolInfo> = server_tools
                            .into_iter()
                            .map(|tool| McpToolInfo {
                                name: format!("{}_{}", config.id, tool.name),
                                tool_name: tool.name.clone(),
                                description: tool.description,
                                server_id: config.id,
                                side_effects: Self::is_write_tool(&tool.name),
                                input_schema: tool.input_schema,
                            })
                            .collect();
                        inner.tool_cache.insert(config.id, infos.clone());
                        tools.extend(infos);
                    }
                    Err(err) => {
                        tracing::warn!(
                            server_id = config.id,
                            server_name = %config.name,
                            error = %err,
                            "failed to list tools for MCP server"
                        );
                    }
                },
                Err(err) => {
                    tracing::warn!(
                        server_id = config.id,
                        server_name = %config.name,
                        error = %err,
                        "failed to connect to MCP server"
                    );
                }
            }
        }

        drop(inner);
        Ok(tools)
    }

    /// Invoke a real MCP tool by name.
    ///
    /// If `server_id` is provided the call is routed to that server and `name`
    /// is treated as the bare tool name. Otherwise the name is parsed as the
    /// prefixed form `{server_id}_{tool_name}` returned by [`Self::list_tools`].
    ///
    /// # Errors
    ///
    /// Returns an error if the server or tool is unknown, or the invocation fails.
    #[allow(clippy::significant_drop_tightening)]
    pub async fn invoke_tool(
        &self,
        name: &str,
        arguments: Value,
        server_id: Option<i64>,
    ) -> Result<Value, AppError> {
        let (server_id, tool_name) = match server_id {
            Some(id) => (id, name),
            None => parse_prefixed_name(name)?,
        };

        let mut inner = self.inner.lock().await;
        let config =
            inner.configs.iter().find(|c| c.id == server_id).cloned().ok_or_else(|| {
                AppError::NotFound {
                    resource: format!("mcp server {server_id}"),
                }
            })?;

        let client = Self::client_for_server(&mut inner.clients, &config).await?;
        client.call_tool(tool_name, arguments).await
    }

    /// Return true if the given tool name is considered a write/side-effect
    /// operation based on common naming heuristics.
    #[must_use]
    pub fn is_write_tool(name: &str) -> bool {
        let lower = name.to_lowercase();
        [
            "write", "create", "delete", "update", "modify", "append", "remove", "put", "post",
            "patch", "exec", "run", "execute", "shell", "send",
        ]
        .iter()
        .any(|suffix| lower.contains(suffix))
    }

    async fn load_configs(pool: &SqlitePool) -> Result<Vec<McpServerConfig>, AppError> {
        let rows = sqlx::query_as::<_, McpServerRow>(
            "SELECT id, name, transport, command, args, url, env, enabled
             FROM mcp_servers
             ORDER BY id ASC",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("load mcp configs failed: {e}"),
        })?;

        rows.into_iter().map(McpServerRow::try_into).collect()
    }

    async fn client_for_server<'a>(
        clients: &'a mut HashMap<i64, Box<dyn McpClient>>,
        config: &McpServerConfig,
    ) -> Result<&'a mut Box<dyn McpClient>, AppError> {
        match clients.entry(config.id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut client = Self::new_client(config)?;
                client.initialize().await?;
                Ok(entry.insert(client))
            }
            std::collections::hash_map::Entry::Occupied(entry) => Ok(entry.into_mut()),
        }
    }

    fn new_client(config: &McpServerConfig) -> Result<Box<dyn McpClient>, AppError> {
        match config.transport {
            McpTransport::Stdio => Ok(Box::new(StdioClient::new(config)?)),
            McpTransport::Http => Ok(Box::new(HttpClient::new(config)?)),
        }
    }
}

impl Default for McpClientManager {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_prefixed_name(name: &str) -> Result<(i64, &str), AppError> {
    let idx = name.find('_').ok_or_else(|| AppError::NotFound {
        resource: format!("mcp tool {name}"),
    })?;
    let prefix = &name[..idx];
    let id = prefix.parse::<i64>().map_err(|_| AppError::NotFound {
        resource: format!("mcp tool {name}"),
    })?;
    let tool_name = &name[idx + 1..];
    Ok((id, tool_name))
}

#[async_trait]
trait McpClient: Send {
    async fn initialize(&mut self) -> Result<Value, AppError>;
    async fn list_tools(&mut self) -> Result<Vec<McpTool>, AppError>;
    async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, AppError>;
}

#[derive(Debug, Deserialize)]
struct McpTool {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "inputSchema")]
    input_schema: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<i64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
    #[allow(dead_code)]
    data: Option<Value>,
}

fn internal_error(message: impl Into<String>) -> AppError {
    AppError::Internal {
        message: message.into(),
    }
}

// ---------------------------------------------------------------------------
// Stdio transport
// ---------------------------------------------------------------------------

struct StdioClient {
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    process: Option<Child>,
    stdin: Option<ChildStdin>,
    pending: PendingMap,
    next_id: AtomicI64,
    reader: Option<tokio::task::JoinHandle<()>>,
    initialized: bool,
}

impl StdioClient {
    fn new(config: &McpServerConfig) -> Result<Self, AppError> {
        let command = config
            .command
            .clone()
            .ok_or_else(|| internal_error("stdio MCP server is missing a command"))?;
        Ok(Self {
            command,
            args: config.args.clone(),
            env: config.env.clone(),
            process: None,
            stdin: None,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicI64::new(1),
            reader: None,
            initialized: false,
        })
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let line = message.to_string();
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| internal_error("MCP stdio process is not running"))?;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| internal_error(format!("failed to write to MCP stdin: {e}")))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| internal_error(format!("failed to write newline to MCP stdin: {e}")))?;
        stdin
            .flush()
            .await
            .map_err(|e| internal_error(format!("failed to flush MCP stdin: {e}")))?;

        match timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(message))) => Err(internal_error(message)),
            Ok(Err(_)) => Err(internal_error("MCP reader closed")),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(internal_error(format!("MCP request {method} timed out")))
            }
        }
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), AppError> {
        let message = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let line = message.to_string();
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| internal_error("MCP stdio process is not running"))?;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| internal_error(format!("failed to write MCP notification: {e}")))?;
        stdin.write_all(b"\n").await.map_err(|e| {
            internal_error(format!("failed to write MCP notification newline: {e}"))
        })?;
        stdin
            .flush()
            .await
            .map_err(|e| internal_error(format!("failed to flush MCP notification: {e}")))?;
        Ok(())
    }

    async fn reader_loop(stdout: ChildStdout, pending: PendingMap) {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&line) {
                if let Some(id) = response.id {
                    let tx = pending.lock().await.remove(&id);
                    if let Some(tx) = tx {
                        let outcome = if let Some(result) = response.result {
                            Ok(result)
                        } else if let Some(err) = response.error {
                            Err(err.message)
                        } else {
                            Ok(Value::Null)
                        };
                        let _ = tx.send(outcome);
                    }
                }
            }
        }
    }
}

impl Drop for StdioClient {
    fn drop(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.abort();
        }
        if let Some(mut child) = self.process.take() {
            let _ = child.start_kill();
        }
    }
}

#[async_trait]
impl McpClient for StdioClient {
    async fn initialize(&mut self) -> Result<Value, AppError> {
        if self.initialized {
            return Ok(Value::Null);
        }

        let mut child = Command::new(&self.command)
            .args(&self.args)
            .envs(&self.env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| internal_error(format!("failed to spawn MCP server: {e}")))?;

        let stdin = child.stdin.take().ok_or_else(|| internal_error("failed to open MCP stdin"))?;
        let stdout =
            child.stdout.take().ok_or_else(|| internal_error("failed to open MCP stdout"))?;
        self.process = Some(child);
        self.stdin = Some(stdin);

        self.reader = Some(tokio::spawn(Self::reader_loop(
            stdout,
            self.pending.clone(),
        )));

        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "portico", "version": env!("CARGO_PKG_VERSION") },
        });
        let _result = self.request("initialize", params).await?;

        self.notify("notifications/initialized", Value::Object(Map::new())).await?;
        self.initialized = true;
        Ok(Value::Null)
    }

    async fn list_tools(&mut self) -> Result<Vec<McpTool>, AppError> {
        let result = self.request("tools/list", Value::Object(Map::new())).await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|v| serde_json::from_value::<McpTool>(v.clone()).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(tools)
    }

    async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, AppError> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });
        self.request("tools/call", params).await
    }
}

// ---------------------------------------------------------------------------
// HTTP transport
// ---------------------------------------------------------------------------

struct HttpClient {
    client: reqwest::Client,
    url: String,
    session_id: Option<String>,
    initialized: bool,
    next_id: AtomicI64,
}

impl HttpClient {
    fn new(config: &McpServerConfig) -> Result<Self, AppError> {
        let url = config
            .url
            .clone()
            .ok_or_else(|| internal_error("HTTP MCP server is missing a URL"))?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| internal_error(format!("failed to build MCP HTTP client: {e}")))?;
        Ok(Self {
            client,
            url,
            session_id: None,
            initialized: false,
            next_id: AtomicI64::new(1),
        })
    }

    async fn post(
        &self,
        method: &str,
        params: Value,
        expect_response: bool,
    ) -> Result<Value, AppError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let mut builder = self.client.post(&self.url).json(&body);
        if let Some(session) = &self.session_id {
            builder = builder.header(
                reqwest::header::HeaderName::from_static("mcp-session-id"),
                session,
            );
        }

        let response = builder
            .send()
            .await
            .map_err(|e| internal_error(format!("HTTP MCP request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_else(|_| "<unreadable>".to_owned());
            return Err(internal_error(format!(
                "HTTP MCP request returned {status}: {text}"
            )));
        }

        if !expect_response {
            return Ok(Value::Null);
        }

        let payload: Value = response
            .json()
            .await
            .map_err(|e| internal_error(format!("failed to parse MCP HTTP response: {e}")))?;

        if let Some(error) = payload.get("error") {
            let message = error.get("message").and_then(Value::as_str).unwrap_or("MCP error");
            return Err(internal_error(message));
        }

        Ok(payload.get("result").cloned().unwrap_or(Value::Null))
    }
}

#[async_trait]
impl McpClient for HttpClient {
    async fn initialize(&mut self) -> Result<Value, AppError> {
        if self.initialized {
            return Ok(Value::Null);
        }

        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "portico", "version": env!("CARGO_PKG_VERSION") },
        });
        let result = self.post("initialize", params, true).await?;
        self.session_id =
            result.get("sessionId").and_then(Value::as_str).map(std::string::String::from);

        self.post(
            "notifications/initialized",
            Value::Object(Map::new()),
            false,
        )
        .await?;
        self.initialized = true;
        Ok(result)
    }

    async fn list_tools(&mut self) -> Result<Vec<McpTool>, AppError> {
        let result = self.post("tools/list", Value::Object(Map::new()), true).await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|v| serde_json::from_value::<McpTool>(v.clone()).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(tools)
    }

    async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, AppError> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });
        self.post("tools/call", params, true).await
    }
}

#[derive(sqlx::FromRow)]
struct McpServerRow {
    id: i64,
    name: String,
    transport: String,
    command: Option<String>,
    args: String,
    url: Option<String>,
    env: String,
    enabled: i64,
}

impl TryFrom<McpServerRow> for McpServerConfig {
    type Error = AppError;

    fn try_from(row: McpServerRow) -> Result<Self, Self::Error> {
        let args: Vec<String> =
            serde_json::from_str(&row.args).map_err(|e| AppError::Internal {
                message: format!("deserialize args failed: {e}"),
            })?;
        let env: HashMap<String, String> =
            serde_json::from_str(&row.env).map_err(|e| AppError::Internal {
                message: format!("deserialize env failed: {e}"),
            })?;

        Ok(Self {
            id: row.id,
            name: row.name,
            transport: row.transport.as_str().try_into()?,
            command: row.command,
            args,
            url: row.url,
            env,
            enabled: row.enabled != 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny Python MCP server used for unit tests.
    ///
    /// It reads line-delimited JSON-RPC from stdin and responds to
    /// `initialize`, `notifications/initialized`, `tools/list` and `tools/call`
    /// with hard-coded replies.
    const ECHO_MCP_SERVER: &str = r#"import sys, json
for line in sys.stdin:
    try:
        msg = json.loads(line)
        method = msg.get("method")
        req_id = msg.get("id")
        if method == "initialize":
            print(f'{{"jsonrpc":"2.0","id":{req_id},"result":{{"protocolVersion":"2024-11-05","capabilities":{{}},"serverInfo":{{"name":"echo"}}}}}}', flush=True)
        elif method == "tools/list":
            print(f'{{"jsonrpc":"2.0","id":{req_id},"result":{{"tools":[{{"name":"echo","description":"Echo input","inputSchema":{{"type":"object","properties":{{"input":{{"type":"string"}}}}}}}},{{"name":"write_file","description":"Write a file"}}]}}}}', flush=True)
        elif method == "tools/call":
            print(f'{{"jsonrpc":"2.0","id":{req_id},"result":{{"content":[{{"type":"text","text":"pong"}}]}}}}', flush=True)
    except Exception:
        pass"#;

    fn stdio_config(id: i64, name: &str) -> McpServerConfig {
        McpServerConfig {
            id,
            name: name.to_owned(),
            transport: McpTransport::Stdio,
            command: Some("python3".to_owned()),
            args: vec!["-c".to_owned(), ECHO_MCP_SERVER.to_owned()],
            url: None,
            env: HashMap::new(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn manager_lists_real_tools_from_stdio_server() {
        let mut manager = McpClientManager::new();
        manager.add_config(stdio_config(1, "echo")).await;

        let tools = manager.list_tools().await.expect("list tools");
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().any(|t| t.name == "1_echo" && !t.side_effects));
        assert!(tools.iter().any(|t| {
            t.name == "1_echo"
                && t.tool_name == "echo"
                && t.input_schema.as_ref().is_some_and(|s| s.get("type").is_some())
        }));
        assert!(tools.iter().any(|t| t.name == "1_write_file" && t.side_effects));
        assert!(tools.iter().all(|t| t.server_id == 1));
    }

    #[tokio::test]
    async fn disabled_and_http_servers_produce_no_tools() {
        let mut manager = McpClientManager::new();
        manager
            .add_config(McpServerConfig {
                id: 1,
                name: "disabled".to_owned(),
                transport: McpTransport::Stdio,
                command: Some("/bin/sh".to_owned()),
                args: vec!["-c".to_owned(), ECHO_MCP_SERVER.to_owned()],
                url: None,
                env: HashMap::new(),
                enabled: false,
            })
            .await;
        manager
            .add_config(McpServerConfig {
                id: 2,
                name: "web".to_owned(),
                transport: McpTransport::Http,
                command: None,
                args: vec![],
                url: Some("http://localhost:9".to_owned()),
                env: HashMap::new(),
                enabled: true,
            })
            .await;

        // The HTTP server is unavailable, so it is skipped and the list is empty.
        let tools = manager.list_tools().await.expect("list tools");
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn manager_invokes_real_stdio_tool() {
        let mut manager = McpClientManager::new();
        manager.add_config(stdio_config(2, "echo")).await;

        let result = manager
            .invoke_tool("2_echo", serde_json::json!({"input": "ping"}), None)
            .await
            .expect("invoke tool");

        assert_eq!(result["content"][0]["text"], "pong");
    }

    #[tokio::test]
    async fn invoke_unknown_tool_returns_not_found() {
        let mut manager = McpClientManager::new();
        manager.add_config(stdio_config(3, "echo")).await;

        let err = manager
            .invoke_tool("no_underscore", Value::Null, None)
            .await
            .expect_err("expected not found");
        assert!(matches!(err, AppError::NotFound { .. }));

        let err = manager
            .invoke_tool("999_unknown", Value::Null, None)
            .await
            .expect_err("expected not found");
        assert!(matches!(err, AppError::NotFound { .. }));
    }

    #[test]
    fn write_tool_detection() {
        assert!(McpClientManager::is_write_tool("write_file"));
        assert!(McpClientManager::is_write_tool("deleteItem"));
        assert!(!McpClientManager::is_write_tool("read_file"));
        assert!(!McpClientManager::is_write_tool("list_directory"));
    }
}
