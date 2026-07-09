import { invoke } from "@tauri-apps/api/core";
import type {
  AgentDefinition,
  AgentRun,
  AgentRunId,
  ApprovalRequestId,
  ArtifactPreview,
  AuditLogEntry,
  Automation,
  AutomationId,
  AutomationTrigger,
  BackgroundTask,
  BackgroundTaskId,
  BrowserAction,
  BrowserWindowId,
  BrowserWindowInfo,
  ContextSummary,
  DesktopCapture,
  DiagnosticsBundle,
  DiagnosticsBundleId,
  InstructionFile,
  McpServerConfig,
  McpToolInfo,
  MemoryId,
  MemoryItem,
  MemoryScope,
  MigrationInfo,
  ModelCapability,
  ModelId,
  ModelInfo,
  Notification,
  NotificationId,
  OrchestrationPlan,
  PermissionRequest,
  PermissionResult,
  PluginId,
  PluginManifest,
  ProviderConfig,
  ProviderId,
  ProviderKind,
  RagChunk,
  RuntimeEvent,
  RunEvent,
  Skill,
  SkillId,
  SubagentRun,
  TerminalId,
  TerminalOutput,
  Thread,
  ThreadId,
  UsageSummary,
  Worktree,
  WorktreeId,
  Workspace,
  WorkspaceId,
} from "@/lib/schemas";
import { runtimeEventSchema } from "@/lib/schemas";

/**
 * Generic API response envelope returned by Tauri commands.
 * Mirrors the backend ApiResponse<T> shape without exposing secrets.
 */
export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

/**
 * Invoke a Tauri command and unwrap the ApiResponse envelope.
 */
export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const response = await invoke<ApiResponse<T>>(command, args);
  if (!response.success) {
    throw new Error(response.error ?? `Command "${command}" failed`);
  }
  if (response.data === undefined) {
    throw new Error(`Command "${command}" returned no data`);
  }
  return response.data;
}

/**
 * Legacy smoke-test greeting call.
 */
export function greet(name: string): Promise<string> {
  return invokeCommand<string>("greet", { name });
}

// Workspace commands

export function createWorkspace(
  name: string,
  rootPath: string,
  trusted: boolean,
): Promise<Workspace> {
  return invokeCommand<Workspace>("create_workspace", { name, rootPath, trusted });
}

export function listWorkspaces(): Promise<Workspace[]> {
  return invokeCommand<Workspace[]>("list_workspaces");
}

export function trustWorkspace(id: WorkspaceId, trusted: boolean): Promise<Workspace> {
  return invokeCommand<Workspace>("trust_workspace", { id, trusted });
}

export function setWorkspacePaths(
  id: WorkspaceId,
  readPaths: string[],
  writePaths: string[],
): Promise<void> {
  return invokeCommand<void>("set_workspace_paths", {
    id,
    read_paths: readPaths,
    write_paths: writePaths,
  });
}

// Thread commands

export function createThread(workspaceId: WorkspaceId, title: string): Promise<Thread> {
  return invokeCommand<Thread>("create_thread", { workspace_id: workspaceId, title });
}

export function listThreads(workspaceId: WorkspaceId): Promise<Thread[]> {
  return invokeCommand<Thread[]>("list_threads", { workspace_id: workspaceId });
}

// Worktree commands

export function createWorktree(
  workspaceId: WorkspaceId,
  threadId: ThreadId,
  name: string,
): Promise<Worktree> {
  return invokeCommand<Worktree>("create_worktree", {
    workspace_id: workspaceId,
    thread_id: threadId,
    name,
  });
}

export function deleteWorktree(id: WorktreeId): Promise<void> {
  return invokeCommand<void>("delete_worktree", { id });
}

export function listWorktrees(workspaceId: WorkspaceId): Promise<Worktree[]> {
  return invokeCommand<Worktree[]>("list_worktrees", { workspace_id: workspaceId });
}

// Git commands

export function gitStatus(workspaceId: WorkspaceId, repoPath: string): Promise<string> {
  return invokeCommand<string>("git_status", { workspace_id: workspaceId, repo_path: repoPath });
}

export function gitDiff(workspaceId: WorkspaceId, repoPath: string): Promise<string> {
  return invokeCommand<string>("git_diff", { workspace_id: workspaceId, repo_path: repoPath });
}

export function gitStage(
  workspaceId: WorkspaceId,
  repoPath: string,
  paths: string[],
): Promise<void> {
  return invokeCommand<void>("git_stage", {
    workspace_id: workspaceId,
    repo_path: repoPath,
    paths,
  });
}

export function gitUnstage(
  workspaceId: WorkspaceId,
  repoPath: string,
  paths: string[],
): Promise<void> {
  return invokeCommand<void>("git_unstage", {
    workspace_id: workspaceId,
    repo_path: repoPath,
    paths,
  });
}

export function gitCommit(
  workspaceId: WorkspaceId,
  repoPath: string,
  message: string,
): Promise<string> {
  return invokeCommand<string>("git_commit", {
    workspace_id: workspaceId,
    repo_path: repoPath,
    message,
  });
}

export function gitBranch(
  workspaceId: WorkspaceId,
  repoPath: string,
  name?: string,
): Promise<string> {
  return invokeCommand<string>("git_branch", {
    workspace_id: workspaceId,
    repo_path: repoPath,
    name: name ?? null,
  });
}

export function gitPush(
  workspaceId: WorkspaceId,
  repoPath: string,
  remote?: string,
): Promise<string> {
  return invokeCommand<string>("git_push", {
    workspace_id: workspaceId,
    repo_path: repoPath,
    remote: remote ?? null,
  });
}

// Terminal commands

export function createTerminal(threadId: ThreadId): Promise<TerminalId> {
  return invokeCommand<TerminalId>("create_terminal", { thread_id: threadId });
}

export function executeTerminalCommand(
  id: TerminalId,
  command: string,
  cwd: string,
): Promise<TerminalOutput> {
  return invokeCommand<TerminalOutput>("execute_terminal_command", { id, command, cwd });
}

export function readTerminalHistory(id: TerminalId): Promise<string[]> {
  return invokeCommand<string[]>("read_terminal_history", { id });
}

// Run commands

export function startRun(workspaceId: WorkspaceId, threadId: ThreadId): Promise<AgentRun> {
  return invokeCommand<AgentRun>("start_run", { workspace_id: workspaceId, thread_id: threadId });
}

export function submitMessage(runId: AgentRunId, content: string): Promise<void> {
  return invokeCommand<void>("submit_message", { run_id: runId, content });
}

export function cancelRun(runId: AgentRunId): Promise<void> {
  return invokeCommand<void>("cancel_run", { run_id: runId });
}

export function pauseRun(runId: AgentRunId): Promise<void> {
  return invokeCommand<void>("pause_run", { run_id: runId });
}

export function resumeRun(runId: AgentRunId): Promise<void> {
  return invokeCommand<void>("resume_run", { run_id: runId });
}

export function listRunEvents(runId: AgentRunId): Promise<RunEvent[]> {
  return invokeCommand<RunEvent[]>("list_run_events", { run_id: runId });
}

// Event helpers

export function parseRuntimeEvent(payload: unknown): RuntimeEvent | undefined {
  const result = runtimeEventSchema.safeParse(payload);
  return result.success ? result.data : undefined;
}

// Provider/model registry commands

export function listProviders(): Promise<ProviderConfig[]> {
  return invokeCommand<ProviderConfig[]>("list_providers");
}

export function createProvider(
  kind: ProviderKind,
  displayName: string,
  baseUrl: string | null,
  apiKeyReference: string,
): Promise<ProviderConfig> {
  return invokeCommand<ProviderConfig>("create_provider", {
    kind,
    display_name: displayName,
    base_url: baseUrl,
    api_key_reference: apiKeyReference,
  });
}

export function updateProvider(config: ProviderConfig): Promise<void> {
  return invokeCommand<void>("update_provider", { config });
}

export function deleteProvider(id: ProviderId): Promise<void> {
  return invokeCommand<void>("delete_provider", { id });
}

export function setProviderSecret(
  apiKeyReference: string,
  apiKey: string,
): Promise<void> {
  return invokeCommand<void>("set_provider_secret", { apiKeyReference, apiKey });
}

export function deleteProviderSecret(apiKeyReference: string): Promise<void> {
  return invokeCommand<void>("delete_provider_secret", { apiKeyReference });
}

export function listModels(providerId?: ProviderId): Promise<ModelInfo[]> {
  return invokeCommand<ModelInfo[]>("list_models", { provider_id: providerId ?? null });
}

export function createModel(
  providerId: ProviderId,
  modelName: string,
  displayName: string,
  capabilities: ModelCapability,
): Promise<ModelInfo> {
  return invokeCommand<ModelInfo>("create_model", {
    provider_id: providerId,
    model_name: modelName,
    display_name: displayName,
    capabilities,
  });
}

export function deleteModel(id: ModelId): Promise<void> {
  return invokeCommand<void>("delete_model", { id });
}

export function getUsageSummary(): Promise<UsageSummary> {
  return invokeCommand<UsageSummary>("get_usage_summary");
}

// Security commands

export function evaluatePermission(request: PermissionRequest): Promise<PermissionResult> {
  return invokeCommand<PermissionResult>("evaluate_permission", { request });
}

export function listAuditLog(
  workspaceId?: WorkspaceId | null,
  threadId?: ThreadId | null,
  runId?: AgentRunId | null,
): Promise<AuditLogEntry[]> {
  return invokeCommand<AuditLogEntry[]>("list_audit_log", {
    workspace_id: workspaceId ?? null,
    thread_id: threadId ?? null,
    run_id: runId ?? null,
  });
}

export function approveRequest(id: ApprovalRequestId): Promise<void> {
  return invokeCommand<void>("approve_request", { id });
}

export function denyRequest(id: ApprovalRequestId, reason?: string | null): Promise<void> {
  return invokeCommand<void>("deny_request", { id, reason: reason ?? null });
}

// Memory commands

export function listMemories(
  scope: MemoryScope,
  workspaceId?: WorkspaceId | null,
  threadId?: ThreadId | null,
): Promise<MemoryItem[]> {
  return invokeCommand<MemoryItem[]>("list_memories", {
    scope,
    workspace_id: workspaceId ?? null,
    thread_id: threadId ?? null,
  });
}

export function createMemory(
  scope: MemoryScope,
  workspaceId: WorkspaceId | null,
  threadId: ThreadId | null,
  key: string,
  value: string,
  sensitive: boolean,
): Promise<MemoryItem> {
  return invokeCommand<MemoryItem>("create_memory", {
    scope,
    workspace_id: workspaceId,
    thread_id: threadId,
    key,
    value,
    sensitive,
  });
}

export function updateMemory(id: MemoryId, value: string): Promise<MemoryItem> {
  return invokeCommand<MemoryItem>("update_memory", { id, value });
}

export function deleteMemory(id: MemoryId): Promise<void> {
  return invokeCommand<void>("delete_memory", { id });
}

// Instruction commands

export function loadInstructions(workspaceRoot: string): Promise<InstructionFile[]> {
  return invokeCommand<InstructionFile[]>("load_instructions", { workspace_root: workspaceRoot });
}

// Context Inspector command

export function inspectContext(
  runId: AgentRunId,
  threadId: ThreadId,
  workspaceId: WorkspaceId,
  workspaceRoot: string,
  query: string,
): Promise<ContextSummary> {
  return invokeCommand<ContextSummary>("inspect_context", {
    run_id: runId,
    thread_id: threadId,
    workspace_id: workspaceId,
    workspace_root: workspaceRoot,
    query,
  });
}

// RAG commands

export function searchRag(workspaceId: WorkspaceId, query: string, topN = 5): Promise<RagChunk[]> {
  return invokeCommand<RagChunk[]>("search_rag", {
    workspace_id: workspaceId,
    query,
    top_n: topN,
  });
}

export function rebuildRagIndex(workspaceId: WorkspaceId): Promise<void> {
  return invokeCommand<void>("rebuild_rag_index", { workspace_id: workspaceId });
}

// Plugin commands

export function installPlugin(manifest: PluginManifest): Promise<PluginManifest> {
  return invokeCommand<PluginManifest>("install_plugin", { manifest });
}

export function listPlugins(): Promise<PluginManifest[]> {
  return invokeCommand<PluginManifest[]>("list_plugins");
}

export function enablePlugin(id: PluginId, enabled: boolean): Promise<void> {
  return invokeCommand<void>("enable_plugin", { id, enabled });
}

export function uninstallPlugin(id: PluginId): Promise<void> {
  return invokeCommand<void>("uninstall_plugin", { id });
}

// Skill commands

export function listSkills(pluginId?: PluginId): Promise<Skill[]> {
  return invokeCommand<Skill[]>("list_skills", { plugin_id: pluginId ?? null });
}

export function invokeSkill(
  skillId: SkillId,
  arguments_?: Record<string, unknown>,
): Promise<unknown> {
  return invokeCommand<unknown>("invoke_skill", { skill_id: skillId, arguments: arguments_ ?? {} });
}

// MCP commands

export function listMcpServers(): Promise<McpServerConfig[]> {
  return invokeCommand<McpServerConfig[]>("list_mcp_servers");
}

export function addMcpServer(config: Omit<McpServerConfig, "id">): Promise<McpServerConfig> {
  return invokeCommand<McpServerConfig>("add_mcp_server", { config });
}

export function removeMcpServer(id: number): Promise<void> {
  return invokeCommand<void>("remove_mcp_server", { id });
}

export function listMcpTools(serverId?: number): Promise<McpToolInfo[]> {
  return invokeCommand<McpToolInfo[]>("list_mcp_tools", { server_id: serverId ?? null });
}

export function invokeMcpTool(
  workspaceId: WorkspaceId,
  toolName: string,
  arguments_: Record<string, unknown>,
  serverId?: number,
): Promise<unknown> {
  return invokeCommand<unknown>("invoke_mcp_tool", {
    workspace_id: workspaceId,
    server_id: serverId ?? null,
    name: toolName,
    arguments: arguments_,
  });
}

export function refreshMcpTools(): Promise<void> {
  return invokeCommand<void>("refresh_mcp_tools");
}

// Orchestrator commands

export function listAgents(): Promise<AgentDefinition[]> {
  return invokeCommand<AgentDefinition[]>("list_agents");
}

export function planSubagents(parentRunId: AgentRunId, task: string): Promise<OrchestrationPlan> {
  return invokeCommand<OrchestrationPlan>("plan_subagents", { parent_run_id: parentRunId, task });
}

export function executeSubagents(plan: OrchestrationPlan): Promise<SubagentRun[]> {
  return invokeCommand<SubagentRun[]>("execute_subagents", { plan });
}

export function cancelSubagent(subagentRunId: AgentRunId): Promise<void> {
  return invokeCommand<void>("cancel_subagent", { subagent_run_id: subagentRunId });
}

// Automation commands

export function listAutomations(workspaceId?: WorkspaceId | null): Promise<Automation[]> {
  return invokeCommand<Automation[]>("list_automations", { workspace_id: workspaceId ?? null });
}

export function createAutomation(
  workspaceId: WorkspaceId,
  name: string,
  description: string,
  trigger: AutomationTrigger,
  cronExpr: string | null,
  enabled: boolean,
): Promise<Automation> {
  return invokeCommand<Automation>("create_automation", {
    workspace_id: workspaceId,
    name,
    description,
    trigger,
    cron_expr: cronExpr,
    enabled,
  });
}

export function updateAutomation(automation: Automation): Promise<Automation> {
  return invokeCommand<Automation>("update_automation", { automation });
}

export function deleteAutomation(id: AutomationId): Promise<void> {
  return invokeCommand<void>("delete_automation", { id });
}

export function runAutomationNow(id: AutomationId): Promise<void> {
  return invokeCommand<void>("run_automation_now", { id });
}

// Notification commands

export function listNotifications(
  workspaceId?: WorkspaceId | null,
  unreadOnly?: boolean,
): Promise<Notification[]> {
  return invokeCommand<Notification[]>("list_notifications", {
    workspace_id: workspaceId ?? null,
    unread_only: unreadOnly ?? false,
  });
}

export function markNotificationRead(id: NotificationId): Promise<Notification> {
  return invokeCommand<Notification>("mark_notification_read", { id });
}

export function dismissNotification(id: NotificationId): Promise<void> {
  return invokeCommand<void>("dismiss_notification", { id });
}

export function sendTestNotification(title: string, body: string): Promise<Notification> {
  return invokeCommand<Notification>("send_test_notification", { title, body });
}

// Background task commands

export function listBackgroundTasks(workspaceId?: WorkspaceId | null): Promise<BackgroundTask[]> {
  return invokeCommand<BackgroundTask[]>("list_background_tasks", {
    workspace_id: workspaceId ?? null,
  });
}

export function cancelBackgroundTask(id: BackgroundTaskId): Promise<void> {
  return invokeCommand<void>("cancel_background_task", { id });
}

// Browser automation commands

export function openBrowserWindow(
  workspaceId: WorkspaceId,
  url: string,
  title?: string,
): Promise<BrowserWindowInfo> {
  return invokeCommand<BrowserWindowInfo>("open_browser_window", {
    workspace_id: workspaceId,
    url,
    title: title ?? null,
  });
}

export function closeBrowserWindow(workspaceId: WorkspaceId, id: BrowserWindowId): Promise<void> {
  return invokeCommand<void>("close_browser_window", { workspace_id: workspaceId, id });
}

export function listBrowserWindows(): Promise<BrowserWindowInfo[]> {
  return invokeCommand<BrowserWindowInfo[]>("list_browser_windows");
}

export function browserUseAction(
  workspaceId: WorkspaceId,
  id: BrowserWindowId,
  action: BrowserAction,
): Promise<string> {
  return invokeCommand<string>("browser_use_action", { workspace_id: workspaceId, id, action });
}

// Desktop automation commands

export function captureScreen(workspaceId: WorkspaceId): Promise<DesktopCapture> {
  return invokeCommand<DesktopCapture>("capture_screen", { workspace_id: workspaceId });
}

export function moveMouse(workspaceId: WorkspaceId, x: number, y: number): Promise<void> {
  return invokeCommand<void>("move_mouse", { workspace_id: workspaceId, x, y });
}

export function clickMouse(workspaceId: WorkspaceId): Promise<void> {
  return invokeCommand<void>("click_mouse", { workspace_id: workspaceId });
}

export function typeText(workspaceId: WorkspaceId, text: string): Promise<void> {
  return invokeCommand<void>("type_text", { workspace_id: workspaceId, text });
}

export function focusApp(workspaceId: WorkspaceId, name: string): Promise<void> {
  return invokeCommand<void>("focus_app", { workspace_id: workspaceId, name });
}

// Artifact preview command

export function previewArtifact(workspaceId: WorkspaceId, path: string): Promise<ArtifactPreview> {
  return invokeCommand<ArtifactPreview>("preview_artifact", { workspace_id: workspaceId, path });
}

// Diagnostics / migration / updater commands

export function collectDiagnosticsBundle(): Promise<DiagnosticsBundle> {
  return invokeCommand<DiagnosticsBundle>("collect_diagnostics_bundle");
}

export function uploadDiagnosticsBundle(bundleId: DiagnosticsBundleId): Promise<void> {
  return invokeCommand<void>("upload_diagnostics_bundle", { bundle_id: bundleId });
}

export function listMigrations(): Promise<MigrationInfo[]> {
  return invokeCommand<MigrationInfo[]>("list_migrations");
}

export function rollbackLastMigration(): Promise<void> {
  return invokeCommand<void>("rollback_last_migration");
}
