//! Migration tracking commands.

use app_models::MigrationInfo;
use base64::Engine;
use chrono::Utc;
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// List applied database migrations from the sqlx migration table.
///
/// Returns an empty list if the `_sqlx_migrations` table does not exist or has
/// no rows.
///
/// # Errors
///
/// Returns an error response if the migration table cannot be queried.
#[tauri::command]
pub async fn list_migrations(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<MigrationInfo>>, String> {
    let pool = state.runtime.storage().pool();

    let table_exists: bool = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("failed to check migration table: {e}"))?
    .unwrap_or(0)
        == 1;

    if !table_exists {
        return Ok(ApiResponse::ok(Vec::new()));
    }

    let rows = sqlx::query_as::<_, MigrationRow>(
        "SELECT version, description, installed_on, checksum FROM _sqlx_migrations WHERE success = 1 ORDER BY version ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("failed to list migrations: {e}"))?;

    let migrations = rows
        .into_iter()
        .map(|row| MigrationInfo {
            version: row.version,
            name: row.description,
            applied_at: row.installed_on.unwrap_or_else(Utc::now),
            checksum: base64::engine::general_purpose::STANDARD.encode(row.checksum),
        })
        .collect();

    Ok(ApiResponse::ok(migrations))
}

/// Rollback placeholder.
///
/// Automatic rollback is not implemented; downgrade scripts are maintained in
/// `crates/app-runtime/migrations/`.
#[tauri::command]
#[must_use]
pub fn rollback_last_migration() -> ApiResponse<()> {
    ApiResponse::err(
        "rollback not implemented; manual downgrade scripts are in crates/app-runtime/migrations/",
    )
}

#[derive(sqlx::FromRow)]
struct MigrationRow {
    version: i64,
    description: String,
    installed_on: Option<chrono::DateTime<chrono::Utc>>,
    checksum: Vec<u8>,
}
