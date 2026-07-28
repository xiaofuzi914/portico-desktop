//! Thread archive: soft-delete, restore, active list filter, expired purge.

use app_runtime::{SqliteStorage, Storage};
use chrono::{Duration, Utc};

#[tokio::test]
async fn archive_hides_from_active_list_and_restore_returns() {
    let storage = SqliteStorage::open_in_memory().await.expect("open db");
    let root = std::env::temp_dir().join(format!("portico-archive-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let workspace = storage
        .create_workspace("ws", &root.to_string_lossy(), true)
        .await
        .expect("workspace");

    let a = storage
        .create_thread(workspace.id, "keep", None)
        .await
        .expect("thread a");
    let b = storage
        .create_thread(workspace.id, "archive-me", None)
        .await
        .expect("thread b");

    let active = storage.list_threads(workspace.id).await.expect("list");
    assert_eq!(active.len(), 2);

    let archived = storage
        .archive_thread(workspace.id, b.id)
        .await
        .expect("archive");
    assert!(archived.archived_at.is_some());

    let active = storage.list_threads(workspace.id).await.expect("list active");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, a.id);

    let archived_list = storage
        .list_archived_threads(workspace.id)
        .await
        .expect("list archived");
    assert_eq!(archived_list.len(), 1);
    assert_eq!(archived_list[0].id, b.id);

    let restored = storage
        .restore_thread(workspace.id, b.id)
        .await
        .expect("restore");
    assert!(restored.archived_at.is_none());
    assert_eq!(
        storage.list_threads(workspace.id).await.expect("list").len(),
        2
    );
}

#[tokio::test]
async fn purge_expired_removes_old_archives() {
    let storage = SqliteStorage::open_in_memory().await.expect("open db");
    let root = std::env::temp_dir().join(format!("portico-purge-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let workspace = storage
        .create_workspace("ws", &root.to_string_lossy(), true)
        .await
        .expect("workspace");

    let old = storage
        .create_thread(workspace.id, "old", None)
        .await
        .expect("old");
    let recent = storage
        .create_thread(workspace.id, "recent", None)
        .await
        .expect("recent");

    storage
        .archive_thread(workspace.id, old.id)
        .await
        .expect("archive old");
    storage
        .archive_thread(workspace.id, recent.id)
        .await
        .expect("archive recent");

    // Backdate the old archive beyond 30 days.
    let past = Utc::now() - Duration::days(40);
    sqlx::query("UPDATE threads SET archived_at = ? WHERE id = ?")
        .bind(past)
        .bind(old.id.0)
        .execute(storage.pool())
        .await
        .expect("backdate");

    let purged = storage
        .purge_expired_archived_threads(30)
        .await
        .expect("purge");
    assert_eq!(purged, 1);

    let archived = storage
        .list_archived_threads(workspace.id)
        .await
        .expect("list");
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].id, recent.id);

    assert!(storage.get_thread(old.id).await.is_err());
}
