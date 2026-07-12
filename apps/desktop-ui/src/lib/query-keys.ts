import type { AgentRunId, ProviderId, TerminalId, ThreadId, WorkspaceId } from "@/lib/schemas";

/**
 * Centralized React Query key factory.
 *
 * Every resource has exactly one key shape. Use these factories for both
 * `useQuery` and `invalidateQueries` so caches stay consistent.
 */

export const workspaceKeys = {
  list: () => ["workspaces"] as const,
  threads: (workspaceId: WorkspaceId) => ["workspaces", workspaceId, "threads"] as const,
  worktrees: (workspaceId: WorkspaceId) => ["workspaces", workspaceId, "worktrees"] as const,
  memories: (workspaceId: WorkspaceId) => ["workspaces", workspaceId, "memories"] as const,
  /** Prefix for all directory listings of a workspace (any relative path). */
  files: (workspaceId: WorkspaceId) => ["workspace-files", workspaceId] as const,
  filesAt: (workspaceId: WorkspaceId, relativePath = "") =>
    ["workspace-files", workspaceId, relativePath] as const,
  gitStatus: (workspaceId: WorkspaceId, repoPath = "") =>
    ["workspaces", workspaceId, "git", "status", repoPath] as const,
  gitDiff: (workspaceId: WorkspaceId, repoPath = "") =>
    ["workspaces", workspaceId, "git", "diff", repoPath] as const,
};

export const threadKeys = {
  list: (workspaceId: WorkspaceId) => workspaceKeys.threads(workspaceId),
};

export const runKeys = {
  events: (workspaceId: WorkspaceId, threadId: ThreadId, runId?: AgentRunId) =>
    ["workspaces", workspaceId, "threads", threadId, "runs", runId ?? "none", "events"] as const,
  context: (workspaceId: WorkspaceId, threadId: ThreadId, runId?: AgentRunId, query = "") =>
    [
      "workspaces",
      workspaceId,
      "threads",
      threadId,
      "runs",
      runId ?? "none",
      "context",
      query,
    ] as const,
};

export const eventKeys = runKeys;

export const gitKeys = {
  status: (workspaceId: WorkspaceId, repoPath = "") =>
    workspaceKeys.gitStatus(workspaceId, repoPath),
  diff: (workspaceId: WorkspaceId, repoPath = "") => workspaceKeys.gitDiff(workspaceId, repoPath),
};

export const terminalKeys = {
  history: (terminalId: TerminalId | undefined) => ["terminals", terminalId, "history"] as const,
};

export const browserWindowKeys = {
  list: () => ["browser-windows"] as const,
};

export const desktopKeys = {
  capture: (workspaceId: WorkspaceId) => ["desktop-capture", workspaceId] as const,
};

export const notificationKeys = {
  list: (workspaceIdFilter: WorkspaceId | null = null, unreadOnly = false) =>
    ["notifications", workspaceIdFilter, unreadOnly] as const,
};

export const backgroundTaskKeys = {
  list: (workspaceIdFilter: WorkspaceId | null = null) =>
    ["background-tasks", workspaceIdFilter] as const,
};

export const automationKeys = {
  list: (workspaceId: WorkspaceId | null = null) => ["automations", workspaceId] as const,
};

export const pluginKeys = {
  list: () => ["plugins"] as const,
  available: () => ["plugins", "available"] as const,
  userDirs: () => ["plugins", "user-dirs"] as const,
};

export const mcpKeys = {
  list: () => ["mcp-servers"] as const,
};

export const skillKeys = {
  list: () => ["skills"] as const,
};

export const providerKeys = {
  list: () => ["providers"] as const,
};

export const modelKeys = {
  list: (providerId?: ProviderId | null) =>
    providerId ? (["models", providerId] as const) : (["models"] as const),
};

export const usageKeys = {
  summary: () => ["usage-summary"] as const,
};

export const migrationKeys = {
  list: () => ["migrations"] as const,
};

export const auditKeys = {
  log: (workspaceId: WorkspaceId | null, threadId: ThreadId | null, runId: AgentRunId | null) =>
    ["audit-log", workspaceId, threadId, runId] as const,
};

export const queryKeys = {
  workspaces: workspaceKeys,
  threads: threadKeys,
  runs: runKeys,
  events: eventKeys,
  git: gitKeys,
  terminals: terminalKeys,
  browserWindows: browserWindowKeys,
  desktop: desktopKeys,
  notifications: notificationKeys,
  backgroundTasks: backgroundTaskKeys,
  automations: automationKeys,
  plugins: pluginKeys,
  mcpServers: mcpKeys,
  skills: skillKeys,
  providers: providerKeys,
  models: modelKeys,
  usage: usageKeys,
  migrations: migrationKeys,
  audit: auditKeys,
};
