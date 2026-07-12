import type { Workspace, WorkspaceId } from "@/lib/schemas";

export type SidebarProjectItem =
  | { kind: "overview"; overflowCount: number }
  | { kind: "workspace"; workspace: Workspace; isRunning: boolean };

export interface SidebarProjectOptions {
  maxItems?: number;
  runningWorkspaceIds?: ReadonlySet<WorkspaceId>;
  lastUsedAtByWorkspaceId?: ReadonlyMap<string, string | number>;
}

const DEFAULT_MAX_ITEMS = 5;

export function buildSidebarProjectItems(
  workspaces: Workspace[],
  options: SidebarProjectOptions = {},
): SidebarProjectItem[] {
  const maxItems = Math.max(1, options.maxItems ?? DEFAULT_MAX_ITEMS);
  const runningWorkspaceIds = options.runningWorkspaceIds ?? new Set<WorkspaceId>();
  const lastUsedAtByWorkspaceId =
    options.lastUsedAtByWorkspaceId ?? new Map<string, string | number>();
  const sortedWorkspaces = [...workspaces].sort((a, b) => {
    const runningDifference =
      Number(runningWorkspaceIds.has(b.id)) - Number(runningWorkspaceIds.has(a.id));
    if (runningDifference !== 0) return runningDifference;

    const updatedDifference =
      projectTimestamp(b, lastUsedAtByWorkspaceId) - projectTimestamp(a, lastUsedAtByWorkspaceId);
    if (updatedDifference !== 0) return updatedDifference;

    return a.name.localeCompare(b.name);
  });

  if (sortedWorkspaces.length <= maxItems) {
    return sortedWorkspaces.map((workspace) => ({
      kind: "workspace",
      workspace,
      isRunning: runningWorkspaceIds.has(workspace.id),
    }));
  }

  return [
    { kind: "overview", overflowCount: sortedWorkspaces.length },
    ...sortedWorkspaces.slice(0, maxItems - 1).map((workspace) => ({
      kind: "workspace" as const,
      workspace,
      isRunning: runningWorkspaceIds.has(workspace.id),
    })),
  ];
}

function projectTimestamp(
  workspace: Workspace,
  lastUsedAtByWorkspaceId: ReadonlyMap<string, string | number>,
): number {
  const lastUsedAt = lastUsedAtByWorkspaceId.get(workspace.id);
  if (typeof lastUsedAt === "number") return lastUsedAt;
  if (typeof lastUsedAt === "string") return Date.parse(lastUsedAt);
  return Date.parse(workspace.updated_at);
}
