CREATE UNIQUE INDEX IF NOT EXISTS idx_workspaces_root_path_unique
    ON workspaces(root_path);
