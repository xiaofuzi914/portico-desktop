import type { Workspace } from "@/lib/schemas";

export interface ProjectOverviewItem {
  workspace: Workspace;
  isTrusted: boolean;
  readPathCount: number;
  writePathCount: number;
}

export function buildProjectOverviewItems(workspaces: Workspace[]): ProjectOverviewItem[] {
  return [...workspaces]
    .sort((a, b) => {
      const updatedDifference = Date.parse(b.updated_at) - Date.parse(a.updated_at);
      if (updatedDifference !== 0) return updatedDifference;
      return a.name.localeCompare(b.name);
    })
    .map((workspace) => ({
      workspace,
      isTrusted: workspace.trusted,
      readPathCount: workspace.allowed_read_paths.length,
      writePathCount: workspace.allowed_write_paths.length,
    }));
}
