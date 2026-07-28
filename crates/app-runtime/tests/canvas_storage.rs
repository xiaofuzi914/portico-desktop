//! Canvas persistence: project uniqueness + snapshot graph round-trip.

use app_models::{
    CanvasEdge, CanvasEdgeId, CanvasEdgeKind, CanvasLink, CanvasLinkId, CanvasLinkRefType,
    CanvasNode, CanvasNodeId, CanvasNodeKind, CanvasNodeSource, CanvasNodeStatus,
};
use app_runtime::{SqliteStorage, Storage};
use chrono::Utc;

#[tokio::test]
async fn project_canvas_is_unique_per_workspace() {
    let storage = SqliteStorage::open_in_memory().await.expect("open db");
    let root = std::env::temp_dir().join(format!("portico-canvas-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let workspace = storage
        .create_workspace("canvas-ws", &root.to_string_lossy(), true)
        .await
        .expect("workspace");

    let a = storage
        .get_or_create_project_canvas(workspace.id)
        .await
        .expect("create");
    let b = storage
        .get_or_create_project_canvas(workspace.id)
        .await
        .expect("again");
    assert_eq!(a.id, b.id);
    assert_eq!(a.kind.as_str(), "project");
    assert!(a.thread_id.is_none());
}

#[tokio::test]
async fn thread_canvas_unique_and_snapshot_round_trip() {
    let storage = SqliteStorage::open_in_memory().await.expect("open db");
    let root = std::env::temp_dir().join(format!("portico-canvas-t-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let workspace = storage
        .create_workspace("canvas-ws", &root.to_string_lossy(), true)
        .await
        .expect("workspace");
    let thread = storage
        .create_thread(workspace.id, "scan project", None)
        .await
        .expect("thread");

    let c1 = storage
        .get_or_create_thread_canvas(workspace.id, thread.id)
        .await
        .expect("thread canvas");
    let c2 = storage
        .get_or_create_thread_canvas(workspace.id, thread.id)
        .await
        .expect("again");
    assert_eq!(c1.id, c2.id);
    assert_eq!(c1.thread_id, Some(thread.id));

    let now = Utc::now();
    let parent = CanvasNode {
        id: CanvasNodeId::new(),
        canvas_id: c1.id,
        kind: CanvasNodeKind::ThreadCluster,
        title: "scan project".into(),
        summary: "cluster".into(),
        status: CanvasNodeStatus::Todo,
        parent_id: None,
        position_x: 10.0,
        position_y: 20.0,
        layout_rank: 0,
        source: CanvasNodeSource::Auto,
        payload_json: r#"{"thread_id":"x"}"#.into(),
        created_at: now,
        updated_at: now,
    };
    let child = CanvasNode {
        id: CanvasNodeId::new(),
        canvas_id: c1.id,
        kind: CanvasNodeKind::Insight,
        title: "entry points".into(),
        summary: "src-tauri / runner".into(),
        status: CanvasNodeStatus::Todo,
        parent_id: Some(parent.id),
        position_x: 40.0,
        position_y: 80.0,
        layout_rank: 1,
        source: CanvasNodeSource::Auto,
        payload_json: "{}".into(),
        created_at: now,
        updated_at: now,
    };
    storage.upsert_canvas_node(&parent).await.expect("parent");
    storage.upsert_canvas_node(&child).await.expect("child");

    let edge = CanvasEdge {
        id: CanvasEdgeId::new(),
        canvas_id: c1.id,
        from_id: parent.id,
        to_id: child.id,
        kind: CanvasEdgeKind::Parent,
        label: None,
        created_at: now,
    };
    storage.upsert_canvas_edge(&edge).await.expect("edge");

    let link = CanvasLink {
        id: CanvasLinkId::new(),
        node_id: child.id,
        ref_type: CanvasLinkRefType::Thread,
        ref_id: thread.id.0.to_string(),
        snippet: Some("scan project".into()),
        created_at: now,
    };
    storage.upsert_canvas_link(&link).await.expect("link");

    let snap = storage.get_canvas_snapshot(c1.id).await.expect("snapshot");
    assert_eq!(snap.nodes.len(), 2);
    assert_eq!(snap.edges.len(), 1);
    assert_eq!(snap.links.len(), 1);
    assert!(snap.canvas.revision >= 3);
    assert_eq!(snap.links[0].ref_type.as_str(), "thread");
}

#[tokio::test]
async fn clear_auto_insight_nodes_keeps_user_nodes_and_marks_extracted() {
    let storage = SqliteStorage::open_in_memory().await.expect("open db");
    let root = std::env::temp_dir().join(format!("portico-canvas-c-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("root");
    let workspace = storage
        .create_workspace("canvas-ws", &root.to_string_lossy(), true)
        .await
        .expect("workspace");
    let canvas = storage
        .get_or_create_project_canvas(workspace.id)
        .await
        .expect("canvas");
    assert!(canvas.last_extracted_at.is_none());

    let now = Utc::now();
    let auto_cluster = CanvasNode {
        id: CanvasNodeId::new(),
        canvas_id: canvas.id,
        kind: CanvasNodeKind::ThreadCluster,
        title: "auto cluster".into(),
        summary: String::new(),
        status: CanvasNodeStatus::Todo,
        parent_id: None,
        position_x: 0.0,
        position_y: 0.0,
        layout_rank: 0,
        source: CanvasNodeSource::Auto,
        payload_json: "{}".into(),
        created_at: now,
        updated_at: now,
    };
    let auto_insight = CanvasNode {
        id: CanvasNodeId::new(),
        canvas_id: canvas.id,
        kind: CanvasNodeKind::Insight,
        title: "auto insight".into(),
        summary: String::new(),
        status: CanvasNodeStatus::Todo,
        parent_id: Some(auto_cluster.id),
        position_x: 0.0,
        position_y: 0.0,
        layout_rank: 1,
        source: CanvasNodeSource::Auto,
        payload_json: "{}".into(),
        created_at: now,
        updated_at: now,
    };
    let user_note = CanvasNode {
        id: CanvasNodeId::new(),
        canvas_id: canvas.id,
        kind: CanvasNodeKind::Note,
        title: "keep me".into(),
        summary: String::new(),
        status: CanvasNodeStatus::Todo,
        parent_id: None,
        position_x: 0.0,
        position_y: 0.0,
        layout_rank: 2,
        source: CanvasNodeSource::User,
        payload_json: "{}".into(),
        created_at: now,
        updated_at: now,
    };
    storage
        .upsert_canvas_node(&auto_cluster)
        .await
        .expect("cluster");
    storage
        .upsert_canvas_node(&auto_insight)
        .await
        .expect("insight");
    storage
        .upsert_canvas_node(&user_note)
        .await
        .expect("note");
    storage
        .upsert_canvas_edge(&CanvasEdge {
            id: CanvasEdgeId::new(),
            canvas_id: canvas.id,
            from_id: auto_cluster.id,
            to_id: auto_insight.id,
            kind: CanvasEdgeKind::Parent,
            label: None,
            created_at: now,
        })
        .await
        .expect("edge");
    storage
        .upsert_canvas_link(&CanvasLink {
            id: CanvasLinkId::new(),
            node_id: auto_insight.id,
            ref_type: CanvasLinkRefType::Thread,
            ref_id: uuid::Uuid::new_v4().to_string(),
            snippet: None,
            created_at: now,
        })
        .await
        .expect("link");

    let deleted = storage
        .clear_auto_insight_nodes(canvas.id)
        .await
        .expect("clear");
    // The cluster row also cascade-deletes its insight child, so SQLite may
    // report 1 or 2 depending on scan order; the snapshot below is authoritative.
    assert!(deleted >= 1);

    let snap = storage
        .get_canvas_snapshot(canvas.id)
        .await
        .expect("snapshot");
    assert_eq!(snap.nodes.len(), 1);
    assert_eq!(snap.nodes[0].id, user_note.id);
    assert!(snap.edges.is_empty());
    assert!(snap.links.is_empty());

    storage
        .mark_canvas_extracted(canvas.id)
        .await
        .expect("mark");
    let marked = storage.get_canvas(canvas.id).await.expect("canvas");
    assert!(marked.last_extracted_at.is_some());
    assert!(marked.revision > snap.canvas.revision);
}
