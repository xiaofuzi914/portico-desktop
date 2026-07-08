//! In-memory terminal session manager with command policy and audit hooks.

use app_models::{AppError, TerminalId, TerminalOutput, ThreadId, WorkspaceId};
use app_security::{AuditEvent, PermissionResult, SecurityContext};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A single terminal session.
pub struct TerminalSession {
    #[allow(dead_code)]
    id: TerminalId,
    thread_id: ThreadId,
    history: Vec<String>,
}

impl TerminalSession {
    #[allow(clippy::missing_const_for_fn)]
    fn new(id: TerminalId, thread_id: ThreadId) -> Self {
        Self {
            id,
            thread_id,
            history: Vec::new(),
        }
    }
}

/// Manages terminal sessions and executes commands through the system shell.
#[derive(Clone)]
pub struct TerminalManager {
    sessions: Arc<Mutex<HashMap<TerminalId, TerminalSession>>>,
    security: Arc<SecurityContext>,
}

impl TerminalManager {
    /// Create a new terminal manager.
    #[must_use]
    pub fn new(security: Arc<SecurityContext>) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            security,
        }
    }

    /// Create a new terminal session for a thread.
    pub async fn create_session(&self, thread_id: ThreadId) -> TerminalId {
        let id = TerminalId::new();
        let session = TerminalSession::new(id, thread_id);
        let mut sessions = self.sessions.lock().await;
        sessions.insert(id, session);
        id
    }

    /// Execute a command in a terminal session.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is missing, the command is denied by
    /// policy, or the command cannot be executed.
    pub async fn execute(
        &self,
        id: TerminalId,
        command: &str,
        cwd: &str,
    ) -> Result<TerminalOutput, AppError> {
        let (thread_id, allowed) = {
            let sessions = self.sessions.lock().await;
            let session = sessions.get(&id).ok_or_else(|| AppError::NotFound {
                resource: format!("terminal session {id:?}"),
            })?;

            let command_result = self.security.command_policy().allow_command_line(command);
            let allowed = matches!(command_result, PermissionResult::Allowed);
            let thread_id = session.thread_id;
            drop(sessions);
            (thread_id, allowed)
        };

        if !allowed {
            let outcome = PermissionResult::Denied {
                reason: "command denied by policy".to_owned(),
            };
            let _ = self.security.audit_logger().log(AuditEvent {
                workspace_id: WorkspaceId::default(),
                thread_id: Some(thread_id),
                run_id: None,
                action: "terminal.execute".to_owned(),
                resource: command.to_owned(),
                outcome,
            });
            return Err(AppError::PermissionDenied {
                reason: "command denied by policy".to_owned(),
            });
        }

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .output()
            .await
            .map_err(|e| AppError::Internal {
                message: format!("terminal command failed: {e}"),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code();

        {
            let mut sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get_mut(&id) {
                session.history.push(format!("> {command}"));
                for line in stdout.lines() {
                    session.history.push(line.to_owned());
                }
                for line in stderr.lines() {
                    session.history.push(format!("stderr: {line}"));
                }
            }
        }

        let outcome = if output.status.success() {
            PermissionResult::Allowed
        } else {
            PermissionResult::Denied {
                reason: format!("exit code {exit_code:?}"),
            }
        };
        let _ = self.security.audit_logger().log(AuditEvent {
            workspace_id: WorkspaceId::default(),
            thread_id: Some(thread_id),
            run_id: None,
            action: "terminal.execute".to_owned(),
            resource: command.to_owned(),
            outcome,
        });

        Ok(TerminalOutput {
            stdout,
            stderr,
            exit_code,
        })
    }

    /// Read command history for a terminal session.
    pub async fn read_history(&self, id: TerminalId) -> Vec<String> {
        let sessions = self.sessions.lock().await;
        sessions.get(&id).map(|session| session.history.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_security::{
        DefaultCommandPolicy, DefaultNetworkPolicy, MemoryAuditLogger, PermissionEngine,
        PermissionRequest, PermissionResult,
    };

    struct AllowAllEngine;

    impl PermissionEngine for AllowAllEngine {
        fn evaluate(&self, _request: PermissionRequest) -> PermissionResult {
            PermissionResult::Allowed
        }
    }

    fn test_security() -> Arc<SecurityContext> {
        Arc::new(SecurityContext::new(
            Arc::new(AllowAllEngine),
            Arc::new(DefaultCommandPolicy::new()),
            Arc::new(DefaultNetworkPolicy::new()),
            Arc::new(MemoryAuditLogger::new()),
        ))
    }

    #[tokio::test]
    async fn execute_command_and_read_history() {
        let manager = TerminalManager::new(test_security());
        let thread_id = ThreadId::new();
        let terminal_id = manager.create_session(thread_id).await;

        let output = manager.execute(terminal_id, "echo hello", "/tmp").await.expect("execute");
        assert!(output.stdout.contains("hello"));

        let history = manager.read_history(terminal_id).await;
        assert!(history.iter().any(|line| line.contains("hello")));
    }

    #[tokio::test]
    async fn denies_dangerous_command() {
        let manager = TerminalManager::new(test_security());
        let terminal_id = manager.create_session(ThreadId::new()).await;

        let result = manager.execute(terminal_id, "rm -rf /", "/tmp").await;
        assert!(matches!(result, Err(AppError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn execute_is_audited() {
        let audit = Arc::new(MemoryAuditLogger::new());
        let security = Arc::new(SecurityContext::new(
            Arc::new(AllowAllEngine),
            Arc::new(DefaultCommandPolicy::new()),
            Arc::new(DefaultNetworkPolicy::new()),
            audit.clone(),
        ));
        let manager = TerminalManager::new(security);
        let terminal_id = manager.create_session(ThreadId::new()).await;

        manager.execute(terminal_id, "echo x", "/tmp").await.unwrap();

        let events = audit.events().expect("read events");
        assert!(events.iter().any(|e| e.action == "terminal.execute"));
    }
}
