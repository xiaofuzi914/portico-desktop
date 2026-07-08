import type { Thread, Workspace } from "@/lib/schemas";

export type AgentHomeMode = "empty" | "workspace";
export type AgentHomePrimaryAction = "create-project" | "create-thread" | "continue-thread";

export interface AgentHomeViewModel {
  mode: AgentHomeMode;
  activeWorkspace?: Workspace;
  recentThreads: Thread[];
  primaryAction: AgentHomePrimaryAction;
}

export function buildAgentHomeViewModel(
  workspaces: Workspace[],
  threads: Thread[],
  selectedWorkspaceId?: string,
): AgentHomeViewModel {
  if (workspaces.length === 0) {
    return {
      mode: "empty",
      activeWorkspace: undefined,
      recentThreads: [],
      primaryAction: "create-project",
    };
  }

  const activeWorkspace =
    workspaces.find((workspace) => workspace.id === selectedWorkspaceId) ?? workspaces[0];

  const recentThreads = [...threads]
    .filter((thread) => thread.workspace_id === activeWorkspace?.id)
    .sort((a, b) => Date.parse(b.updated_at) - Date.parse(a.updated_at));

  return {
    mode: "workspace",
    activeWorkspace,
    recentThreads,
    primaryAction: recentThreads.length > 0 ? "continue-thread" : "create-thread",
  };
}
