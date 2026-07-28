import type { Workspace, WorkspaceId } from "@/lib/schemas";

/** One folder row in the allowlist summary (folder-centric, not path-list). */
export interface AllowedFolderItem {
  /** Absolute path (stable key). */
  path: string;
  /** Last path segment for display. */
  name: string;
  /** Parent directory shown as muted secondary line when useful. */
  parentPath: string;
  isProjectRoot: boolean;
  canRead: boolean;
  canWrite: boolean;
}

export interface AllowedPathsSummaryItem {
  workspace: Workspace;
  /** Unique folders with read/write/root annotations. */
  folders: AllowedFolderItem[];
  totalCount: number;
}

function normalizePath(path: string): string {
  const trimmed = path.trim();
  if (!trimmed) return "";
  // Collapse trailing slashes except root.
  if (trimmed.length > 1 && (trimmed.endsWith("/") || trimmed.endsWith("\\"))) {
    return trimmed.replace(/[/\\]+$/, "");
  }
  return trimmed;
}

function pathBasename(path: string): string {
  const normalized = normalizePath(path);
  if (!normalized) return path;
  const parts = normalized.split(/[/\\]/).filter(Boolean);
  return parts[parts.length - 1] ?? normalized;
}

function pathParent(path: string): string {
  const normalized = normalizePath(path);
  if (!normalized) return "";
  const idx = Math.max(normalized.lastIndexOf("/"), normalized.lastIndexOf("\\"));
  if (idx <= 0) return "";
  return normalized.slice(0, idx);
}

/**
 * Collapse allowlist read/write paths (+ project root) into unique folders
 * with permission tags. Same path with both read+write appears once.
 */
export function buildFolderAllowlist(
  workspace: Workspace,
): AllowedFolderItem[] {
  const root = normalizePath(workspace.root_path);
  const readSet = new Set(
    workspace.allowed_read_paths.map(normalizePath).filter(Boolean),
  );
  const writeSet = new Set(
    workspace.allowed_write_paths.map(normalizePath).filter(Boolean),
  );

  const all = new Set<string>();
  if (root) all.add(root);
  for (const p of readSet) all.add(p);
  for (const p of writeSet) all.add(p);

  const folders: AllowedFolderItem[] = [...all].map((path) => {
    const isProjectRoot = Boolean(root && path === root);
    // Project root is always treated as readable when trusted workspace root.
    const canRead = readSet.has(path) || isProjectRoot;
    const canWrite = writeSet.has(path);
    return {
      path,
      name: pathBasename(path) || path,
      parentPath: pathParent(path),
      isProjectRoot,
      canRead,
      canWrite,
    };
  });

  // Root first, then by name.
  folders.sort((a, b) => {
    if (a.isProjectRoot !== b.isProjectRoot) return a.isProjectRoot ? -1 : 1;
    return a.name.localeCompare(b.name) || a.path.localeCompare(b.path);
  });

  return folders;
}

export function buildAllowedPathsSummary(
  workspaces: Workspace[],
  workspaceId?: WorkspaceId | null,
): AllowedPathsSummaryItem[] {
  return workspaces
    .filter((workspace) => !workspaceId || workspace.id === workspaceId)
    // Only surface projects that have an explicit allowlist (not bare root only).
    .filter(
      (workspace) =>
        workspace.allowed_read_paths.length > 0 || workspace.allowed_write_paths.length > 0,
    )
    .map((workspace) => {
      const folders = buildFolderAllowlist(workspace);
      return {
        workspace,
        folders,
        totalCount: folders.length,
      };
    })
    .filter((item) => item.totalCount > 0);
}

/** Back-compat helpers for tests / callers that still think in path lists. */
export function flattenReadPaths(item: AllowedPathsSummaryItem): string[] {
  return item.folders.filter((f) => f.canRead).map((f) => f.path);
}

export function flattenWritePaths(item: AllowedPathsSummaryItem): string[] {
  return item.folders.filter((f) => f.canWrite).map((f) => f.path);
}
