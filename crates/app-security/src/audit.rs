//! In-memory [`AuditLogger`] implementation for tests.

use crate::{AuditEvent, AuditLogger};
use std::sync::Mutex;

/// Stores audit events in memory.
#[derive(Debug, Default)]
pub struct MemoryAuditLogger {
    events: Mutex<Vec<AuditEvent>>,
}

impl MemoryAuditLogger {
    /// Create a new empty memory audit logger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a copy of all recorded events.
    ///
    /// # Errors
    ///
    /// Returns an internal error if the event lock is poisoned.
    pub fn events(&self) -> Result<Vec<AuditEvent>, app_models::AppError> {
        let events = self.events.lock().map_err(|_| app_models::AppError::Internal {
            message: "audit log lock poisoned".to_owned(),
        })?;
        Ok(events.clone())
    }
}

impl AuditLogger for MemoryAuditLogger {
    fn log(&self, event: AuditEvent) -> Result<(), app_models::AppError> {
        self.events
            .lock()
            .map_err(|_| app_models::AppError::Internal {
                message: "audit log lock poisoned".to_owned(),
            })?
            .push(event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PermissionResult;
    use app_models::WorkspaceId;

    #[test]
    fn memory_logger_records_events() {
        let logger = MemoryAuditLogger::new();
        logger
            .log(AuditEvent {
                workspace_id: WorkspaceId::new(),
                thread_id: None,
                run_id: None,
                action: "test.action".to_owned(),
                resource: "resource".to_owned(),
                outcome: PermissionResult::Allowed,
            })
            .expect("log event");

        let events = logger.events().expect("read events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "test.action");
    }
}
