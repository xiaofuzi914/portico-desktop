//! Persisted notification center backed by [`Storage`].

use app_models::{
    AgentRunId, AppError, Notification, NotificationCategory, NotificationId, ThreadId, WorkspaceId,
};
use chrono::Utc;
use std::sync::Arc;

use crate::storage::Storage;

/// A thin wrapper around storage for creating and managing notifications.
#[derive(Clone)]
pub struct NotificationCenter {
    storage: Arc<dyn Storage>,
}

impl NotificationCenter {
    /// Create a new notification center backed by the given storage.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Create and persist a notification.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification cannot be persisted.
    pub async fn create(
        &self,
        workspace_id: WorkspaceId,
        thread_id: Option<ThreadId>,
        run_id: Option<AgentRunId>,
        title: impl Into<String>,
        body: impl Into<String>,
        category: NotificationCategory,
    ) -> Result<Notification, AppError> {
        let notification = Notification {
            id: NotificationId::new(),
            workspace_id,
            thread_id,
            run_id,
            title: title.into(),
            body: body.into(),
            category,
            read: false,
            created_at: Utc::now(),
        };
        self.storage.create_notification(&notification).await?;
        Ok(notification)
    }

    /// List notifications in a workspace, or across all workspaces when `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if notifications cannot be read.
    pub async fn list(
        &self,
        workspace_id: Option<WorkspaceId>,
        unread_only: bool,
        limit: i64,
    ) -> Result<Vec<Notification>, AppError> {
        self.storage.list_notifications(workspace_id, unread_only, limit).await
    }

    /// Fetch a notification by id.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification is missing.
    pub async fn get(&self, id: NotificationId) -> Result<Notification, AppError> {
        self.storage.get_notification(id).await
    }

    /// Mark a notification as read or unread.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification is missing.
    pub async fn mark_read(&self, id: NotificationId, read: bool) -> Result<(), AppError> {
        self.storage.mark_notification_read(id, read).await
    }

    /// Dismiss (delete) a notification.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification is missing.
    pub async fn dismiss(&self, id: NotificationId) -> Result<(), AppError> {
        self.storage.dismiss_notification(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStorage;

    async fn setup() -> (NotificationCenter, WorkspaceId) {
        let storage = Arc::new(SqliteStorage::open_in_memory().await.expect("open db"));
        let workspace = storage
            .create_workspace("test", "/tmp/test", false)
            .await
            .expect("create workspace");
        (NotificationCenter::new(storage), workspace.id)
    }

    #[tokio::test]
    async fn create_and_list_notifications() {
        let (center, workspace_id) = setup().await;

        let notification = center
            .create(
                workspace_id,
                None,
                None,
                "Test",
                "Body",
                NotificationCategory::System,
            )
            .await
            .expect("create notification");

        let notifications = center.list(Some(workspace_id), false, 10).await.expect("list");
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].id, notification.id);
        assert!(!notifications[0].read);
    }

    #[tokio::test]
    async fn mark_read_and_dismiss() {
        let (center, workspace_id) = setup().await;

        let notification = center
            .create(
                workspace_id,
                None,
                None,
                "Test",
                "Body",
                NotificationCategory::InApp,
            )
            .await
            .expect("create notification");

        center.mark_read(notification.id, true).await.expect("mark read");

        let unread = center.list(Some(workspace_id), true, 10).await.expect("list unread");
        assert!(unread.is_empty());

        center.dismiss(notification.id).await.expect("dismiss");
        let all = center.list(Some(workspace_id), false, 10).await.expect("list all");
        assert!(all.is_empty());
    }
}
