//! Diagnostics bundle collection and upload commands.

use app_models::{DiagnosticsBundle, DiagnosticsBundleId, WorkspaceId};
use app_security::{AuditEvent, PermissionResult};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Collect a redacted diagnostics bundle for support or debugging.
///
/// The bundle includes only allowlisted metadata and aggregate audit counts.
/// Raw logs and audit payloads are deliberately omitted because regex-based
/// redaction cannot safely recognize arbitrary provider or tool secrets.
///
/// # Errors
///
/// Returns an error response if the bundle directory or contents cannot be
/// created.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub async fn collect_diagnostics_bundle(
    state: State<'_, AppState>,
) -> Result<ApiResponse<DiagnosticsBundle>, String> {
    let bundle_id = DiagnosticsBundleId::new();
    let app_data_dir = &state.app_data_dir;
    let bundle_dir = app_data_dir.join("diagnostics").join(bundle_id.0.to_string());
    std::fs::create_dir_all(&bundle_dir)
        .map_err(|e| format!("failed to create diagnostics bundle dir: {e}"))?;

    // Never copy raw application logs into an export. Unknown credential formats
    // cannot be made safe by a best-effort regex redactor.
    let dest_log = bundle_dir.join("app.log");
    std::fs::write(&dest_log, safe_log_export(None))
        .map_err(|e| format!("failed to write safe log notice: {e}"))?;

    // Export aggregate counts only. Audit resources may contain paths, prompts,
    // command arguments, or other user-controlled sensitive values.
    let audit_summary_path = bundle_dir.join("audit-summary.jsonl");
    let audit_entries = state.runtime.list_audit_log(None, None, None).await;
    let (audit_count, collection_succeeded) =
        audit_entries.as_ref().map_or((0, false), |entries| (entries.len(), true));
    let summary = serde_json::to_vec_pretty(&safe_audit_summary(audit_count, collection_succeeded))
        .map_err(|e| format!("failed to serialize safe audit summary: {e}"))?;
    std::fs::write(&audit_summary_path, summary)
        .map_err(|e| format!("failed to write safe audit summary: {e}"))?;

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

const fn safe_log_export(_raw_log: Option<&str>) -> &'static str {
    "Raw application logs are omitted from diagnostics exports.\n"
}

fn safe_audit_summary(total_events: usize, collection_succeeded: bool) -> serde_json::Value {
    serde_json::json!({
        "format_version": 1,
        "collection_succeeded": collection_succeeded,
        "total_events": total_events,
        "payloads_included": false,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_log_omits_raw_content_even_when_it_contains_an_unknown_secret() {
        let canary = "portico-canary-7b4dc16d-unknown-format";
        let exported = safe_log_export(Some(&format!("provider failed with {canary}")));

        assert!(!exported.contains(canary));
        assert!(exported.contains("omitted"));
    }

    #[test]
    fn audit_summary_contains_counts_but_no_event_payload() {
        let summary = safe_audit_summary(7, true);

        assert_eq!(summary["format_version"], 1);
        assert_eq!(summary["total_events"], 7);
        assert_eq!(summary["payloads_included"], false);
    }
}
