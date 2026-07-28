//! Delete workspace removes Portico data but keeps the API path simple.

use app_runtime::{SqliteStorage, Storage};

#[tokio::test]
async fn delete_workspace_removes_from_list() {
    let storage = SqliteStorage::open_in_memory().await.expect("open db");
    let root = std::env::temp_dir().join(format!("portico-ws-del-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let workspace = storage
        .create_workspace("to-delete", &root.to_string_lossy(), true)
        .await
        .expect("workspace");
    let _thread = storage
        .create_thread(workspace.id, "session", None)
        .await
        .expect("thread");

    assert_eq!(storage.list_workspaces().await.expect("list").len(), 1);
    storage.delete_workspace(workspace.id).await.expect("delete");
    assert!(storage.list_workspaces().await.expect("list").is_empty());
    assert!(storage.get_workspace(workspace.id).await.is_err());
}
