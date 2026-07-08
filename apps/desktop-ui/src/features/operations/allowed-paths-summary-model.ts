import type { Workspace, WorkspaceId } from "@/lib/schemas";

export interface AllowedPathsSummaryItem {
  workspace: Workspace;
  readPaths: string[];
  writePaths: string[];
  totalCount: number;
}

export function buildAllowedPathsSummary(
  workspaces: Workspace[],
  workspaceId?: WorkspaceId | null,
): AllowedPathsSummaryItem[] {
  return workspaces
    .filter((workspace) => !workspaceId || workspace.id === workspaceId)
    .map((workspace) => ({
      workspace,
      readPaths: [...workspace.allowed_read_paths],
      writePaths: [...workspace.allowed_write_paths],
      totalCount: workspace.allowed_read_paths.length + workspace.allowed_write_paths.length,
    }))
    .filter((item) => item.totalCount > 0);
}
