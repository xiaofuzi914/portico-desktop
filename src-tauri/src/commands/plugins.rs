//! Plugin, skill, and MCP Tauri commands.

use app_models::{AppError, McpServerConfig, PluginId, PluginManifest, Skill};
use app_security::{PermissionRequest, PermissionResult};
use autoagents_adapter::McpToolAdapter;
use serde_json::Value;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;
use crate::plugin_package;
use crate::portico_paths::{AvailablePluginPackage, PorticoUserDirs};

/// Return `~/.portico` layout paths (created at app start).
#[tauri::command]
pub async fn get_portico_user_dirs(
    state: State<'_, AppState>,
) -> Result<ApiResponse<PorticoUserDirs>, String> {
    Ok(ApiResponse::ok(state.portico_user_dirs.clone()))
}

/// List drop-in packages under `~/.portico/plugins/available`.
#[tauri::command]
pub async fn list_available_plugin_packages(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<AvailablePluginPackage>>, String> {
    Ok(
        match crate::portico_paths::list_available_packages(
            &state.portico_user_dirs.plugins_available,
        ) {
            Ok(packages) => ApiResponse::ok(packages),
            Err(error) => ApiResponse::err(error),
        },
    )
}

/// Install a package from `~/.portico/plugins/available/{folder_name}`.
#[tauri::command]
pub async fn install_available_plugin_package(
    state: State<'_, AppState>,
    folder_name: String,
) -> Result<ApiResponse<PluginManifest>, String> {
    let safe = match validate_available_folder_name(&folder_name) {
        Ok(name) => name,
        Err(error) => return Ok(ApiResponse::err(error)),
    };
    let source = state.portico_user_dirs.plugins_available.join(&safe);
    if !source.is_dir() {
        return Ok(ApiResponse::err(format!(
            "package not found in available plugins: {safe}"
        )));
    }
    install_plugin_package_from_path(state, source).await
}

/// One-click install a built-in catalog package (no folder picker).
///
/// Seeds the package into `~/.portico/plugins/available` from the monorepo /
/// resource catalog when needed, then installs into `installed/`.
#[tauri::command]
pub async fn install_catalog_plugin(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    package_name: String,
) -> Result<ApiResponse<PluginManifest>, String> {
    let progress = crate::plugin_github::InstallProgress::new(app);
    progress.info("start", format!("local catalog install: {package_name}"));
    let safe = match validate_available_folder_name(&package_name) {
        Ok(name) => name,
        Err(error) => {
            progress.error("failed", &error);
            return Ok(ApiResponse::err(error));
        }
    };
    let available_root = state.portico_user_dirs.plugins_available.clone();
    progress.info("seed", "copying monorepo / resource package into available/…");
    let seed_result = tauri::async_runtime::spawn_blocking(move || {
        crate::portico_paths::seed_catalog_package_to_available(&available_root, &safe)
    })
    .await
    .map_err(|error| format!("catalog seed task failed: {error}"));
    let source = match seed_result {
        Ok(Ok(path)) => {
            progress.ok("seed", format!("package at {}", path.display()));
            path
        }
        Ok(Err(error)) | Err(error) => {
            progress.error("failed", &error);
            return Ok(ApiResponse::err(error));
        }
    };
    progress.info("register", "registering in database…");
    let result = install_plugin_package_from_path(state, source).await;
    match &result {
        Ok(resp) if resp.success => {
            if let Some(m) = &resp.data {
                progress.ok("done", format!("installed {} v{}", m.display_name, m.version));
            }
        }
        Ok(resp) => progress.error("failed", resp.error.as_deref().unwrap_or("install failed")),
        Err(e) => progress.error("failed", e),
    }
    result
}

/// List built-in plugin sources (`examples/plugins/sources.json`).
#[tauri::command]
pub async fn list_plugin_sources() -> Result<ApiResponse<Vec<crate::plugin_github::PluginSourceEntry>>, String> {
    Ok(match crate::plugin_github::load_plugin_sources() {
        Ok(sources) => ApiResponse::ok(sources),
        Err(error) => ApiResponse::err(error),
    })
}

/// Closed-loop install from a GitHub URL or catalog source name.
///
/// Example:
/// - `https://github.com/markdown-viewer/markdown-viewer-extension`
/// - `markdown-viewer-provider` (catalog name from sources.json)
#[tauri::command]
pub async fn install_plugin_from_github(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    source: String,
    git_ref: Option<String>,
) -> Result<ApiResponse<PluginManifest>, String> {
    let dirs = state.portico_user_dirs.clone();
    let source_owned = source.clone();
    let git_ref_owned = git_ref.clone();
    let progress = crate::plugin_github::InstallProgress::new(app);
    progress.info("start", format!("install from: {source_owned}"));
    let progress_for_task = progress.clone();
    let materialize = tauri::async_runtime::spawn_blocking(move || {
        crate::plugin_github::resolve_and_materialize(
            &progress_for_task,
            &dirs,
            &source_owned,
            git_ref_owned.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("github install task failed: {e}"))?;

    let (package_path, _entry) = match materialize {
        Ok(v) => v,
        Err(error) => {
            progress.error("failed", &error);
            return Ok(ApiResponse::err(error));
        }
    };

    progress.info("register", "registering plugin in database…");
    let result = install_plugin_package_from_path(state, package_path).await;
    match &result {
        Ok(resp) if resp.success => {
            if let Some(m) = &resp.data {
                progress.ok(
                    "done",
                    format!("installed {} v{}", m.display_name, m.version),
                );
            } else {
                progress.ok("done", "install finished");
            }
        }
        Ok(resp) => {
            progress.error(
                "failed",
                resp.error.as_deref().unwrap_or("install failed"),
            );
        }
        Err(e) => progress.error("failed", e),
    }
    result
}

/// Ensure bundled catalog packages appear under `available/` for one-click install.
#[tauri::command]
pub async fn seed_catalog_plugins(
    state: State<'_, AppState>,
) -> Result<ApiResponse<u64>, String> {
    let available_root = state.portico_user_dirs.plugins_available.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::portico_paths::seed_all_catalog_packages(&available_root)
    })
    .await
    .map_err(|error| format!("catalog seed task failed: {error}"));
    match result {
        Ok(Ok(count)) => Ok(ApiResponse::ok(count as u64)),
        Ok(Err(error)) | Err(error) => Ok(ApiResponse::err(error)),
    }
}

/// Open `~/.portico/plugins` (or available/installed) in the OS file manager.
#[tauri::command]
pub async fn open_portico_plugins_dir(
    state: State<'_, AppState>,
    which: Option<String>,
) -> Result<ApiResponse<()>, String> {
    let dirs = &state.portico_user_dirs;
    let target = match which.as_deref() {
        Some("available") => dirs.plugins_available.clone(),
        Some("installed") => dirs.plugins_installed.clone(),
        _ => dirs.plugins.clone(),
    };
    match open_directory_in_os(&target) {
        Ok(()) => Ok(ApiResponse::ok(())),
        Err(error) => Ok(ApiResponse::err(error)),
    }
}

fn validate_available_folder_name(name: &str) -> Result<String, String> {
    let path = Path::new(name);
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("invalid available plugin folder name".to_owned());
    }
    Ok(name.to_owned())
}

fn open_directory_in_os(path: &Path) -> Result<(), String> {
    let status = {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open").arg(path).status()
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer").arg(path).status()
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            std::process::Command::new("xdg-open").arg(path).status()
        }
    }
    .map_err(|error| format!("open plugins directory failed: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("open plugins directory exited with {status}"))
    }
}

async fn install_plugin_package_from_path(
    state: State<'_, AppState>,
    source: PathBuf,
) -> Result<ApiResponse<PluginManifest>, String> {
    // Read id first so we can replace an existing install cleanly.
    let preview_manifest = match tauri::async_runtime::spawn_blocking({
        let source = source.clone();
        move || plugin_package::read_manifest_public(&source)
    })
    .await
    {
        Ok(Ok(m)) => m,
        Ok(Err(error)) => {
            return Ok(ApiResponse::err(format!("read plugin manifest failed: {error}")));
        }
        Err(error) => {
            return Ok(ApiResponse::err(format!("read plugin manifest task failed: {error}")));
        }
    };

    // Real reinstall: drop DB row + files before copying a fresh package.
    if let Ok(existing) = state.runtime.plugin_registry().list_plugins().await {
        if let Some(previous) = existing.into_iter().find(|p| p.id == preview_manifest.id) {
            if let Err(error) = purge_plugin_files(&state, &previous).await {
                return Ok(ApiResponse::err(error));
            }
            let _ = state
                .runtime
                .plugin_registry()
                .uninstall_plugin(previous.id)
                .await;
        }
    }

    let plugins_root = state.portico_user_dirs.plugins_installed.clone();
    let installed = tauri::async_runtime::spawn_blocking(move || {
        plugin_package::install_directory(&source, &plugins_root)
    })
    .await
    .map_err(|error| format!("plugin installation task failed: {error}"));

    let (manifest, installed_path) = match installed {
        Ok(Ok(value)) => value,
        Ok(Err(error)) | Err(error) => return Ok(ApiResponse::err(error)),
    };

    match state.runtime.plugin_registry().install_plugin(manifest).await {
        Ok(manifest) => {
            // Verify files actually exist after install.
            if !installed_path.is_dir() {
                let _ = state
                    .runtime
                    .plugin_registry()
                    .uninstall_plugin(manifest.id)
                    .await;
                return Ok(ApiResponse::err(format!(
                    "install reported success but package directory is missing: {}",
                    installed_path.display()
                )));
            }
            Ok(ApiResponse::ok(manifest))
        }
        Err(error) => {
            let _ = plugin_package::remove_path_if_present(&installed_path);
            Ok(ApiResponse::err(error.to_string()))
        }
    }
}

/// Delete package files for a plugin (recorded path + installed/{id} + staging leftovers).
async fn purge_plugin_files(
    state: &State<'_, AppState>,
    plugin: &app_models::PluginManifest,
) -> Result<(), String> {
    let id = plugin.id.0.to_string();
    let plugins_root = state.portico_user_dirs.plugins_installed.clone();
    let plugins_tree = state.portico_user_dirs.plugins.clone();
    let recorded = plugin.install_path.clone().map(PathBuf::from);
    tauri::async_runtime::spawn_blocking(move || {
        plugin_package::remove_installed_package(&plugins_root, &id)?;
        if let Some(path) = recorded {
            // Only delete paths under the user plugins tree for safety.
            if path.starts_with(&plugins_tree) || path.starts_with(&plugins_root) {
                plugin_package::remove_path_if_present(&path)?;
            }
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("plugin file purge task failed: {error}"))?
}

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
    if manifest.entrypoint.is_some()
        || manifest.install_path.is_some()
        || !manifest.capabilities.is_empty()
    {
        return Ok(ApiResponse::err(
            "UI-capability plugins must be installed from a validated plugin package",
        ));
    }
    Ok(
        match state.runtime.plugin_registry().install_plugin(manifest).await {
            Ok(manifest) => ApiResponse::ok(manifest),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Install a plugin package selected by the user.
///
/// Packages are copied into `~/.portico/plugins/installed` only after their
/// manifest and complete directory tree pass validation.
///
/// # Errors
///
/// Returns an error if the blocking installation task cannot be joined.
#[tauri::command]
pub async fn install_plugin_package(
    state: State<'_, AppState>,
    source_path: String,
) -> Result<ApiResponse<PluginManifest>, String> {
    install_plugin_package_from_path(state, PathBuf::from(source_path)).await
}

/// List all installed plugins.
///
/// Ensures `install_path` is populated for packages under the default
/// `~/.portico/plugins/installed/{id}` tree so the UI can resolve entrypoints
/// even if an older row stored null.
///
/// # Errors
///
/// Returns an error response if plugins cannot be listed.
///
/// Version (and other package identity fields) prefer the on-disk
/// `plugin.json` under `install_path` when present — the package is the
/// source of truth, not a stale DB row after a partial file update.
#[tauri::command]
pub async fn list_plugins(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<PluginManifest>>, String> {
    Ok(match state.runtime.plugin_registry().list_plugins().await {
        Ok(mut plugins) => {
            let installed_root = state.portico_user_dirs.plugins_installed.clone();
            for plugin in &mut plugins {
                if plugin.install_path.is_none() {
                    let fallback = installed_root.join(plugin.id.0.to_string());
                    if fallback.is_dir() {
                        plugin.install_path =
                            Some(fallback.to_string_lossy().into_owned());
                    }
                }
                if let Some(path) = plugin.install_path.clone() {
                    apply_package_manifest_fields(plugin, Path::new(&path));
                }
            }
            ApiResponse::ok(plugins)
        }
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Overlay identity fields from the installed package's own `plugin.json`.
fn apply_package_manifest_fields(plugin: &mut PluginManifest, install_path: &Path) {
    let Ok(package) = plugin_package::read_manifest_public(install_path) else {
        return;
    };
    // Always trust the package file for version — this is "插件使用其自身的版本".
    if !package.version.is_empty() {
        plugin.version = package.version;
    }
    if !package.display_name.is_empty() {
        plugin.display_name = package.display_name;
    }
    if !package.description.is_empty() {
        plugin.description = package.description;
    }
    if !package.name.is_empty() {
        plugin.name = package.name;
    }
    if package.entrypoint.is_some() {
        plugin.entrypoint = package.entrypoint;
    }
    if !package.capabilities.is_empty() {
        plugin.capabilities = package.capabilities;
    }
}

/// Read a plugin package text file (HTML/JS/CSS) for host-side srcdoc boot.
///
/// Only allows paths under `~/.portico/plugins/installed`. Rejects traversal.
#[tauri::command]
pub async fn read_plugin_entrypoint_html(
    state: State<'_, AppState>,
    install_path: String,
    entrypoint: String,
) -> Result<ApiResponse<String>, String> {
    let installed_root = state.portico_user_dirs.plugins_installed.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        read_plugin_package_text(&installed_root, &install_path, &entrypoint, 2 * 1024 * 1024)
    })
    .await
    .map_err(|error| format!("read plugin entrypoint task failed: {error}"))?;
    Ok(match result {
        Ok(html) => ApiResponse::ok(html),
        Err(error) => ApiResponse::err(error),
    })
}

/// Read a small package-relative text asset (e.g. `provider.js`, `styles.css`).
#[tauri::command]
pub async fn read_plugin_package_text_file(
    state: State<'_, AppState>,
    install_path: String,
    relative_path: String,
) -> Result<ApiResponse<String>, String> {
    let installed_root = state.portico_user_dirs.plugins_installed.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        read_plugin_package_text(
            &installed_root,
            &install_path,
            &relative_path,
            512 * 1024,
        )
    })
    .await
    .map_err(|error| format!("read plugin package text task failed: {error}"))?;
    Ok(match result {
        Ok(text) => ApiResponse::ok(text),
        Err(error) => ApiResponse::err(error),
    })
}

fn read_plugin_package_text(
    installed_root: &Path,
    install_path: &str,
    relative: &str,
    max_bytes: u64,
) -> Result<String, String> {
    if relative.is_empty()
        || relative.contains("..")
        || Path::new(relative)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("invalid plugin relative path".to_owned());
    }
    let lower = relative.to_ascii_lowercase();
    if !(lower.ends_with(".html")
        || lower.ends_with(".htm")
        || lower.ends_with(".js")
        || lower.ends_with(".css")
        || lower.ends_with(".mjs"))
    {
        return Err("plugin text file must be html/js/css".to_owned());
    }

    let install = PathBuf::from(install_path);
    let install_canon = install
        .canonicalize()
        .map_err(|error| format!("plugin install path not found: {error}"))?;
    let root_canon = installed_root
        .canonicalize()
        .map_err(|error| format!("plugins installed root missing: {error}"))?;

    if !install_canon.starts_with(&root_canon) {
        return Err("plugin install path is outside the plugins store".to_owned());
    }

    let file = install_canon.join(relative);
    let file_canon = file
        .canonicalize()
        .map_err(|error| format!("plugin file not found: {error}"))?;
    if !file_canon.starts_with(&install_canon) {
        return Err("plugin file escapes install directory".to_owned());
    }

    let meta = std::fs::metadata(&file_canon)
        .map_err(|error| format!("failed to stat plugin file: {error}"))?;
    if !meta.is_file() || meta.len() > max_bytes {
        return Err(format!(
            "plugin file must be a regular file ≤ {max_bytes} bytes"
        ));
    }

    std::fs::read_to_string(&file_canon)
        .map_err(|error| format!("failed to read plugin file: {error}"))
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

/// Uninstall a plugin: delete DB row and remove package files under
/// `~/.portico/plugins/installed/{id}` (and recorded install_path when safe).
///
/// # Errors
///
/// Returns an error response if the plugin is missing or files cannot be removed.
#[tauri::command]
pub async fn uninstall_plugin(
    state: State<'_, AppState>,
    id: PluginId,
) -> Result<ApiResponse<()>, String> {
    let existing = match state.runtime.plugin_registry().list_plugins().await {
        Ok(plugins) => plugins.into_iter().find(|plugin| plugin.id == id),
        Err(error) => return Ok(ApiResponse::err(error.to_string())),
    };

    // Always attempt filesystem cleanup even if the DB row is already gone
    // (heals half-failed previous uninstalls).
    let default_path = state
        .portico_user_dirs
        .plugins_installed
        .join(id.0.to_string())
        .to_string_lossy()
        .into_owned();
    let cleanup_target = match existing {
        Some(mut plugin) => {
            if plugin.install_path.is_none() {
                plugin.install_path = Some(default_path);
            }
            plugin
        }
        None => app_models::PluginManifest {
            id,
            name: String::new(),
            version: String::new(),
            display_name: String::new(),
            description: String::new(),
            skills: Vec::new(),
            tools: Vec::new(),
            entrypoint: None,
            capabilities: Vec::new(),
            install_path: Some(default_path),
            permissions: app_models::PluginPermissions {
                network: Vec::new(),
                filesystem: "none".to_owned(),
            },
            enabled: false,
            installed_at: chrono::Utc::now(),
        },
    };

    if let Err(error) = purge_plugin_files(&state, &cleanup_target).await {
        return Ok(ApiResponse::err(error));
    }

    match state.runtime.plugin_registry().uninstall_plugin(id).await {
        Ok(()) => Ok(ApiResponse::ok(())),
        Err(AppError::NotFound { .. }) => {
            // Files already purged; treat missing DB row as success after cleanup.
            Ok(ApiResponse::ok(()))
        }
        Err(error) => Ok(ApiResponse::err(error.to_string())),
    }
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

/// Refresh the list of tools exposed by enabled MCP servers and re-register them
/// with the agent tool registry.
///
/// # Errors
///
/// Returns an error response if MCP tools cannot be listed.
#[tauri::command]
pub async fn refresh_mcp_tools(state: State<'_, AppState>) -> Result<ApiResponse<()>, String> {
    let manager = state.runtime.mcp_manager();
    let tools = {
        let guard = manager.lock().await;
        guard.refresh_tools().await;
        guard.list_tools().await
    }
    .map_err(|err| err.to_string())?;

    let manager_arc = {
        let guard = manager.lock().await;
        Arc::new(guard.clone())
    };

    // Drop stale MCP tools (prefixed with `{numeric_server_id}_`) before
    // re-registering the current set.
    state.tool_registry.retain(|name, _| {
        name.split_once('_').is_none_or(|(prefix, _)| prefix.parse::<i64>().is_err())
    });

    for tool in tools {
        state.tool_registry.register(Arc::new(McpToolAdapter::new(
            tool,
            Arc::clone(&manager_arc),
            state.security.clone(),
        )));
    }

    Ok(ApiResponse::ok(()))
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
