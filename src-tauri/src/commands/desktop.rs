//! Desktop automation and capture commands.

use app_models::{DesktopCapture, WorkspaceId};
use app_security::{PermissionRequest, PermissionResult};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};
use screenshots::{Screen, image::ImageFormat};
use tauri::State;

use crate::AppState;

/// Build a permission request for a desktop-control action.
fn desktop_control_request(
    workspace_id: WorkspaceId,
    resource: impl Into<String>,
) -> PermissionRequest {
    PermissionRequest {
        workspace_id,
        thread_id: None,
        run_id: None,
        action: "desktop.control".to_owned(),
        resource: resource.into(),
        trusted_workspace: false,
    }
}

/// Ensure the desktop-control action is allowed for the workspace.
fn ensure_desktop_control(
    state: &State<'_, AppState>,
    workspace_id: WorkspaceId,
    resource: impl Into<String>,
) -> Result<(), String> {
    match state
        .runtime
        .evaluate_permission(desktop_control_request(workspace_id, resource))
    {
        PermissionResult::Allowed => Ok(()),
        PermissionResult::Ask { request } => Err(format!(
            "approval required for {} on {}",
            request.action, request.resource
        )),
        PermissionResult::Denied { reason } => Err(reason),
    }
}

/// Capture the primary screen as a base64-encoded PNG.
///
/// # Errors
///
/// Returns an error string if permission is denied or capture fails.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn capture_screen(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<DesktopCapture, String> {
    ensure_desktop_control(&state, workspace_id, "screen:capture")?;

    let screens = Screen::all().map_err(|err| format!("failed to enumerate screens: {err}"))?;
    let screen = screens.into_iter().next().ok_or("no screens available")?;

    let image = screen.capture().map_err(|err| format!("failed to capture screen: {err}"))?;
    let width = image.width();
    let height = image.height();

    let mut png = Vec::new();
    screenshots::image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|err| format!("failed to encode png: {err}"))?;
    let image_base64 = STANDARD.encode(&png);

    Ok(DesktopCapture {
        image_base64,
        width,
        height,
    })
}

/// Move the mouse cursor to the given screen coordinates.
///
/// # Errors
///
/// Returns an error string if permission is denied or the mouse cannot be moved.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn move_mouse(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    x: i32,
    y: i32,
) -> Result<(), String> {
    ensure_desktop_control(&state, workspace_id, format!("mouse:move:{x},{y}"))?;

    let mut enigo = Enigo::new(&Settings::default()).map_err(|err| err.to_string())?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|err| format!("failed to move mouse: {err}"))
}

/// Click the left mouse button.
///
/// # Errors
///
/// Returns an error string if permission is denied or the click fails.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn click_mouse(state: State<'_, AppState>, workspace_id: WorkspaceId) -> Result<(), String> {
    ensure_desktop_control(&state, workspace_id, "mouse:click")?;

    let mut enigo = Enigo::new(&Settings::default()).map_err(|err| err.to_string())?;
    enigo
        .button(Button::Left, Direction::Click)
        .map_err(|err| format!("failed to click mouse: {err}"))
}

/// Type the given text using the keyboard.
///
/// # Errors
///
/// Returns an error string if permission is denied or the text cannot be typed.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn type_text(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    text: String,
) -> Result<(), String> {
    ensure_desktop_control(&state, workspace_id, "keyboard:type")?;

    let mut enigo = Enigo::new(&Settings::default()).map_err(|err| err.to_string())?;
    enigo.text(&text).map_err(|err| format!("failed to type text: {err}"))
}

/// Best-effort focus of an application by name.
///
/// On macOS this runs `AppleScript` via `osascript`. On other platforms it
/// currently returns `Ok(())` after checking permissions.
///
/// # Errors
///
/// Returns an error string if either the `desktop.control` or `shell.execute`
/// permission is denied.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn focus_app(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    name: String,
) -> Result<(), String> {
    match state.runtime.evaluate_permission(PermissionRequest {
        workspace_id,
        thread_id: None,
        run_id: None,
        action: "desktop.control".to_owned(),
        resource: format!("focus:{name}"),
        trusted_workspace: false,
    }) {
        PermissionResult::Allowed => {}
        PermissionResult::Ask { request } => {
            return Err(format!(
                "approval required for {} on {}",
                request.action, request.resource
            ));
        }
        PermissionResult::Denied { reason } => return Err(reason),
    }

    let script = format!(
        r#"tell application "{}" to activate"#,
        name.replace('"', "\\\"")
    );
    match state.runtime.evaluate_permission(PermissionRequest {
        workspace_id,
        thread_id: None,
        run_id: None,
        action: "shell.execute".to_owned(),
        resource: script.clone(),
        trusted_workspace: false,
    }) {
        PermissionResult::Allowed => {}
        PermissionResult::Ask { request } => {
            return Err(format!(
                "approval required for {} on {}",
                request.action, request.resource
            ));
        }
        PermissionResult::Denied { reason } => return Err(reason),
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|err| format!("failed to run osascript: {err}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_models::WorkspaceId;
    use app_security::{PermissionEngine, PermissionResult};

    struct DenyEngine;

    impl PermissionEngine for DenyEngine {
        fn evaluate(&self, _request: PermissionRequest) -> PermissionResult {
            PermissionResult::Denied {
                reason: "denied by mock".to_owned(),
            }
        }
    }

    #[test]
    fn desktop_control_request_builds_expected_action() {
        let workspace_id = WorkspaceId::new();
        let request = desktop_control_request(workspace_id, "screen:capture");
        assert_eq!(request.action, "desktop.control");
        assert_eq!(request.resource, "screen:capture");
    }

    #[test]
    fn mock_engine_denies_desktop_control_request() {
        let engine = DenyEngine;
        let request = desktop_control_request(WorkspaceId::new(), "mouse:click");
        let result = engine.evaluate(request);
        assert!(matches!(
            result,
            PermissionResult::Denied {
                reason,
            } if reason == "denied by mock"
        ));
    }
}
