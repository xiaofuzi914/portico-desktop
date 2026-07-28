//! Project / thread canvas (mind-map) commands.

use app_models::{
    AppError, Canvas, CanvasEdge, CanvasEdgeId, CanvasEdgeKind, CanvasId, CanvasLink, CanvasLinkId,
    CanvasLinkRefType, CanvasNode, CanvasNodeId, CanvasNodeKind, CanvasNodeSource, CanvasNodeStatus,
    CanvasSnapshot, Thread, ThreadId, WorkspaceId,
};
use app_workflows::canvas_extract::extract_session_cards;
use app_workflows::canvas_goal::decompose_goal_template;
use app_workflows::canvas_layout::{
    goal_column_x, goal_column_x_after_conversation, goal_column_x_for_session, layout_goal_spine,
    layout_session_tree, LayoutPos, NODE_WIDTH, ORIGIN_Y,
};
use tauri::State;

use crate::AppState;
use crate::error::ApiResponse;

/// Get or create the default project canvas for a workspace.
#[tauri::command]
pub async fn get_or_create_project_canvas(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<ApiResponse<Canvas>, String> {
    Ok(
        match state.runtime.storage().get_or_create_project_canvas(workspace_id).await {
            Ok(canvas) => ApiResponse::ok(canvas),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Get or create the canvas for a thread.
#[tauri::command]
pub async fn get_or_create_thread_canvas(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
) -> Result<ApiResponse<Canvas>, String> {
    Ok(
        match state
            .runtime
            .storage()
            .get_or_create_thread_canvas(workspace_id, thread_id)
            .await
        {
            Ok(canvas) => ApiResponse::ok(canvas),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Load a full canvas graph snapshot.
#[tauri::command]
pub async fn get_canvas_snapshot(
    state: State<'_, AppState>,
    canvas_id: CanvasId,
) -> Result<ApiResponse<CanvasSnapshot>, String> {
    Ok(
        match state.runtime.storage().get_canvas_snapshot(canvas_id).await {
            Ok(snapshot) => ApiResponse::ok(snapshot),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Persist pan/zoom JSON for a canvas.
#[tauri::command]
pub async fn update_canvas_viewport(
    state: State<'_, AppState>,
    canvas_id: CanvasId,
    viewport_json: String,
) -> Result<ApiResponse<Canvas>, String> {
    Ok(
        match state
            .runtime
            .storage()
            .update_canvas_viewport(canvas_id, &viewport_json)
            .await
        {
            Ok(canvas) => ApiResponse::ok(canvas),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Insert or update a canvas node.
#[tauri::command]
pub async fn upsert_canvas_node(
    state: State<'_, AppState>,
    node: CanvasNode,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.storage().upsert_canvas_node(&node).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Delete a canvas node.
#[tauri::command]
pub async fn delete_canvas_node(
    state: State<'_, AppState>,
    node_id: CanvasNodeId,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state.runtime.storage().delete_canvas_node(node_id).await {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Insert or update a canvas edge.
#[tauri::command]
pub async fn upsert_canvas_edge(
    state: State<'_, AppState>,
    edge: CanvasEdge,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.storage().upsert_canvas_edge(&edge).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Delete a canvas edge.
#[tauri::command]
pub async fn delete_canvas_edge(
    state: State<'_, AppState>,
    edge_id: CanvasEdgeId,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state.runtime.storage().delete_canvas_edge(edge_id).await {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Insert or update a canvas entity link.
#[tauri::command]
pub async fn upsert_canvas_link(
    state: State<'_, AppState>,
    link: CanvasLink,
) -> Result<ApiResponse<()>, String> {
    Ok(match state.runtime.storage().upsert_canvas_link(&link).await {
        Ok(()) => ApiResponse::ok(()),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

/// Delete a canvas entity link.
#[tauri::command]
pub async fn delete_canvas_link(
    state: State<'_, AppState>,
    link_id: CanvasLinkId,
) -> Result<ApiResponse<()>, String> {
    Ok(
        match state.runtime.storage().delete_canvas_link(link_id).await {
            Ok(()) => ApiResponse::ok(()),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

/// Extract conversation insights onto a canvas (heuristic mode only for now).
#[tauri::command]
pub async fn extract_canvas_insights(
    state: State<'_, AppState>,
    canvas_id: CanvasId,
    mode: String,
) -> Result<ApiResponse<CanvasSnapshot>, String> {
    Ok(match extract_canvas_insights_inner(&state, canvas_id, &mode).await {
        Ok(snapshot) => ApiResponse::ok(snapshot),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

async fn extract_canvas_insights_inner(
    state: &AppState,
    canvas_id: CanvasId,
    mode: &str,
) -> Result<CanvasSnapshot, AppError> {
    if mode != "heuristic" {
        return Err(AppError::Internal {
            message: format!("unsupported extract mode \"{mode}\"; only \"heuristic\" is available"),
        });
    }

    let storage = state.runtime.storage();
    let canvas = storage.get_canvas(canvas_id).await?;

    // Always load workspace sessions so the mind map shows conversation
    // relationships (parent → branch), whether opened from project or a thread.
    // User/goal/stage nodes are preserved; auto session cards are rebuilt.
    let mut threads: Vec<Thread> = storage.list_threads(canvas.workspace_id).await?;
    threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    threads.truncate(app_workflows::canvas_extract::MAX_THREADS);

    let mut pairs: Vec<(Thread, Vec<app_models::Message>)> = Vec::with_capacity(threads.len());
    for thread in threads {
        // Cap at the DB layer so long agent chats never load full history into memory.
        let messages = storage
            .list_recent_messages(
                thread.id,
                app_workflows::canvas_extract::MAX_MESSAGES_PER_THREAD,
            )
            .await?;
        pairs.push((thread, messages));
    }

    storage.clear_auto_insight_nodes(canvas_id).await?;
    // Related edges to stages are wiped with insight nodes; refresh after rebuild.
    let now = chrono::Utc::now();

    // Project + thread mind maps: one card per session, edges = branch lineage.
    // Thread canvas still builds the full workspace graph so branching is visible.
    let cards = extract_session_cards(&pairs);
    let conversation_right_x =
        persist_session_relationship_map(storage.as_ref(), canvas_id, &cards, now).await?;

    // Park Goal/Stage spines to the right of conversation so they never sit
    // inside the session tree (common source of cluttered maps).
    let goal_x = goal_column_x_after_conversation(conversation_right_x);
    let _ = relayout_goal_layer(storage.as_ref(), canvas_id, goal_x).await;

    storage.mark_canvas_extracted(canvas_id).await?;
    // Re-attach insight → stage support edges and refresh stage prompts.
    let _ = refresh_goal_links_inner(state, canvas_id).await;
    storage.get_canvas_snapshot(canvas_id).await
}

/// Persist one ThreadCluster node per session + Parent edges from parent sessions.
///
/// Returns the right edge of the laid-out conversation block for goal parking.
async fn persist_session_relationship_map(
    storage: &dyn app_runtime::storage::Storage,
    canvas_id: CanvasId,
    cards: &[app_workflows::canvas_extract::SessionCard],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<f64, AppError> {
    if cards.is_empty() {
        return Ok(app_workflows::canvas_layout::ORIGIN_X);
    }

    // Index cards for parent lookup.
    let index_by_thread: std::collections::HashMap<uuid::Uuid, usize> = cards
        .iter()
        .enumerate()
        .map(|(i, c)| (c.thread_id.0, i))
        .collect();

    let parent_of: Vec<Option<usize>> = cards
        .iter()
        .map(|c| {
            c.parent_thread_id
                .and_then(|pid| index_by_thread.get(&pid.0).copied())
        })
        .collect();

    let positions = layout_session_tree(&parent_of);
    let right = app_workflows::canvas_layout::session_tree_right_x(&positions);

    let mut node_ids: Vec<CanvasNodeId> = Vec::with_capacity(cards.len());
    for (i, card) in cards.iter().enumerate() {
        let pos = positions.get(i).copied().unwrap_or(LayoutPos {
            x: app_workflows::canvas_layout::ORIGIN_X,
            y: app_workflows::canvas_layout::ORIGIN_Y,
        });
        let node = CanvasNode {
            id: CanvasNodeId::new(),
            canvas_id,
            kind: CanvasNodeKind::ThreadCluster,
            title: card.title.clone(),
            summary: card.summary.clone(),
            status: if card.message_count == 0 {
                CanvasNodeStatus::Todo
            } else {
                CanvasNodeStatus::Done
            },
            parent_id: None,
            position_x: pos.x,
            position_y: pos.y,
            layout_rank: i as i64,
            source: CanvasNodeSource::Auto,
            payload_json: serde_json::json!({
                "thread_id": card.thread_id.0,
                "parent_thread_id": card.parent_thread_id.map(|p| p.0),
                "narrative_role": "root",
                "layer": "conversation",
                "message_count": card.message_count,
            })
            .to_string(),
            created_at: now,
            updated_at: now,
        };
        storage.upsert_canvas_node(&node).await?;
        storage
            .upsert_canvas_link(&CanvasLink {
                id: CanvasLinkId::new(),
                node_id: node.id,
                ref_type: CanvasLinkRefType::Thread,
                ref_id: card.thread_id.0.to_string(),
                snippet: Some(card.summary.clone()),
                created_at: now,
            })
            .await?;
        node_ids.push(node.id);
    }

    // Parent → child branch edges (session relationship).
    for (i, card) in cards.iter().enumerate() {
        let Some(parent_tid) = card.parent_thread_id else {
            continue;
        };
        let Some(&parent_idx) = index_by_thread.get(&parent_tid.0) else {
            continue;
        };
        if parent_idx == i {
            continue;
        }
        storage
            .upsert_canvas_edge(&CanvasEdge {
                id: CanvasEdgeId::new(),
                canvas_id,
                from_id: node_ids[parent_idx],
                to_id: node_ids[i],
                kind: CanvasEdgeKind::Parent,
                label: Some("发散".to_owned()),
                created_at: now,
            })
            .await?;
    }

    Ok(right)
}

/// Decompose a free-text goal into Stage nodes under a Goal node (template MVP).
#[tauri::command]
pub async fn decompose_canvas_goal(
    state: State<'_, AppState>,
    canvas_id: CanvasId,
    goal_text: String,
    parent_node_id: Option<CanvasNodeId>,
) -> Result<ApiResponse<CanvasSnapshot>, String> {
    Ok(
        match decompose_canvas_goal_inner(&state, canvas_id, &goal_text, parent_node_id).await {
            Ok(snapshot) => ApiResponse::ok(snapshot),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

async fn decompose_canvas_goal_inner(
    state: &AppState,
    canvas_id: CanvasId,
    goal_text: &str,
    parent_node_id: Option<CanvasNodeId>,
) -> Result<CanvasSnapshot, AppError> {
    let goal_text = goal_text.trim();
    if goal_text.is_empty() {
        return Err(AppError::Internal {
            message: "goal_text must not be empty".to_owned(),
        });
    }
    let stages = decompose_goal_template(goal_text);
    if stages.is_empty() {
        return Err(AppError::Internal {
            message: "no stages produced for goal".to_owned(),
        });
    }

    let storage = state.runtime.storage();
    let snap = storage.get_canvas_snapshot(canvas_id).await?;
    let now = chrono::Utc::now();

    // Recent conversation leaves become context for stage prompts (skip branch headers).
    let mut related_insights: Vec<&CanvasNode> = snap
        .nodes
        .iter()
        .filter(|n| n.kind == CanvasNodeKind::Insight)
        .filter(|n| narrative_role_of(n).as_deref() != Some("branch"))
        .collect();
    if related_insights.is_empty() {
        related_insights = snap
            .nodes
            .iter()
            .filter(|n| n.kind == CanvasNodeKind::Insight)
            .collect();
    }
    related_insights.sort_by_key(|n| {
        let branch = narrative_branch_of(n).unwrap_or_default();
        let branch_rank = match branch.as_str() {
            "conclusion" => 0u8,
            "progress" => 1,
            "intent" => 2,
            _ => 3,
        };
        (branch_rank, n.layout_rank)
    });
    related_insights.truncate(12);
    let related_ids: Vec<String> = related_insights.iter().map(|n| n.id.0.to_string()).collect();
    let related_titles: Vec<String> = related_insights
        .iter()
        .map(|n| n.title.clone())
        .collect();
    let context_block = if related_titles.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n【画布已整理要点】\n{}",
            related_titles
                .iter()
                .take(8)
                .map(|t| format!("- {t}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    // Prefer live conversation bbox so goals never land between narrative columns.
    let conversation_right = conversation_right_edge(&snap.nodes);
    let goal_x = if conversation_right > app_workflows::canvas_layout::ORIGIN_X + 1.0 {
        goal_column_x_after_conversation(conversation_right)
    } else {
        let cluster_count = snap
            .nodes
            .iter()
            .filter(|n| n.kind == CanvasNodeKind::ThreadCluster)
            .count();
        let branch_count = snap
            .nodes
            .iter()
            .filter(|n| {
                n.kind == CanvasNodeKind::Insight
                    && narrative_role_of(n).as_deref() == Some("branch")
            })
            .count();
        if branch_count > 0 {
            goal_column_x_for_session(branch_count)
        } else {
            goal_column_x(cluster_count)
        }
    };
    let goal_origin = LayoutPos {
        x: goal_x,
        y: ORIGIN_Y,
    };
    let (goal_pos, stage_positions) = layout_goal_spine(goal_origin, stages.len());

    let goal_node = if let Some(id) = parent_node_id {
        let existing = snap
            .nodes
            .iter()
            .find(|n| n.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound {
                resource: format!("canvas_node:{}", id.0),
            })?;
        if existing.kind != CanvasNodeKind::Goal {
            return Err(AppError::Internal {
                message: "parent_node_id must be a Goal node".to_owned(),
            });
        }
        let mut updated = existing;
        updated.title = goal_text.to_owned();
        updated.summary = if related_titles.is_empty() {
            "用户设定的项目目标".to_owned()
        } else {
            format!("关联 {} 条对话要点", related_titles.len())
        };
        updated.source = CanvasNodeSource::User;
        updated.updated_at = now;
        updated.position_x = goal_pos.x;
        updated.position_y = goal_pos.y;
        updated.payload_json = serde_json::json!({
            "acceptance": "",
            "suggested_prompt": goal_text,
            "related_insight_ids": related_ids,
            "layer": "goal",
        })
        .to_string();
        storage.upsert_canvas_node(&updated).await?;
        let snap2 = storage.get_canvas_snapshot(canvas_id).await?;
        for child in snap2.nodes.iter().filter(|n| {
            n.parent_id == Some(updated.id)
                && n.kind == CanvasNodeKind::Stage
                && n.source == CanvasNodeSource::Auto
        }) {
            let _ = storage.delete_canvas_node(child.id).await;
        }
        updated
    } else {
        let goal = CanvasNode {
            id: CanvasNodeId::new(),
            canvas_id,
            kind: CanvasNodeKind::Goal,
            title: goal_text.to_owned(),
            summary: if related_titles.is_empty() {
                "用户设定的项目目标".to_owned()
            } else {
                format!("关联 {} 条对话要点", related_titles.len())
            },
            status: CanvasNodeStatus::Todo,
            parent_id: None,
            position_x: goal_pos.x,
            position_y: goal_pos.y,
            layout_rank: 0,
            source: CanvasNodeSource::User,
            payload_json: serde_json::json!({
                "acceptance": "",
                "suggested_prompt": goal_text,
                "related_insight_ids": related_ids,
                "layer": "goal",
            })
            .to_string(),
            created_at: now,
            updated_at: now,
        };
        storage.upsert_canvas_node(&goal).await?;
        goal
    };

    for (j, stage) in stages.iter().enumerate() {
        let pos = stage_positions.get(j).copied().unwrap_or(LayoutPos {
            x: goal_node.position_x + 28.0,
            y: goal_node.position_y + (j as f64 + 1.0) * 132.0,
        });
        let prompt = format!("{}{}", stage.suggested_prompt, context_block);
        let stage_node = CanvasNode {
            id: CanvasNodeId::new(),
            canvas_id,
            kind: CanvasNodeKind::Stage,
            title: stage.title.clone(),
            summary: stage.summary.clone(),
            status: CanvasNodeStatus::Todo,
            parent_id: Some(goal_node.id),
            position_x: pos.x,
            position_y: pos.y,
            layout_rank: j as i64,
            source: CanvasNodeSource::Auto,
            payload_json: serde_json::json!({
                "acceptance": stage.acceptance,
                "suggested_prompt": prompt,
                "launch_mode": stage.launch_mode.as_str(),
                "last_run_id": null,
                "last_thread_id": null,
                "related_insight_ids": related_ids,
                "layer": "goal",
            })
            .to_string(),
            created_at: now,
            updated_at: now,
        };
        storage.upsert_canvas_node(&stage_node).await?;
        storage
            .upsert_canvas_edge(&CanvasEdge {
                id: CanvasEdgeId::new(),
                canvas_id,
                from_id: goal_node.id,
                to_id: stage_node.id,
                kind: CanvasEdgeKind::Parent,
                label: None,
                created_at: now,
            })
            .await?;
        // One soft support edge per stage (from best leaf) — avoids spiderweb.
        if let Some(insight) = pick_support_insight(&related_insights, j) {
            storage
                .upsert_canvas_edge(&CanvasEdge {
                    id: CanvasEdgeId::new(),
                    canvas_id,
                    from_id: insight.id,
                    to_id: stage_node.id,
                    kind: CanvasEdgeKind::Related,
                    label: Some("支撑".to_owned()),
                    created_at: now,
                })
                .await?;
        }
    }

    storage.get_canvas_snapshot(canvas_id).await
}

/// After the UI launches a stage, persist status + last thread/run ids into payload.
#[tauri::command]
pub async fn mark_canvas_stage_launched(
    state: State<'_, AppState>,
    node_id: CanvasNodeId,
    thread_id: ThreadId,
    run_id: Option<String>,
) -> Result<ApiResponse<CanvasNode>, String> {
    Ok(
        match mark_stage_launched_inner(&state, node_id, thread_id, run_id.as_deref()).await {
            Ok(node) => ApiResponse::ok(node),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

async fn mark_stage_launched_inner(
    state: &AppState,
    node_id: CanvasNodeId,
    thread_id: ThreadId,
    run_id: Option<&str>,
) -> Result<CanvasNode, AppError> {
    let storage = state.runtime.storage();
    // Find node via any project canvas is awkward — scan by loading is not available.
    // Use raw: we need get_node. Fall back: list not available; store node id lookup.
    // Implement via SQL through a new helper? Use get by iterating is bad.
    // Simpler: add get_canvas_node to storage if missing.
    let node = storage.get_canvas_node(node_id).await?;
    if node.kind != CanvasNodeKind::Stage && node.kind != CanvasNodeKind::Goal {
        return Err(AppError::Internal {
            message: "only Goal/Stage nodes can be marked launched".to_owned(),
        });
    }
    let mut payload: serde_json::Value =
        serde_json::from_str(&node.payload_json).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "last_thread_id".to_owned(),
            serde_json::Value::String(thread_id.0.to_string()),
        );
        if let Some(rid) = run_id {
            obj.insert(
                "last_run_id".to_owned(),
                serde_json::Value::String(rid.to_owned()),
            );
        }
    }
    let now = chrono::Utc::now();
    let mut updated = node;
    updated.payload_json = payload.to_string();
    updated.status = CanvasNodeStatus::InProgress;
    updated.updated_at = now;
    storage.upsert_canvas_node(&updated).await?;
    storage
        .upsert_canvas_link(&CanvasLink {
            id: CanvasLinkId::new(),
            node_id: updated.id,
            ref_type: CanvasLinkRefType::Thread,
            ref_id: thread_id.0.to_string(),
            snippet: Some(updated.title.clone()),
            created_at: now,
        })
        .await?;
    Ok(updated)
}

/// Mark a stage/goal done or blocked from the UI.
#[tauri::command]
pub async fn set_canvas_node_status(
    state: State<'_, AppState>,
    node_id: CanvasNodeId,
    status: String,
) -> Result<ApiResponse<CanvasNode>, String> {
    Ok(match set_node_status_inner(&state, node_id, &status).await {
        Ok(node) => ApiResponse::ok(node),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

async fn set_node_status_inner(
    state: &AppState,
    node_id: CanvasNodeId,
    status: &str,
) -> Result<CanvasNode, AppError> {
    let status = parse_node_status(status)?;
    let storage = state.runtime.storage();
    let mut node = storage.get_canvas_node(node_id).await?;
    node.status = status;
    node.updated_at = chrono::Utc::now();
    storage.upsert_canvas_node(&node).await?;
    Ok(node)
}


fn parse_node_status(value: &str) -> Result<CanvasNodeStatus, AppError> {
    match value {
        "todo" | "Todo" => Ok(CanvasNodeStatus::Todo),
        "in_progress" | "InProgress" => Ok(CanvasNodeStatus::InProgress),
        "done" | "Done" => Ok(CanvasNodeStatus::Done),
        "blocked" | "Blocked" => Ok(CanvasNodeStatus::Blocked),
        "stale" | "Stale" => Ok(CanvasNodeStatus::Stale),
        _ => Err(AppError::Internal {
            message: format!("invalid canvas node status: {value}"),
        }),
    }
}

/// After a run finishes, mark linked Stage/Goal nodes Done or Blocked.
///
/// Matches nodes by `last_run_id` / `last_thread_id` in payload (set at launch).
#[tauri::command]
pub async fn reconcile_canvas_stages_from_run(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
    run_id: String,
    outcome: String,
) -> Result<ApiResponse<Vec<CanvasNode>>, String> {
    Ok(
        match reconcile_stages_inner(&state, workspace_id, thread_id, &run_id, &outcome).await {
            Ok(nodes) => ApiResponse::ok(nodes),
            Err(err) => ApiResponse::err(err.to_string()),
        },
    )
}

async fn reconcile_stages_inner(
    state: &AppState,
    workspace_id: WorkspaceId,
    thread_id: ThreadId,
    run_id: &str,
    outcome: &str,
) -> Result<Vec<CanvasNode>, AppError> {
    let status = match outcome {
        "done" | "Done" | "Completed" | "completed" => CanvasNodeStatus::Done,
        "blocked" | "Blocked" | "Failed" | "failed" => CanvasNodeStatus::Blocked,
        "in_progress" | "InProgress" | "Running" => CanvasNodeStatus::InProgress,
        other => {
            return Err(AppError::Internal {
                message: format!("invalid reconcile outcome: {other}"),
            });
        }
    };
    let storage = state.runtime.storage();
    let mut candidates = storage
        .find_canvas_nodes_by_payload_needle(workspace_id, run_id)
        .await?;
    if candidates.is_empty() {
        // Fall back: match by launched thread id.
        candidates = storage
            .find_canvas_nodes_by_payload_needle(workspace_id, &thread_id.0.to_string())
            .await?;
    }

    let now = chrono::Utc::now();
    let mut updated_nodes = Vec::new();
    for mut node in candidates {
        // Only touch InProgress stages/goals (don't clobber user-marked Done).
        if !matches!(
            node.status,
            CanvasNodeStatus::InProgress | CanvasNodeStatus::Todo
        ) && status == CanvasNodeStatus::Done
        {
            // Allow Done overwrite only from InProgress/Todo; skip already Blocked→Done? allow.
        }
        if node.status == CanvasNodeStatus::Done && status == CanvasNodeStatus::Blocked {
            // Prefer failure signal if still somehow Done.
        }
        let mut payload: serde_json::Value =
            serde_json::from_str(&node.payload_json).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = payload.as_object_mut() {
            obj.insert(
                "last_outcome".to_owned(),
                serde_json::Value::String(outcome.to_owned()),
            );
            obj.insert(
                "last_run_id".to_owned(),
                serde_json::Value::String(run_id.to_owned()),
            );
            obj.insert(
                "last_thread_id".to_owned(),
                serde_json::Value::String(thread_id.0.to_string()),
            );
        }
        node.payload_json = payload.to_string();
        node.status = status;
        node.updated_at = now;
        storage.upsert_canvas_node(&node).await?;

        // If a stage completes, try to mark parent Goal Done when all sibling stages Done.
        if status == CanvasNodeStatus::Done {
            if let Some(parent_id) = node.parent_id {
                maybe_complete_parent_goal(storage.as_ref(), parent_id).await?;
            }
        }
        updated_nodes.push(node);
    }
    Ok(updated_nodes)
}

async fn maybe_complete_parent_goal(
    storage: &dyn app_runtime::Storage,
    parent_id: CanvasNodeId,
) -> Result<(), AppError> {
    let parent = match storage.get_canvas_node(parent_id).await {
        Ok(n) => n,
        Err(_) => return Ok(()),
    };
    if parent.kind != CanvasNodeKind::Goal {
        return Ok(());
    }
    let snap = storage.get_canvas_snapshot(parent.canvas_id).await?;
    let stages: Vec<_> = snap
        .nodes
        .iter()
        .filter(|n| n.parent_id == Some(parent_id) && n.kind == CanvasNodeKind::Stage)
        .collect();
    if stages.is_empty() {
        return Ok(());
    }
    if stages.iter().all(|s| s.status == CanvasNodeStatus::Done) {
        let mut goal = parent;
        goal.status = CanvasNodeStatus::Done;
        goal.updated_at = chrono::Utc::now();
        storage.upsert_canvas_node(&goal).await?;
    }
    Ok(())
}

/// Re-link recent conversation insights to existing goal stages (after extract).
#[tauri::command]
pub async fn refresh_canvas_goal_links(
    state: State<'_, AppState>,
    canvas_id: CanvasId,
) -> Result<ApiResponse<CanvasSnapshot>, String> {
    Ok(match refresh_goal_links_inner(&state, canvas_id).await {
        Ok(snap) => ApiResponse::ok(snap),
        Err(err) => ApiResponse::err(err.to_string()),
    })
}

async fn refresh_goal_links_inner(
    state: &AppState,
    canvas_id: CanvasId,
) -> Result<CanvasSnapshot, AppError> {
    let storage = state.runtime.storage();
    let snap = storage.get_canvas_snapshot(canvas_id).await?;
    // Prefer leaf insights only (skip branch headers) so support edges stay short
    // and readable. Fall back to any insight if the canvas has no narrative leaves.
    let mut leaf_insights: Vec<CanvasNode> = snap
        .nodes
        .iter()
        .filter(|n| n.kind == CanvasNodeKind::Insight)
        .filter(|n| narrative_role_of(n).as_deref() != Some("branch"))
        .cloned()
        .collect();
    if leaf_insights.is_empty() {
        leaf_insights = snap
            .nodes
            .iter()
            .filter(|n| n.kind == CanvasNodeKind::Insight)
            .cloned()
            .collect();
    }
    // Prefer conclusion → progress → intent for stable, meaningful support links.
    leaf_insights.sort_by_key(|n| {
        let branch = narrative_branch_of(n).unwrap_or_default();
        let branch_rank = match branch.as_str() {
            "conclusion" => 0u8,
            "progress" => 1,
            "intent" => 2,
            _ => 3,
        };
        (branch_rank, n.layout_rank)
    });
    let insights: Vec<_> = leaf_insights.into_iter().take(12).collect();
    let stages: Vec<_> = snap
        .nodes
        .iter()
        .filter(|n| n.kind == CanvasNodeKind::Stage)
        .cloned()
        .collect();
    let related_ids: Vec<String> = insights.iter().map(|n| n.id.0.to_string()).collect();
    let now = chrono::Utc::now();

    // Drop previous "支撑" related edges so re-extract does not duplicate.
    for edge in snap.edges.iter().filter(|e| e.kind == CanvasEdgeKind::Related) {
        let _ = storage.delete_canvas_edge(edge.id).await;
    }

    for (stage_idx, mut stage) in stages.into_iter().enumerate() {
        let mut payload: serde_json::Value =
            serde_json::from_str(&stage.payload_json).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = payload.as_object_mut() {
            obj.insert(
                "related_insight_ids".to_owned(),
                serde_json::json!(related_ids),
            );
            // Append context to suggested_prompt once (avoid unbounded growth).
            if let Some(serde_json::Value::String(prompt)) = obj.get("suggested_prompt").cloned() {
                let base = prompt
                    .split("\n\n【画布已整理要点】")
                    .next()
                    .unwrap_or(prompt.as_str())
                    .to_owned();
                if !related_ids.is_empty() {
                    let titles: String = insights
                        .iter()
                        .take(8)
                        .map(|i| format!("- {}", i.title))
                        .collect::<Vec<_>>()
                        .join("\n");
                    obj.insert(
                        "suggested_prompt".to_owned(),
                        serde_json::Value::String(format!(
                            "{base}\n\n【画布已整理要点】\n{titles}"
                        )),
                    );
                } else {
                    obj.insert("suggested_prompt".to_owned(), serde_json::Value::String(base));
                }
            }
        }
        stage.payload_json = payload.to_string();
        stage.updated_at = now;
        storage.upsert_canvas_node(&stage).await?;

        // One support edge per stage keeps the map scannable.
        if let Some(insight) = pick_support_insight(&insights.iter().collect::<Vec<_>>(), stage_idx)
        {
            storage
                .upsert_canvas_edge(&CanvasEdge {
                    id: CanvasEdgeId::new(),
                    canvas_id,
                    from_id: insight.id,
                    to_id: stage.id,
                    kind: CanvasEdgeKind::Related,
                    label: Some("支撑".to_owned()),
                    created_at: now,
                })
                .await?;
        }
    }

    storage.get_canvas_snapshot(canvas_id).await
}

/// Reposition all Goal nodes (and Stage children) into clean spines at `goal_x`.
///
/// Also parks **orphan stages** (no parent Goal) into a spine so they do not sit
/// inside the conversation columns after extract. Only `position_x/y` + rank change.
async fn relayout_goal_layer(
    storage: &dyn app_runtime::storage::Storage,
    canvas_id: CanvasId,
    goal_x: f64,
) -> Result<(), AppError> {
    let snap = storage.get_canvas_snapshot(canvas_id).await?;
    let mut goals: Vec<CanvasNode> = snap
        .nodes
        .iter()
        .filter(|n| n.kind == CanvasNodeKind::Goal)
        .cloned()
        .collect();
    // Stable order: older goals first (creation time), then id.
    goals.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.0.cmp(&b.id.0))
    });

    let goal_ids: std::collections::HashSet<_> = goals.iter().map(|g| g.id).collect();
    let now = chrono::Utc::now();
    let pitch = app_workflows::canvas_layout::column_pitch();
    let mut col = 0usize;

    for mut goal in goals {
        let mut stages: Vec<CanvasNode> = snap
            .nodes
            .iter()
            .filter(|n| n.parent_id == Some(goal.id) && n.kind == CanvasNodeKind::Stage)
            .cloned()
            .collect();
        stages.sort_by_key(|s| s.layout_rank);

        let origin = LayoutPos {
            x: goal_x + col as f64 * pitch,
            y: ORIGIN_Y,
        };
        let (gpos, spos) = layout_goal_spine(origin, stages.len());
        goal.position_x = gpos.x;
        goal.position_y = gpos.y;
        goal.layout_rank = col as i64;
        goal.updated_at = now;
        storage.upsert_canvas_node(&goal).await?;

        for (j, mut stage) in stages.into_iter().enumerate() {
            let pos = spos.get(j).copied().unwrap_or(LayoutPos {
                x: gpos.x,
                y: gpos.y + app_workflows::canvas_layout::row_pitch() * (j as f64 + 1.0),
            });
            stage.position_x = pos.x;
            stage.position_y = pos.y;
            stage.layout_rank = j as i64;
            stage.updated_at = now;
            storage.upsert_canvas_node(&stage).await?;
        }
        col += 1;
    }

    // Orphan stages (added via toolbar without a Goal) — stack in next column.
    let mut orphans: Vec<CanvasNode> = snap
        .nodes
        .iter()
        .filter(|n| n.kind == CanvasNodeKind::Stage)
        .filter(|n| match n.parent_id {
            None => true,
            Some(pid) => !goal_ids.contains(&pid),
        })
        .cloned()
        .collect();
    if !orphans.is_empty() {
        orphans.sort_by(|a, b| {
            a.layout_rank
                .cmp(&b.layout_rank)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });
        let origin = LayoutPos {
            x: goal_x + col as f64 * pitch,
            y: ORIGIN_Y,
        };
        // Treat first orphan as the "head" of the spine (no Goal card).
        for (j, mut stage) in orphans.into_iter().enumerate() {
            stage.position_x = origin.x;
            stage.position_y = origin.y + j as f64 * app_workflows::canvas_layout::row_pitch();
            stage.layout_rank = j as i64;
            stage.updated_at = now;
            storage.upsert_canvas_node(&stage).await?;
        }
    }
    Ok(())
}

/// Right edge of conversation cards (ThreadCluster + Insight).
fn conversation_right_edge(nodes: &[CanvasNode]) -> f64 {
    nodes
        .iter()
        .filter(|n| {
            matches!(
                n.kind,
                CanvasNodeKind::ThreadCluster | CanvasNodeKind::Insight
            )
        })
        .map(|n| n.position_x + NODE_WIDTH)
        .fold(app_workflows::canvas_layout::ORIGIN_X, f64::max)
}

fn narrative_role_of(node: &CanvasNode) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&node.payload_json).ok()?;
    v.get("narrative_role")?
        .as_str()
        .map(str::to_owned)
}

fn narrative_branch_of(node: &CanvasNode) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&node.payload_json).ok()?;
    v.get("branch")?.as_str().map(str::to_owned)
}

/// Pick one leaf insight to soft-link to stage `stage_idx`.
///
/// Spreads stages across the ranked insight list so multiple stages do not all
/// attach to the same top card (which draws a fan of dashed lines).
fn pick_support_insight<'a>(insights: &[&'a CanvasNode], stage_idx: usize) -> Option<&'a CanvasNode> {
    if insights.is_empty() {
        return None;
    }
    // Prefer conclusion leaves for early stages, then walk the ranked list.
    let idx = stage_idx % insights.len();
    insights.get(idx).copied()
}
