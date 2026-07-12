//! Workspace-related Tauri commands.

use app_models::{ArtifactPreview, Workspace, WorkspaceId};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::path::{Component, Path, PathBuf};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

#[derive(Debug, Serialize)]
pub struct WorkspaceFileEntry {
    name: String,
    relative_path: String,
    is_directory: bool,
}

const MAX_MARKDOWN_PREVIEW_BYTES: u64 = 5 * 1024 * 1024;

fn validate_relative_path(relative_path: &str) -> Result<&Path, app_models::AppError> {
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(app_models::AppError::PermissionDenied {
            reason: "workspace file path must be relative and cannot traverse parents".to_owned(),
        });
    }
    Ok(relative)
}

/// Open a directory in the OS file manager (Finder / Explorer / xdg-open).
fn open_directory_in_os(path: &Path) -> Result<(), app_models::AppError> {
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
    .map_err(|error| app_models::AppError::Internal {
        message: format!("open folder in OS failed: {error}"),
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(app_models::AppError::Internal {
            message: format!("open folder in OS exited with {status}"),
        })
    }
}

/// Create a new workspace.
///
/// # Errors
///
/// Returns an error response if the workspace cannot be created.
#[tauri::command]
pub async fn create_workspace(
    state: State<'_, AppState>,
    name: String,
    root_path: String,
) -> Result<ApiResponse<Workspace>, String> {
    Ok(
        match state.runtime.workspace_manager().add_workspace(&name, &root_path).await {
            Ok(workspace) => ApiResponse::ok(workspace),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List all workspaces.
///
/// # Errors
///
/// Returns an error response if workspaces cannot be listed.
#[tauri::command]
pub async fn list_workspaces(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<Workspace>>, String> {
    Ok(match state.runtime.list_workspaces().await {
        Ok(workspaces) => ApiResponse::ok(workspaces),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Fetch a workspace by id.
///
/// # Errors
///
/// Returns an error response if the workspace is not found or cannot be read.
#[tauri::command]
pub async fn get_workspace(
    state: State<'_, AppState>,
    id: WorkspaceId,
) -> Result<ApiResponse<Workspace>, String> {
    Ok(match state.runtime.get_workspace(id).await {
        Ok(workspace) => ApiResponse::ok(workspace),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Update whether a workspace is trusted for sensitive operations.
///
/// # Errors
///
/// Returns an error response if the workspace cannot be updated.
#[tauri::command]
pub async fn trust_workspace(
    state: State<'_, AppState>,
    id: WorkspaceId,
    trusted: bool,
) -> Result<ApiResponse<Workspace>, String> {
    Ok(
        match state.runtime.workspace_manager().trust_workspace(id, trusted).await {
            Ok(workspace) => ApiResponse::ok(workspace),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Replace the allowed read/write paths for a workspace.
///
/// # Errors
///
/// Returns an error response if the paths cannot be persisted.
#[tauri::command]
pub async fn set_workspace_paths(
    state: State<'_, AppState>,
    id: WorkspaceId,
    read_paths: Vec<String>,
    write_paths: Vec<String>,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state
            .runtime
            .workspace_manager()
            .set_allowed_paths(id, read_paths, write_paths)
            .await
        {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// List one directory inside a trusted workspace.
///
/// # Errors
///
/// Rejects untrusted workspaces, path traversal, symlink escapes, and unreadable directories.
#[tauri::command]
pub async fn list_workspace_files(
    state: State<'_, AppState>,
    id: WorkspaceId,
    relative_path: String,
) -> Result<ApiResponse<Vec<WorkspaceFileEntry>>, String> {
    let result = async {
        let mut workspace = state.runtime.get_workspace(id).await?;
        if !workspace.trusted {
            return Err(app_models::AppError::PermissionDenied {
                reason: "trust the project before browsing its files".to_owned(),
            });
        }
        if workspace.allowed_read_paths.is_empty() {
            workspace = state.runtime.workspace_manager().trust_workspace(id, true).await?;
        }

        let relative = validate_relative_path(&relative_path)?;

        let root = PathBuf::from(&workspace.root_path);
        let target = root.join(relative).canonicalize().map_err(|error| {
            app_models::AppError::PermissionDenied {
                reason: format!("workspace directory is unavailable: {error}"),
            }
        })?;
        if !target.starts_with(&root)
            || !workspace
                .allowed_read_paths
                .iter()
                .map(PathBuf::from)
                .any(|allowed| target.starts_with(allowed))
        {
            return Err(app_models::AppError::PermissionDenied {
                reason: "workspace directory is outside the allowed read paths".to_owned(),
            });
        }

        let mut directory =
            tokio::fs::read_dir(&target)
                .await
                .map_err(|error| app_models::AppError::Internal {
                    message: format!("list workspace directory failed: {error}"),
                })?;
        let mut entries = Vec::new();
        loop {
            let Some(entry) =
                directory.next_entry().await.map_err(|error| app_models::AppError::Internal {
                    message: format!("read workspace directory entry failed: {error}"),
                })?
            else {
                break;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let entry_relative = relative.join(&name).to_string_lossy().into_owned();
            let is_directory = entry.file_type().await.is_ok_and(|kind| kind.is_dir());
            entries.push(WorkspaceFileEntry {
                name,
                relative_path: entry_relative,
                is_directory,
            });
        }
        entries.sort_by(|left, right| {
            right
                .is_directory
                .cmp(&left.is_directory)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(entries)
    }
    .await;

    Ok(match result {
        Ok(entries) => ApiResponse::ok(entries),
        Err(error) => ApiResponse::err(error.to_string()),
    })
}

/// Open the current workspace folder (or a relative subfolder) in the OS file manager.
///
/// # Errors
///
/// Rejects untrusted workspaces, path traversal, and directories outside allowlists.
#[tauri::command]
pub async fn open_workspace_folder(
    state: State<'_, AppState>,
    id: WorkspaceId,
    relative_path: String,
) -> Result<ApiResponse<()>, String> {
    let result = async {
        let mut workspace = state.runtime.get_workspace(id).await?;
        if !workspace.trusted {
            return Err(app_models::AppError::PermissionDenied {
                reason: "trust the project before opening its folder in the OS".to_owned(),
            });
        }
        if workspace.allowed_read_paths.is_empty() {
            workspace = state.runtime.workspace_manager().trust_workspace(id, true).await?;
        }

        let relative = validate_relative_path(&relative_path)?;
        let root = PathBuf::from(&workspace.root_path)
            .canonicalize()
            .map_err(|error| app_models::AppError::PermissionDenied {
                reason: format!("workspace root is unavailable: {error}"),
            })?;
        let target = if relative_path.is_empty() {
            root.clone()
        } else {
            root.join(relative).canonicalize().map_err(|error| {
                app_models::AppError::PermissionDenied {
                    reason: format!("workspace directory is unavailable: {error}"),
                }
            })?
        };
        if !target.is_dir() {
            return Err(app_models::AppError::PermissionDenied {
                reason: "path is not a directory".to_owned(),
            });
        }
        if !target.starts_with(&root)
            || !workspace
                .allowed_read_paths
                .iter()
                .map(PathBuf::from)
                .any(|allowed| {
                    let allowed = allowed.canonicalize().unwrap_or(allowed);
                    target.starts_with(allowed)
                })
        {
            return Err(app_models::AppError::PermissionDenied {
                reason: "workspace directory is outside the allowed read paths".to_owned(),
            });
        }

        open_directory_in_os(&target)?;
        Ok(())
    }
    .await;

    Ok(match result {
        Ok(()) => ApiResponse::ok(()),
        Err(error) => ApiResponse::err(error.to_string()),
    })
}

/// Read a Markdown file inside a trusted workspace for preview.
///
/// # Errors
///
/// Returns an error response when the workspace or requested file cannot be read safely.
#[tauri::command]
pub async fn preview_workspace_markdown(
    state: State<'_, AppState>,
    id: WorkspaceId,
    relative_path: String,
) -> Result<ApiResponse<ArtifactPreview>, String> {
    let result = async {
        let mut workspace = state.runtime.get_workspace(id).await?;
        if !workspace.trusted {
            return Err(app_models::AppError::PermissionDenied {
                reason: "trust the project before previewing its files".to_owned(),
            });
        }
        if workspace.allowed_read_paths.is_empty() {
            workspace = state.runtime.workspace_manager().trust_workspace(id, true).await?;
        }

        let relative = validate_relative_path(&relative_path)?;
        let extension = relative.extension().and_then(std::ffi::OsStr::to_str);
        if !extension.is_some_and(|value| value.eq_ignore_ascii_case("md")) {
            return Err(app_models::AppError::PermissionDenied {
                reason: "only Markdown files can be previewed".to_owned(),
            });
        }

        let root = PathBuf::from(&workspace.root_path);
        let target = root.join(relative).canonicalize().map_err(|error| {
            app_models::AppError::PermissionDenied {
                reason: format!("workspace file is unavailable: {error}"),
            }
        })?;
        if !target.starts_with(&root)
            || !workspace
                .allowed_read_paths
                .iter()
                .map(PathBuf::from)
                .any(|allowed| target.starts_with(allowed))
        {
            return Err(app_models::AppError::PermissionDenied {
                reason: "workspace file is outside the allowed read paths".to_owned(),
            });
        }

        let metadata =
            tokio::fs::metadata(&target)
                .await
                .map_err(|error| app_models::AppError::Internal {
                    message: format!("inspect workspace file failed: {error}"),
                })?;
        if metadata.len() > MAX_MARKDOWN_PREVIEW_BYTES {
            return Err(app_models::AppError::PermissionDenied {
                reason: "Markdown preview is limited to 5 MB".to_owned(),
            });
        }
        let contents =
            tokio::fs::read(&target).await.map_err(|error| app_models::AppError::Internal {
                message: format!("read workspace file failed: {error}"),
            })?;
        if u64::try_from(contents.len()).unwrap_or(u64::MAX) > MAX_MARKDOWN_PREVIEW_BYTES {
            return Err(app_models::AppError::PermissionDenied {
                reason: "Markdown preview is limited to 5 MB".to_owned(),
            });
        }
        let size_bytes = u64::try_from(contents.len()).unwrap_or(u64::MAX);
        Ok(ArtifactPreview {
            path: target.to_string_lossy().into_owned(),
            mime_type: "text/markdown".to_owned(),
            content_base64: STANDARD.encode(contents),
            size_bytes,
        })
    }
    .await;

    Ok(match result {
        Ok(preview) => ApiResponse::ok(preview),
        Err(error) => ApiResponse::err(error.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::validate_relative_path;

    #[test]
    fn accepts_safe_relative_paths() {
        assert!(validate_relative_path("docs/readme.md").is_ok());
    }

    #[test]
    fn rejects_parent_traversal_and_absolute_paths() {
        assert!(validate_relative_path("../secret.md").is_err());
        assert!(validate_relative_path("/tmp/secret.md").is_err());
    }
}
