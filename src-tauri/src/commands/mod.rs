//! Tauri command handlers exposed to the Portico frontend.

pub mod artifact;
pub mod automation;
pub mod background_task;
pub mod browser;
pub mod desktop;
pub mod diagnostics;
pub mod git;
pub mod memory;
pub mod migration;
pub mod model;
pub mod notification;
pub mod orchestrator;
pub mod plugins;
pub mod run;
pub mod security;
pub mod terminal;
pub mod thread;
pub mod workspace;
pub mod worktree;

use crate::error::ApiResponse;

/// Greet a user by name.
#[tauri::command]
#[must_use]
pub fn greet(name: &str) -> ApiResponse<String> {
    ApiResponse::ok(format!("Hello, {name}!"))
}
