-- Project and thread canvases: persisted node-link graphs owned by workspaces.
CREATE TABLE IF NOT EXISTS canvases (
    id BLOB PRIMARY KEY NOT NULL,
    workspace_id BLOB NOT NULL,
    thread_id BLOB,
    title TEXT NOT NULL,
    kind TEXT NOT NULL,
    viewport_json TEXT NOT NULL DEFAULT '{"x":0,"y":0,"zoom":1}',
    revision INTEGER NOT NULL DEFAULT 0,
    last_extracted_at DATETIME,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
);

-- One project canvas per workspace.
CREATE UNIQUE INDEX IF NOT EXISTS idx_canvases_project_unique
    ON canvases (workspace_id) WHERE thread_id IS NULL AND kind = 'project';

-- One canvas per thread.
CREATE UNIQUE INDEX IF NOT EXISTS idx_canvases_thread_unique
    ON canvases (thread_id) WHERE thread_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS canvas_nodes (
    id BLOB PRIMARY KEY NOT NULL,
    canvas_id BLOB NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'todo',
    parent_id BLOB,
    position_x REAL NOT NULL DEFAULT 0,
    position_y REAL NOT NULL DEFAULT 0,
    layout_rank INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL DEFAULT 'auto',
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    FOREIGN KEY (canvas_id) REFERENCES canvases(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES canvas_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_canvas_nodes_canvas
    ON canvas_nodes (canvas_id);

CREATE TABLE IF NOT EXISTS canvas_edges (
    id BLOB PRIMARY KEY NOT NULL,
    canvas_id BLOB NOT NULL,
    from_id BLOB NOT NULL,
    to_id BLOB NOT NULL,
    kind TEXT NOT NULL,
    label TEXT,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (canvas_id) REFERENCES canvases(id) ON DELETE CASCADE,
    FOREIGN KEY (from_id) REFERENCES canvas_nodes(id) ON DELETE CASCADE,
    FOREIGN KEY (to_id) REFERENCES canvas_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_canvas_edges_canvas
    ON canvas_edges (canvas_id);

CREATE TABLE IF NOT EXISTS canvas_links (
    id BLOB PRIMARY KEY NOT NULL,
    node_id BLOB NOT NULL,
    ref_type TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    snippet TEXT,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (node_id) REFERENCES canvas_nodes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_canvas_links_node
    ON canvas_links (node_id);
