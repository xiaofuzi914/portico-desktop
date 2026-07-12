import type { Thread, Workspace, WorkspaceId } from "@/lib/schemas";

export type AgentHomeMode = "empty" | "workspace";
export type AgentHomePrimaryAction = "create-project" | "create-thread" | "continue-thread";

export interface GlobalThreadItem {
  thread: Thread;
  workspace: Workspace;
}

export interface AgentHomeViewModel {
  mode: AgentHomeMode;
  activeWorkspace?: Workspace;
  recentThreads: Thread[];
  /** All conversations across projects, newest first. */
  globalThreads: GlobalThreadItem[];
  primaryAction: AgentHomePrimaryAction;
}

/**
 * Build home view model.
 *
 * - `recentThreads` remains workspace-scoped for backward-compatible callers.
 * - `globalThreads` is the product surface: every conversation across workspaces.
 */
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
      globalThreads: [],
      primaryAction: "create-project",
    };
  }

  const workspaceById = new Map(workspaces.map((workspace) => [workspace.id, workspace]));
  const activeWorkspace =
    workspaces.find((workspace) => workspace.id === selectedWorkspaceId) ?? workspaces[0];

  const recentThreads = [...threads]
    .filter((thread) => thread.workspace_id === activeWorkspace?.id)
    .sort((a, b) => Date.parse(b.updated_at) - Date.parse(a.updated_at));

  const globalThreads = [...threads]
    .map((thread) => {
      const workspace = workspaceById.get(thread.workspace_id as WorkspaceId);
      if (!workspace) return null;
      return { thread, workspace } satisfies GlobalThreadItem;
    })
    .filter((item): item is GlobalThreadItem => item !== null)
    .sort((a, b) => Date.parse(b.thread.updated_at) - Date.parse(a.thread.updated_at));

  return {
    mode: "workspace",
    activeWorkspace,
    recentThreads,
    globalThreads,
    primaryAction: globalThreads.length > 0 ? "continue-thread" : "create-thread",
  };
}
