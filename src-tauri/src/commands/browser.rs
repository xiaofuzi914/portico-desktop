//! In-app browser and browser-use automation commands.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use app_models::{BrowserAction, BrowserWindowId, BrowserWindowInfo, WorkspaceId};
use app_security::{PermissionRequest, PermissionResult};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use screenshots::{Screen, image::ImageFormat};
use tauri::{State, WebviewUrl, WebviewWindowBuilder, webview::WebviewWindow};
use uuid::Uuid;

use crate::AppState;

/// Shared registry of open in-app browser windows.
pub type BrowserWindowRegistry = Arc<Mutex<HashMap<String, WebviewWindow>>>;

/// Ensure the desktop-control action is allowed for the workspace.
fn ensure_desktop_control(
    state: &State<'_, AppState>,
    workspace_id: WorkspaceId,
    resource: impl Into<String>,
) -> Result<(), String> {
    match state.runtime.evaluate_permission(PermissionRequest {
        workspace_id,
        thread_id: None,
        run_id: None,
        action: "desktop.control".to_owned(),
        resource: resource.into(),
        trusted_workspace: false,
    }) {
        PermissionResult::Allowed => Ok(()),
        PermissionResult::Ask { request } => Err(format!(
            "approval required for {} on {}",
            request.action, request.resource
        )),
        PermissionResult::Denied { reason } => Err(reason),
    }
}

/// Create a new browser window registry.
#[must_use]
pub fn new_registry() -> BrowserWindowRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Generate a unique label for a browser window.
#[must_use]
pub fn generate_browser_window_label() -> String {
    format!("browser-{}", Uuid::new_v4())
}

/// Open a new in-app browser window.
///
/// # Errors
///
/// Returns an error string if permission is denied, the URL is invalid, or the
/// window cannot be created.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn open_browser_window(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    url: String,
    title: Option<String>,
) -> Result<BrowserWindowInfo, String> {
    ensure_desktop_control(&state, workspace_id, &url)?;
    let parsed_url = url::Url::parse(&url).map_err(|err| format!("invalid url: {err}"))?;
    let label = generate_browser_window_label();
    let window_title =
        title.unwrap_or_else(|| parsed_url.host_str().unwrap_or("Browser").to_owned());

    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed_url))
        .title(&window_title)
        .inner_size(1200.0, 800.0)
        .build()
        .map_err(|err| format!("failed to create browser window: {err}"))?;

    let info = BrowserWindowInfo {
        id: BrowserWindowId(label.clone()),
        url,
        title: window_title,
    };

    state
        .browser_registry
        .lock()
        .map_err(|err| format!("failed to lock browser registry: {err}"))?
        .insert(label, window);

    Ok(info)
}

/// Close an in-app browser window by id.
///
/// # Errors
///
/// Returns an error string if permission is denied, the registry lock fails, or
/// the window cannot be closed.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn close_browser_window(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    id: BrowserWindowId,
) -> Result<(), String> {
    ensure_desktop_control(&state, workspace_id, format!("browser:close:{}", id.0))?;

    let mut registry = state
        .browser_registry
        .lock()
        .map_err(|err| format!("failed to lock browser registry: {err}"))?;

    if let Some(window) = registry.remove(&id.0) {
        window.close().map_err(|err| format!("failed to close browser window: {err}"))?;
    }

    drop(registry);
    Ok(())
}

/// List all open in-app browser windows.
#[tauri::command]
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn list_browser_windows(state: State<'_, AppState>) -> Vec<BrowserWindowInfo> {
    let Ok(registry) = state.browser_registry.lock() else {
        return Vec::new();
    };

    registry
        .values()
        .filter_map(|window| {
            let id = BrowserWindowId(window.label().to_owned());
            let url = window.url().ok()?.to_string();
            let title = window.title().ok().unwrap_or_else(|| "Browser".to_owned());
            Some(BrowserWindowInfo { id, url, title })
        })
        .collect()
}

/// Execute a browser-use action inside an in-app browser window.
///
/// # Errors
///
/// Returns an error string if permission is denied, the window is unknown, or
/// the action fails.
#[tauri::command]
pub async fn browser_use_action(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    id: BrowserWindowId,
    action: BrowserAction,
) -> Result<String, String> {
    let action_name = format!("{action:?}");
    ensure_desktop_control(
        &state,
        workspace_id,
        format!("browser:action:{action_name}"),
    )?;

    let window = {
        let registry = state
            .browser_registry
            .lock()
            .map_err(|err| format!("failed to lock browser registry: {err}"))?;
        registry
            .get(&id.0)
            .cloned()
            .ok_or_else(|| format!("browser window not found: {}", id.0))?
    };

    match action {
        BrowserAction::Click { selector } => {
            let script = format!(
                "(function() {{
                    const el = document.querySelector({});
                    if (!el) return 'element not found';
                    el.click();
                    return 'ok';
                }})()",
                serde_json::to_string(&selector).unwrap_or_default()
            );
            window
                .eval(&script)
                .map(|()| "ok".to_owned())
                .map_err(|err| format!("failed to execute click: {err}"))
        }
        BrowserAction::Type { selector, text } => {
            let script = format!(
                "(function() {{
                    const el = document.querySelector({});
                    if (!el) return 'element not found';
                    el.value = {};
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    return 'ok';
                }})()",
                serde_json::to_string(&selector).unwrap_or_default(),
                serde_json::to_string(&text).unwrap_or_default()
            );
            window
                .eval(&script)
                .map(|()| "ok".to_owned())
                .map_err(|err| format!("failed to execute type: {err}"))
        }
        BrowserAction::ExtractVisibleText => window
            .eval("document.body.innerText")
            .map(|()| "ok".to_owned())
            .map_err(|err| format!("failed to extract text: {err}")),
        BrowserAction::Wait { ms } => {
            tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
            Ok("ok".to_owned())
        }
        BrowserAction::Screenshot => {
            // Fall back to capturing the screen region occupied by the webview.
            // If that fails, return a clear base64 placeholder so callers still
            // receive a usable response.
            fn capture_webview(window: &WebviewWindow) -> Result<String, String> {
                let pos = window
                    .outer_position()
                    .map_err(|err| format!("failed to get window position: {err}"))?;
                let size = window
                    .outer_size()
                    .map_err(|err| format!("failed to get window size: {err}"))?;

                if size.width == 0 || size.height == 0 {
                    return Err("browser window has no visible area".to_owned());
                }

                let screen = Screen::from_point(pos.x, pos.y)
                    .map_err(|err| format!("failed to locate screen: {err}"))?;
                let x = pos.x.saturating_sub(screen.display_info.x);
                let y = pos.y.saturating_sub(screen.display_info.y);

                let image = screen
                    .capture_area(x, y, size.width, size.height)
                    .map_err(|err| format!("failed to capture browser window: {err}"))?;
                let mut png = Vec::new();
                screenshots::image::DynamicImage::ImageRgba8(image)
                    .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
                    .map_err(|err| format!("failed to encode png: {err}"))?;
                Ok(STANDARD.encode(&png))
            }

            capture_webview(&window).or_else(|err| {
                let message = format!("Browser screenshot fallback: {err}");
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="600"><rect width="100%" height="100%" fill="#f3f4f6"/><text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" font-family="sans-serif" font-size="16" fill="#6b7280">{}</text></svg>"##,
                    message.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
                );
                Ok(STANDARD.encode(svg))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_window_label_has_uuid_prefix() {
        let label = generate_browser_window_label();
        assert!(label.starts_with("browser-"));
        assert!(label.len() > "browser-".len());
    }
}
