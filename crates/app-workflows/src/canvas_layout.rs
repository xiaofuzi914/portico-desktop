//! Hierarchical layout for canvas node trees (no overlap).
//!
//! Uses fixed card metrics matching the React Flow node chrome so extract
//! positions render without stacking.

/// Card width used for column pitch (matches session card `w-[280px]`).
pub const NODE_WIDTH: f64 = 280.0;
/// Estimated card height for session title + multi-line summary chrome.
pub const NODE_HEIGHT: f64 = 148.0;
/// Horizontal gap between columns (session narrative / default).
pub const COL_GAP: f64 = 96.0;
/// Project map: wider gap so session columns do not feel glued.
pub const PROJECT_COL_GAP: f64 = 96.0;
/// Extra vertical gap between project grid rows (after tallest column in row).
pub const PROJECT_ROW_GAP: f64 = 80.0;
/// Max session columns per project-map row before wrapping.
pub const PROJECT_MAX_COLS: usize = 3;
/// Vertical gap between stacked cards.
pub const ROW_GAP: f64 = 44.0;
/// Branch card height (matches `h-[72px]` in `canvas-node.tsx`); leaves sit closer
/// under a branch than under a full-height card.
pub const BRANCH_HEIGHT: f64 = 72.0;
/// Indent of children under a parent cluster (legacy forest only).
pub const CHILD_INDENT: f64 = 28.0;
/// Horizontal breathing room between conversation block and goal spine.
pub const LAYER_GAP: f64 = 120.0;
/// Left/top margin of the whole forest.
pub const ORIGIN_X: f64 = 48.0;
pub const ORIGIN_Y: f64 = 48.0;

/// Column pitch (width + gap).
#[must_use]
pub const fn column_pitch() -> f64 {
    NODE_WIDTH + COL_GAP
}

/// Project map column pitch (wider).
#[must_use]
pub const fn project_column_pitch() -> f64 {
    NODE_WIDTH + PROJECT_COL_GAP
}

/// Row pitch (height + gap).
#[must_use]
pub const fn row_pitch() -> f64 {
    NODE_HEIGHT + ROW_GAP
}

/// Position of one laid-out node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutPos {
    pub x: f64,
    pub y: f64,
}

/// Lay out a **session relationship tree** (parent → branched children).
///
/// `parent_of[i]` is the parent index of node `i`, or `None` for roots.
/// Returns one position per input node (same order).
///
/// Roots sit on a top row; children fan out below their parent so the mind map
/// reads as conversation branching rather than a dense insight dump.
#[must_use]
pub fn layout_session_tree(parent_of: &[Option<usize>]) -> Vec<LayoutPos> {
    let n = parent_of.len();
    if n == 0 {
        return Vec::new();
    }

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut roots: Vec<usize> = Vec::new();
    for (i, parent) in parent_of.iter().enumerate() {
        match parent {
            Some(p) if *p < n && *p != i => children[*p].push(i),
            _ => roots.push(i),
        }
    }
    if roots.is_empty() {
        // Cycle / all parents self — treat every node as root.
        roots = (0..n).collect();
        children = vec![Vec::new(); n];
    }

    fn subtree_width(i: usize, children: &[Vec<usize>]) -> f64 {
        if children[i].is_empty() {
            return 1.0;
        }
        children[i]
            .iter()
            .map(|&c| subtree_width(c, children))
            .sum::<f64>()
            .max(1.0)
    }

    fn place(
        i: usize,
        left: f64,
        depth: usize,
        children: &[Vec<usize>],
        positions: &mut [LayoutPos],
        pitch: f64,
        v_pitch: f64,
    ) -> f64 {
        let w = subtree_width(i, children);
        let center_x = left + (w - 1.0) * pitch / 2.0;
        positions[i] = LayoutPos {
            x: ORIGIN_X + center_x,
            y: ORIGIN_Y + depth as f64 * v_pitch,
        };
        if children[i].is_empty() {
            return w;
        }
        let mut cursor = left;
        for &c in &children[i] {
            let cw = place(c, cursor, depth + 1, children, positions, pitch, v_pitch);
            cursor += cw * pitch;
        }
        w
    }

    let mut positions = vec![LayoutPos { x: 0.0, y: 0.0 }; n];
    let pitch = column_pitch();
    let v_pitch = row_pitch() + 24.0;
    let mut cursor = 0.0;
    for &r in &roots {
        let w = place(r, cursor, 0, &children, &mut positions, pitch, v_pitch);
        cursor += w * pitch + pitch * 0.35; // gap between root forests
    }

    positions
}

/// Right edge of a session relationship tree layout.
#[must_use]
pub fn session_tree_right_x(positions: &[LayoutPos]) -> f64 {
    max_right_edge(positions)
}

/// Lay out thread clusters left-to-right; each cluster's insights stack below
/// (legacy; prefer [`layout_project_forest`] for project canvas).
///
/// `cluster_child_counts[i]` = number of insight children for cluster `i`.
#[must_use]
pub fn layout_cluster_forest(cluster_child_counts: &[usize]) -> (Vec<LayoutPos>, Vec<Vec<LayoutPos>>) {
    let mut clusters = Vec::with_capacity(cluster_child_counts.len());
    let mut children = Vec::with_capacity(cluster_child_counts.len());

    for (i, &n_children) in cluster_child_counts.iter().enumerate() {
        let cx = ORIGIN_X + i as f64 * column_pitch();
        let cy = ORIGIN_Y;
        clusters.push(LayoutPos { x: cx, y: cy });

        let mut kids = Vec::with_capacity(n_children);
        for j in 0..n_children {
            kids.push(LayoutPos {
                x: cx + CHILD_INDENT,
                y: cy + row_pitch() + j as f64 * row_pitch(),
            });
        }
        children.push(kids);
    }
    (clusters, children)
}

/// Project canvas: grid of session roots with summary leaves as a **vertical spine**.
///
/// Leaves share the root's X (top/bottom handles). Edges should be chained
/// root→leaf0→leaf1→… so paths do not stack through intermediate cards.
///
/// Wraps after [`PROJECT_MAX_COLS`] so many sessions do not form one endless row.
/// Row height grows with the tallest stack in that row so columns never collide.
#[must_use]
pub fn layout_project_forest(
    cluster_child_counts: &[usize],
) -> (Vec<LayoutPos>, Vec<Vec<LayoutPos>>) {
    let n = cluster_child_counts.len();
    let mut clusters = Vec::with_capacity(n);
    let mut children = Vec::with_capacity(n);
    if n == 0 {
        return (clusters, children);
    }

    let cols = PROJECT_MAX_COLS.max(1);
    let pitch = project_column_pitch();
    // Extra air under the session root before the first leaf so the first
    // Parent edge is a short clean drop, not a tight squeeze.
    let first_leaf_gap = row_pitch() + ROW_GAP;
    let mut row_start = 0usize;
    let mut current_y = ORIGIN_Y;

    while row_start < n {
        let row_end = (row_start + cols).min(n);
        let row_counts = &cluster_child_counts[row_start..row_end];
        let max_kids = row_counts.iter().copied().max().unwrap_or(0);
        // Root + gap-to-first + remaining leaf rows + breathing room before next grid row.
        let row_block_height = if max_kids == 0 {
            row_pitch() + PROJECT_ROW_GAP
        } else {
            first_leaf_gap + max_kids as f64 * row_pitch() + PROJECT_ROW_GAP
        };

        for (i, &n_children) in row_counts.iter().enumerate() {
            let cx = ORIGIN_X + i as f64 * pitch;
            let cy = current_y;
            clusters.push(LayoutPos { x: cx, y: cy });
            let mut kids = Vec::with_capacity(n_children);
            for j in 0..n_children {
                kids.push(LayoutPos {
                    // Center under parent (top/bottom handles) — one clean spine.
                    x: cx,
                    y: cy + first_leaf_gap + j as f64 * row_pitch(),
                });
            }
            children.push(kids);
        }

        current_y += row_block_height;
        row_start = row_end;
    }

    (clusters, children)
}

/// Lay out a goal and its stages as a vertical spine starting at `origin`.
///
/// Stages share the goal's X so the goal column reads as one clean stack
/// (no sideways indent that collides with conversation columns).
#[must_use]
pub fn layout_goal_spine(origin: LayoutPos, stage_count: usize) -> (LayoutPos, Vec<LayoutPos>) {
    let goal = origin;
    let mut stages = Vec::with_capacity(stage_count);
    for j in 0..stage_count {
        stages.push(LayoutPos {
            x: origin.x,
            y: origin.y + row_pitch() + j as f64 * row_pitch(),
        });
    }
    (goal, stages)
}

/// Right edge (x + width) of a laid-out session narrative with `branch_count` columns.
#[must_use]
pub fn session_narrative_right_x(branch_count: usize) -> f64 {
    let n = branch_count.max(1);
    ORIGIN_X + (n as f64) * NODE_WIDTH + (n.saturating_sub(1) as f64) * COL_GAP
}

/// Right edge of a project-map conversation grid.
#[must_use]
pub fn project_forest_right_x(cluster_count: usize) -> f64 {
    if cluster_count == 0 {
        return ORIGIN_X;
    }
    let cols = cluster_count.min(PROJECT_MAX_COLS).max(1);
    // Last column origin + card width.
    ORIGIN_X + (cols.saturating_sub(1) as f64) * project_column_pitch() + NODE_WIDTH
}

/// X origin for the goal column when conversation clusters occupy the left.
///
/// Uses project grid width so goals never land between session columns.
#[must_use]
pub fn goal_column_x(cluster_count: usize) -> f64 {
    if cluster_count == 0 {
        ORIGIN_X
    } else {
        project_forest_right_x(cluster_count) + LAYER_GAP
    }
}

/// X origin for the goal column to the right of a session narrative.
#[must_use]
pub fn goal_column_x_for_session(branch_count: usize) -> f64 {
    session_narrative_right_x(branch_count) + LAYER_GAP
}

/// Place goals clear of an already-laid-out conversation block.
///
/// `conversation_right_x` is the max of `node.position_x + NODE_WIDTH` over
/// conversation nodes (ThreadCluster / Insight). Returns at least [`ORIGIN_X`].
#[must_use]
pub fn goal_column_x_after_conversation(conversation_right_x: f64) -> f64 {
    if !conversation_right_x.is_finite() || conversation_right_x <= ORIGIN_X {
        ORIGIN_X
    } else {
        conversation_right_x + LAYER_GAP
    }
}

/// Max right edge of nodes at the given positions (position is top-left of card).
#[must_use]
pub fn max_right_edge<'a, I>(positions: I) -> f64
where
    I: IntoIterator<Item = &'a LayoutPos>,
{
    positions
        .into_iter()
        .map(|p| p.x + NODE_WIDTH)
        .fold(ORIGIN_X, f64::max)
}

/// Next free column index given existing cluster x positions.
#[must_use]
pub fn next_cluster_column(existing_cluster_xs: &[f64]) -> usize {
    if existing_cluster_xs.is_empty() {
        return 0;
    }
    let max_x = existing_cluster_xs
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let idx = ((max_x - ORIGIN_X) / column_pitch()).floor() as i64 + 1;
    idx.max(0) as usize
}

/// Session mind-map: root on top, then 1–3 narrative columns (intent / progress / conclusion).
///
/// `branch_leaf_counts[i]` = number of leaf cards under branch column `i`.
/// Returns `(root, branches, leaves_per_branch)`.
#[must_use]
pub fn layout_session_narrative(
    branch_leaf_counts: &[usize],
) -> (LayoutPos, Vec<LayoutPos>, Vec<Vec<LayoutPos>>) {
    let n = branch_leaf_counts.len().max(1);
    // Center root above the middle of the branch row.
    let span = (n.saturating_sub(1)) as f64 * column_pitch();
    let left = ORIGIN_X;
    let root = LayoutPos {
        x: left + span / 2.0,
        y: ORIGIN_Y,
    };
    // Extra drop under the root so fan-out arms have a clear vertical stub into
    // each branch (arrows must point *down* into 意图/推进/结论, not sideways).
    let branch_y = ORIGIN_Y + row_pitch() + ROW_GAP * 2.5;
    // Branch cards are shorter than full cards: leaves start right below the
    // branch chrome so column rhythm matches the leaf-to-leaf gap.
    let leaf_start_y = branch_y + BRANCH_HEIGHT + ROW_GAP;
    let mut branches = Vec::with_capacity(n);
    let mut leaves = Vec::with_capacity(n);
    for (i, &count) in branch_leaf_counts.iter().enumerate() {
        let bx = left + i as f64 * column_pitch();
        branches.push(LayoutPos { x: bx, y: branch_y });
        let mut kids = Vec::with_capacity(count);
        for j in 0..count {
            kids.push(LayoutPos {
                x: bx,
                y: leaf_start_y + j as f64 * row_pitch(),
            });
        }
        leaves.push(kids);
    }
    (root, branches, leaves)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forest_does_not_overlap_columns() {
        let (clusters, kids) = layout_cluster_forest(&[3, 2]);
        assert_eq!(clusters.len(), 2);
        assert!(clusters[1].x - clusters[0].x >= NODE_WIDTH + COL_GAP - 0.1);
        assert_eq!(kids[0].len(), 3);
        assert!(kids[0][1].y - kids[0][0].y >= NODE_HEIGHT + ROW_GAP - 0.1);
        assert!(kids[0][0].x > clusters[0].x);
    }

    #[test]
    fn project_forest_centers_children_and_wraps() {
        let (clusters, kids) = layout_project_forest(&[2, 1, 3, 1]);
        assert_eq!(clusters.len(), 4);
        // First row: 3 sessions; second row: 1.
        assert_eq!(clusters[0].y, clusters[1].y);
        assert_eq!(clusters[1].y, clusters[2].y);
        assert!(clusters[3].y > clusters[0].y);
        // Wider pitch than legacy forest.
        assert!(clusters[1].x - clusters[0].x >= NODE_WIDTH + PROJECT_COL_GAP - 0.1);
        // Children share parent x (centered spine).
        assert!((kids[0][0].x - clusters[0].x).abs() < 0.01);
        assert!(kids[0][0].y > clusters[0].y);
        // First leaf sits below root with breathing room.
        assert!(kids[0][0].y - clusters[0].y >= row_pitch() + ROW_GAP - 0.1);
        // Leaf-to-leaf keeps full pitch.
        assert!((kids[0][1].y - kids[0][0].y - row_pitch()).abs() < 0.01);
        // Second row has room below first row's tallest stack (3 kids on col 2).
        let first_row_bottom =
            clusters[2].y + row_pitch() + ROW_GAP + 3.0 * row_pitch();
        assert!(clusters[3].y >= first_row_bottom - 0.1);
    }

    #[test]
    fn goal_spine_stacks_stages_under_goal() {
        let (goal, stages) = layout_goal_spine(LayoutPos { x: 10.0, y: 20.0 }, 4);
        assert_eq!(goal.x, 10.0);
        assert_eq!(stages.len(), 4);
        assert!(stages[0].y > goal.y);
        // Stages align under goal (no sideways indent into conversation).
        assert!((stages[0].x - goal.x).abs() < 0.01);
        assert!((stages[3].x - goal.x).abs() < 0.01);
    }

    #[test]
    fn next_column_advances_past_existing() {
        let xs = vec![ORIGIN_X, ORIGIN_X + column_pitch()];
        assert_eq!(next_cluster_column(&xs), 2);
        assert_eq!(next_cluster_column(&[]), 0);
    }

    #[test]
    fn session_narrative_is_three_columns_under_root() {
        let (root, branches, leaves) = layout_session_narrative(&[2, 1, 3]);
        assert_eq!(branches.len(), 3);
        assert_eq!(leaves[0].len(), 2);
        assert_eq!(leaves[2].len(), 3);
        assert!(branches[1].x > branches[0].x);
        assert!(branches[0].y > root.y);
        assert!(leaves[0][0].y > branches[0].y);
        // Columns do not overlap horizontally.
        assert!(branches[1].x - branches[0].x >= NODE_WIDTH + COL_GAP - 0.1);
        // First leaf hugs the shorter branch card; leaf rows keep full pitch.
        let first_gap = leaves[0][0].y - branches[0].y;
        assert!((first_gap - (BRANCH_HEIGHT + ROW_GAP)).abs() < 0.01);
        let leaf_gap = leaves[0][1].y - leaves[0][0].y;
        assert!((leaf_gap - row_pitch()).abs() < 0.01);
        // Root is centered above the branch span.
        let mid = (branches[0].x + branches[2].x) / 2.0;
        assert!((root.x - mid).abs() < 0.01);
    }

    #[test]
    fn goal_column_sits_clear_of_session_narrative() {
        let (_root, branches, _leaves) = layout_session_narrative(&[2, 2, 2]);
        let narrative_right = session_narrative_right_x(3);
        let goal_x = goal_column_x_for_session(3);
        assert!(goal_x >= narrative_right + LAYER_GAP - 0.1);
        // Must not land between branch columns.
        for b in &branches {
            let branch_right = b.x + NODE_WIDTH;
            assert!(
                goal_x >= branch_right + LAYER_GAP - 0.1,
                "goal_x={goal_x} collides with branch at {}",
                b.x
            );
        }
    }

    #[test]
    fn goal_column_sits_clear_of_project_grid() {
        let (clusters, _) = layout_project_forest(&[1, 1, 1]);
        let right = project_forest_right_x(3);
        let goal_x = goal_column_x(3);
        assert!(goal_x >= right + LAYER_GAP - 0.1);
        let last = clusters.last().unwrap();
        assert!(goal_x >= last.x + NODE_WIDTH + LAYER_GAP - 0.1);
    }

    #[test]
    fn goal_after_conversation_bbox() {
        let right = 900.0;
        let gx = goal_column_x_after_conversation(right);
        assert!((gx - (right + LAYER_GAP)).abs() < 0.01);
        assert_eq!(goal_column_x_after_conversation(f64::NAN), ORIGIN_X);
    }
}
