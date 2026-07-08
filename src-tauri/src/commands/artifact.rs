//! Artifact preview commands.

use std::path::Path;

use app_models::{ArtifactPreview, WorkspaceId};
use app_security::{PermissionRequest, PermissionResult};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use tauri::State;

use crate::AppState;

/// Detect a MIME type from a file extension.
#[must_use]
pub fn detect_mime_type(path: &str) -> &'static str {
    Path::new(path).extension().and_then(std::ffi::OsStr::to_str).map_or(
        "application/octet-stream",
        |ext| match ext.to_lowercase().as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "txt" => "text/plain",
            "md" => "text/markdown",
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" | "mjs" => "text/javascript",
            "json" => "application/json",
            "xml" => "application/xml",
            "mp3" => "audio/mpeg",
            "mp4" => "video/mp4",
            "webm" => "video/webm",
            "wav" => "audio/wav",
            "ogg" => "audio/ogg",
            "zip" => "application/zip",
            "gz" => "application/gzip",
            "tar" => "application/x-tar",
            "rs" => "text/x-rust",
            "py" => "text/x-python",
            "ts" => "application/typescript",
            "tsx" => "text/x-tsx",
            "jsx" => "text/x-jsx",
            _ => "application/octet-stream",
        },
    )
}

/// Read and base64-encode an artifact file for preview.
///
/// # Errors
///
/// Returns an error string if permission is denied or the file cannot be read.
#[tauri::command]
pub async fn preview_artifact(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    path: String,
) -> Result<ArtifactPreview, String> {
    match state.runtime.evaluate_permission(PermissionRequest {
        workspace_id,
        thread_id: None,
        run_id: None,
        action: "filesystem.read".to_owned(),
        resource: path.clone(),
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

    let contents = tokio::fs::read(&path)
        .await
        .map_err(|err| format!("failed to read file: {err}"))?;
    let size_bytes = u64::try_from(contents.len()).unwrap_or(u64::MAX);
    let mime_type = detect_mime_type(&path).to_owned();
    let content_base64 = STANDARD.encode(&contents);

    Ok(ArtifactPreview {
        path,
        mime_type,
        content_base64,
        size_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mime_type_from_extension() {
        assert_eq!(detect_mime_type("image.png"), "image/png");
        assert_eq!(detect_mime_type("photo.JPG"), "image/jpeg");
        assert_eq!(detect_mime_type("document.pdf"), "application/pdf");
        assert_eq!(detect_mime_type("script.rs"), "text/x-rust");
    }

    #[test]
    fn detect_mime_type_defaults_to_octet_stream() {
        assert_eq!(detect_mime_type("unknown"), "application/octet-stream");
    }
}
