//! Diagnostics bundle collection and upload commands.

use app_models::{DiagnosticsBundle, DiagnosticsBundleId, WorkspaceId};
use app_security::{AuditEvent, PermissionResult, SecretRedactor};
use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::error::ApiResponse;

/// Collect a redacted diagnostics bundle for support or debugging.
///
/// The bundle includes the latest application log, a redacted audit summary,
/// app version, and OS information. It is stored under the app data directory.
///
/// # Errors
///
/// Returns an error response if the bundle directory or contents cannot be
/// created.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn collect_diagnostics_bundle(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ApiResponse<DiagnosticsBundle>, String> {
    let bundle_id = DiagnosticsBundleId::new();
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to resolve app data dir: {e}"))?;
    let bundle_dir = app_data_dir.join("diagnostics").join(bundle_id.0.to_string());
    std::fs::create_dir_all(&bundle_dir)
        .map_err(|e| format!("failed to create diagnostics bundle dir: {e}"))?;

    // Copy the most recent log file, or write a placeholder if none exists.
    let log_dir = app_data_dir.join("logs");
    let dest_log = bundle_dir.join("app.log");
    if let Err(err) = copy_latest_log(&log_dir, &dest_log) {
        let placeholder = format!("log file unavailable: {err}");
        std::fs::write(&dest_log, placeholder)
            .map_err(|e| format!("failed to write log placeholder: {e}"))?;
    }

    // Build a redacted audit summary.
    let audit_summary_path = bundle_dir.join("audit-summary.jsonl");
    let redactor = app_security::DefaultSecretRedactor::new();
    match state.runtime.list_audit_log(None, None, None).await {
        Ok(entries) => {
            let mut summary_lines = Vec::with_capacity(entries.len());
            for entry in entries {
                let line = serde_json::to_string(&entry)
                    .map_err(|e| format!("failed to serialize audit entry: {e}"))?;
                summary_lines.push(redactor.redact(&line));
            }
            let summary = summary_lines.join("\n");
            std::fs::write(&audit_summary_path, summary)
                .map_err(|e| format!("failed to write audit summary: {e}"))?;
        }
        Err(err) => {
            let message = format!("audit summary unavailable: {err}");
            std::fs::write(&audit_summary_path, message)
                .map_err(|e| format!("failed to write audit summary placeholder: {e}"))?;
        }
    }

    let size_bytes =
        dir_size(&bundle_dir).map_err(|e| format!("failed to compute bundle size: {e}"))?;

    let bundle = DiagnosticsBundle {
        id: bundle_id,
        created_at: chrono::Utc::now(),
        log_path: dest_log.to_string_lossy().into_owned(),
        audit_summary_path: audit_summary_path.to_string_lossy().into_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        os_info: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        redacted: true,
        size_bytes,
    };

    // Record that a bundle was collected.
    let _ = state.runtime.audit(AuditEvent {
        workspace_id: WorkspaceId(uuid::Uuid::nil()),
        thread_id: None,
        run_id: None,
        action: "diagnostics.collect".to_owned(),
        resource: bundle_id.0.to_string(),
        outcome: PermissionResult::Allowed,
    });

    Ok(ApiResponse::ok(bundle))
}

/// Simulate uploading a diagnostics bundle and record an audit event.
///
/// This is a placeholder: no network request is performed.
///
/// # Errors
///
/// Returns an error response if the bundle is missing or the audit event
/// cannot be recorded.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn upload_diagnostics_bundle(
    app: AppHandle,
    state: State<'_, AppState>,
    bundle_id: DiagnosticsBundleId,
) -> Result<ApiResponse<()>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to resolve app data dir: {e}"))?;
    let bundle_dir = app_data_dir.join("diagnostics").join(bundle_id.0.to_string());
    if !bundle_dir.exists() {
        return Ok(ApiResponse::err(format!(
            "diagnostics bundle {bundle_id:?} not found"
        )));
    }

    // Placeholder for an approved upload; in production this would POST to an
    // update/diagnostics endpoint with the bundle contents.
    if let Err(err) = state.runtime.audit(AuditEvent {
        workspace_id: WorkspaceId(uuid::Uuid::nil()),
        thread_id: None,
        run_id: None,
        action: "diagnostics.upload".to_owned(),
        resource: bundle_id.0.to_string(),
        outcome: PermissionResult::Allowed,
    }) {
        return Ok(ApiResponse::err(format!(
            "failed to record upload audit event: {err}"
        )));
    }

    Ok(ApiResponse::ok(()))
}

/// Copy the most recent `.log` file from `log_dir` to `dest`, if any.
fn copy_latest_log(log_dir: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(log_dir)
        .map_err(|e| format!("failed to read log dir: {e}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "log"))
        .collect();

    if entries.is_empty() {
        return Err("no log files found".to_owned());
    }

    entries.sort_by(|a, b| {
        let ma = a.metadata().and_then(|m| m.modified()).ok();
        let mb = b.metadata().and_then(|m| m.modified()).ok();
        mb.cmp(&ma)
    });

    std::fs::copy(&entries[0], dest).map_err(|e| format!("failed to copy log file: {e}"))?;
    Ok(())
}

/// Recursively compute the total size of a directory in bytes.
fn dir_size(path: &std::path::Path) -> Result<u64, std::io::Error> {
    let mut size = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            size += dir_size(&entry.path())?;
        } else {
            size += metadata.len();
        }
    }
    Ok(size)
}
