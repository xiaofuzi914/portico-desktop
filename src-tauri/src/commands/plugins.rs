//! Plugin, skill, and MCP Tauri commands.

use app_models::{AppError, McpServerConfig, PluginId, PluginManifest, Skill};
use app_security::{PermissionRequest, PermissionResult};
use serde_json::Value;
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Install or update a plugin from its manifest.
///
/// The manifest includes declared permissions so the frontend can surface them
/// before calling this command.
///
/// # Errors
///
/// Returns an error response if the manifest cannot be persisted.
#[tauri::command]
pub async fn install_plugin(
    state: State<'_, AppState>,
    manifest: PluginManifest,
) -> Result<ApiResponse<PluginManifest>, String> {
    Ok(
        match state.runtime.plugin_registry().install_plugin(manifest).await {
            Ok(manifest) => ApiResponse::ok(manifest),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List all installed plugins.
///
/// # Errors
///
/// Returns an error response if plugins cannot be listed.
#[tauri::command]
pub async fn list_plugins(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<PluginManifest>>, String> {
    Ok(match state.runtime.plugin_registry().list_plugins().await {
        Ok(plugins) => ApiResponse::ok(plugins),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Enable or disable a plugin.
///
/// # Errors
///
/// Returns an error response if the plugin is missing or cannot be updated.
#[tauri::command]
pub async fn enable_plugin(
    state: State<'_, AppState>,
    id: PluginId,
    enabled: bool,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state.runtime.plugin_registry().enable_plugin(id, enabled).await {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Uninstall a plugin and its registered skills.
///
/// # Errors
///
/// Returns an error response if the plugin is missing or cannot be removed.
#[tauri::command]
pub async fn uninstall_plugin(
    state: State<'_, AppState>,
    id: PluginId,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state.runtime.plugin_registry().uninstall_plugin(id).await {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List skills, optionally filtered to a plugin.
///
/// # Errors
///
/// Returns an error response if skills cannot be listed.
#[tauri::command]
pub async fn list_skills(
    state: State<'_, AppState>,
    plugin_id: Option<PluginId>,
) -> Result<ApiResponse<Vec<Skill>>, String> {
    Ok(
        match state.runtime.plugin_registry().list_skills(plugin_id).await {
            Ok(skills) => ApiResponse::ok(skills),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List configured MCP servers.
///
/// # Errors
///
/// Returns an error response if MCP servers cannot be listed.
#[tauri::command]
pub async fn list_mcp_servers(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<McpServerConfig>>, String> {
    Ok(
        match state.runtime.plugin_registry().list_mcp_servers().await {
            Ok(servers) => ApiResponse::ok(servers),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Add a new MCP server configuration.
///
/// # Errors
///
/// Returns an error response if the configuration cannot be persisted.
#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, AppState>,
    config: McpServerConfig,
) -> Result<ApiResponse<McpServerConfig>, String> {
    let manager = state.runtime.mcp_manager();
    let mut guard = manager.lock().await;
    Ok(match guard.add_config_persisted(config).await {
        Ok(config) => ApiResponse::ok(config),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Remove an MCP server configuration by id.
///
/// # Errors
///
/// Returns an error response if the server is missing or cannot be removed.
#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, AppState>,
    id: i64,
) -> Result<ApiResponse<()>, String> {
    let manager = state.runtime.mcp_manager();
    let mut guard = manager.lock().await;
    Ok(match guard.remove_config(id).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// List tools exposed by enabled MCP servers.
///
/// # Errors
///
/// Returns an error response if tools cannot be listed.
#[tauri::command]
pub async fn list_mcp_tools(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<McpToolSummary>>, String> {
    let tools = {
        let manager = state.runtime.mcp_manager();
        let guard = manager.lock().await;
        guard.list_tools().await
    }
    .map_err(|err| err.to_string())?;
    let summaries: Vec<McpToolSummary> = tools
        .into_iter()
        .map(|t| McpToolSummary {
            name: t.name,
            description: t.description,
            server_id: t.server_id,
            side_effects: t.side_effects,
        })
        .collect();
    Ok(ApiResponse::ok(summaries))
}

/// Invoke an MCP tool by name, routing the request through the permission engine.
///
/// Side-effect tools default to requiring approval; the command returns a
/// permission-denied error when approval is required or the engine denies the
/// action.
///
/// # Errors
///
/// Returns an error response if the tool is unknown, permission is denied, or
/// invocation fails.
#[tauri::command]
pub async fn invoke_mcp_tool(
    state: State<'_, AppState>,
    workspace_id: app_models::WorkspaceId,
    server_id: Option<i64>,
    name: String,
    arguments: Value,
) -> Result<ApiResponse<Value>, String> {
    let action = if app_runtime::McpClientManager::is_write_tool(&name) {
        "mcp.invoke.write"
    } else {
        "mcp.invoke.read"
    };

    let request = PermissionRequest {
        workspace_id,
        thread_id: None,
        run_id: None,
        action: action.to_owned(),
        resource: name.clone(),
        trusted_workspace: false,
    };

    match state.runtime.evaluate_permission(request) {
        PermissionResult::Allowed => {}
        PermissionResult::Ask { request } => {
            return Ok(ApiResponse::err(
                AppError::PermissionDenied {
                    reason: format!(
                        "approval required for {} on {}",
                        request.action, request.resource
                    ),
                }
                .to_string(),
            ));
        }
        PermissionResult::Denied { reason } => {
            return Ok(ApiResponse::err(
                AppError::PermissionDenied { reason }.to_string(),
            ));
        }
    }

    let result = {
        let manager = state.runtime.mcp_manager();
        let guard = manager.lock().await;
        guard.invoke_tool(&name, arguments, server_id).await
    };
    Ok(match result {
        Ok(value) => ApiResponse::ok(value),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Frontend-friendly summary of an MCP tool.
#[derive(Debug, serde::Serialize)]
pub struct McpToolSummary {
    /// Tool name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Server that owns the tool.
    pub server_id: i64,
    /// Whether invoking the tool may mutate external state.
    pub side_effects: bool,
}
