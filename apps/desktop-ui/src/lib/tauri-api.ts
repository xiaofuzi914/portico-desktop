import { invoke } from "@tauri-apps/api/core";
import type {
  ActiveModelSelection,
  AgentDefinition,
  AgentRun,
  AgentRunId,
  ApprovalRequest,
  ApprovalRequestId,
  ArtifactPreview,
  AuditLogEntry,
  Automation,
  AutomationId,
  AutomationTrigger,
  Canvas,
  CanvasEdge,
  CanvasEdgeId,
  CanvasId,
  CanvasLink,
  CanvasLinkId,
  CanvasNode,
  CanvasNodeId,
  CanvasSnapshot,
  BackgroundTask,
  BackgroundTaskId,
  BrowserAction,
  BrowserWindowId,
  BrowserWindowInfo,
  ContextSummary,
  DesktopCapture,
  DiagnosticsBundle,
  InstructionFile,
  McpServerConfig,
  McpToolInfo,
  MemoryId,
  MemoryItem,
  MemoryScope,
  Message,
  MigrationInfo,
  ModelCapability,
  ModelId,
  ModelInfo,
  ModelSelectionScope,
  Notification,
  NotificationId,
  Orchestration,
  OrchestrationId,
  OrchestrationPlan,
  PatternHint,
  PluginId,
  PluginManifest,
  ProviderConfig,
  ProviderHealth,
  ProviderId,
  ProviderKind,
  RagChunk,
  RuntimeEvent,
  RunEvent,
  RunModelSnapshot,
  Skill,
  TerminalId,
  TerminalOutput,
  Thread,
  ThreadId,
  WorkflowPattern,
  WorkflowPatternId,
  WorkflowTemplate,
  WorkflowTemplateId,
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
  // Unit responses serialize as `data: null` (Rust `()`); treat as success.
  if (response.data === undefined || response.data === null) {
    return undefined as T;
  }
  return response.data;
}

// Workspace commands

export function createWorkspace(name: string, rootPath: string): Promise<Workspace> {
  return invokeCommand<Workspace>("create_workspace", { name, rootPath });
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
    readPaths,
    writePaths,
  });
}

/** Attach an extra project folder (read+write) next to the main engineering root. */
export function addLinkedProjectFolder(id: WorkspaceId, path: string): Promise<Workspace> {
  return invokeCommand<Workspace>("add_linked_project_folder", { id, path });
}

/** Detach a linked folder (cannot remove the main root). */
export function removeLinkedProjectFolder(id: WorkspaceId, path: string): Promise<Workspace> {
  return invokeCommand<Workspace>("remove_linked_project_folder", { id, path });
}

export function getWorkspace(id: WorkspaceId): Promise<Workspace> {
  return invokeCommand<Workspace>("get_workspace", { id });
}

/**
 * Remove a project from Portico. Does not delete files on disk.
 */
export function deleteWorkspace(id: WorkspaceId): Promise<void> {
  return invokeCommand<void>("delete_workspace", { id });
}

export interface WorkspaceFileEntry {
  name: string;
  relative_path: string;
  is_directory: boolean;
}

export function listWorkspaceFiles(
  id: WorkspaceId,
  relativePath = "",
  basePath?: string | null,
): Promise<WorkspaceFileEntry[]> {
  return invokeCommand<WorkspaceFileEntry[]>("list_workspace_files", {
    id,
    relativePath,
    basePath: basePath ?? null,
  });
}

/** Open a workspace directory in Finder / Explorer / the system file manager. */
export function openWorkspaceFolder(
  id: WorkspaceId,
  relativePath = "",
  basePath?: string | null,
): Promise<void> {
  return invokeCommand<void>("open_workspace_folder", {
    id,
    relativePath,
    basePath: basePath ?? null,
  });
}

export function previewWorkspaceMarkdown(
  id: WorkspaceId,
  relativePath: string,
  basePath?: string | null,
): Promise<ArtifactPreview> {
  return invokeCommand<ArtifactPreview>("preview_workspace_markdown", {
    id,
    relativePath,
    basePath: basePath ?? null,
  });
}

// Thread commands

export function createThread(workspaceId: WorkspaceId, title: string): Promise<Thread> {
  return invokeCommand<Thread>("create_thread", { workspaceId, title });
}

/**
 * Branch a new session from an existing one with parent context seeded.
 * Pass `focusText` (划词) to prioritise that snippet in the seed + title.
 * The new session appears as a child edge on the project mind map after sync.
 */
export function branchThreadFromContext(
  workspaceId: WorkspaceId,
  parentThreadId: ThreadId,
  title?: string | null,
  focusText?: string | null,
): Promise<Thread> {
  return invokeCommand<Thread>("branch_thread_from_context", {
    workspaceId,
    parentThreadId,
    title: title ?? null,
    focusText: focusText ?? null,
  });
}

/** Active (non-archived) sessions. Also triggers 30-day archive purge. */
export function listThreads(workspaceId: WorkspaceId): Promise<Thread[]> {
  return invokeCommand<Thread[]>("list_threads", { workspaceId });
}

export function listArchivedThreads(workspaceId: WorkspaceId): Promise<Thread[]> {
  return invokeCommand<Thread[]>("list_archived_threads", { workspaceId });
}

export function getThread(id: ThreadId): Promise<Thread> {
  return invokeCommand<Thread>("get_thread", { id });
}

export function updateThreadTitle(id: ThreadId, title: string): Promise<Thread> {
  return invokeCommand<Thread>("update_thread_title", { id, title });
}

/** Soft-delete: move session to archive (retained 30 days). */
export function archiveThread(workspaceId: WorkspaceId, id: ThreadId): Promise<Thread> {
  return invokeCommand<Thread>("archive_thread", { workspaceId, id });
}

export function restoreThread(workspaceId: WorkspaceId, id: ThreadId): Promise<Thread> {
  return invokeCommand<Thread>("restore_thread", { workspaceId, id });
}

/** Permanent delete (prefer archiveThread for normal UI delete). */
export function deleteThread(workspaceId: WorkspaceId, id: ThreadId): Promise<void> {
  return invokeCommand<void>("delete_thread", { workspaceId, id });
}

// Worktree commands

export function createWorktree(
  workspaceId: WorkspaceId,
  threadId: ThreadId,
  name: string,
): Promise<Worktree> {
  return invokeCommand<Worktree>("create_worktree", {
    workspaceId,
    threadId,
    name,
  });
}

export function deleteWorktree(id: WorktreeId): Promise<void> {
  return invokeCommand<void>("delete_worktree", { id });
}

export function listWorktrees(workspaceId: WorkspaceId): Promise<Worktree[]> {
  return invokeCommand<Worktree[]>("list_worktrees", { workspaceId });
}

// Git commands

export function gitStatus(workspaceId: WorkspaceId, repoPath: string): Promise<string> {
  return invokeCommand<string>("git_status", { workspaceId, repoPath });
}

export function gitDiff(workspaceId: WorkspaceId, repoPath: string): Promise<string> {
  return invokeCommand<string>("git_diff", { workspaceId, repoPath });
}

export function gitStage(
  workspaceId: WorkspaceId,
  repoPath: string,
  paths: string[],
): Promise<void> {
  return invokeCommand<void>("git_stage", {
    workspaceId,
    repoPath,
    paths,
  });
}

export function gitUnstage(
  workspaceId: WorkspaceId,
  repoPath: string,
  paths: string[],
): Promise<void> {
  return invokeCommand<void>("git_unstage", {
    workspaceId,
    repoPath,
    paths,
  });
}

export function gitCommit(
  workspaceId: WorkspaceId,
  repoPath: string,
  message: string,
): Promise<string> {
  return invokeCommand<string>("git_commit", {
    workspaceId,
    repoPath,
    message,
  });
}

export function gitBranch(
  workspaceId: WorkspaceId,
  repoPath: string,
  name?: string,
): Promise<string> {
  return invokeCommand<string>("git_branch", {
    workspaceId,
    repoPath,
    name: name ?? null,
  });
}

export function gitPush(
  workspaceId: WorkspaceId,
  repoPath: string,
  remote?: string,
): Promise<string> {
  return invokeCommand<string>("git_push", {
    workspaceId,
    repoPath,
    remote: remote ?? null,
  });
}

// Terminal commands

export function createTerminal(threadId: ThreadId): Promise<TerminalId> {
  return invokeCommand<TerminalId>("create_terminal", { threadId });
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
  return invokeCommand<AgentRun>("start_run", { workspaceId, threadId });
}

export function submitMessage(runId: AgentRunId, content: string): Promise<void> {
  return invokeCommand<void>("submit_message", { runId, content });
}

export type SendMessageOptions = {
  clientRequestId?: string;
  /** DeepSeek-style: off | on | auto */
  thinkingMode?: "off" | "on" | "auto" | null;
  /** OpenAI/Codex-style: low | medium | high */
  reasoningEffort?: "low" | "medium" | "high" | null;
};

export function sendMessage(
  threadId: ThreadId,
  content: string,
  clientRequestIdOrOptions?: string | SendMessageOptions,
): Promise<AgentRun> {
  const options: SendMessageOptions =
    typeof clientRequestIdOrOptions === "string" || clientRequestIdOrOptions === undefined
      ? { clientRequestId: clientRequestIdOrOptions }
      : clientRequestIdOrOptions;
  return invokeCommand<AgentRun>("send_message", {
    threadId,
    content,
    clientRequestId: options.clientRequestId ?? null,
    thinkingMode: options.thinkingMode ?? null,
    reasoningEffort: options.reasoningEffort ?? null,
  });
}

export function listMessages(threadId: ThreadId): Promise<Message[]> {
  return invokeCommand<Message[]>("list_messages", { threadId });
}

export function listRuns(threadId: ThreadId): Promise<AgentRun[]> {
  return invokeCommand<AgentRun[]>("list_runs", { threadId });
}

export function cancelRun(runId: AgentRunId): Promise<void> {
  return invokeCommand<void>("cancel_run", { runId });
}

export function pauseRun(runId: AgentRunId): Promise<void> {
  return invokeCommand<void>("pause_run", { runId });
}

export function resumeRun(runId: AgentRunId): Promise<void> {
  return invokeCommand<void>("resume_run", { runId });
}

export function listRunEvents(runId: AgentRunId): Promise<RunEvent[]> {
  return invokeCommand<RunEvent[]>("list_run_events", { runId });
}

export type RunTokenUsage = Readonly<{
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
}>;

export function getRunTokenUsage(runId: AgentRunId): Promise<RunTokenUsage> {
  return invokeCommand<RunTokenUsage>("get_run_token_usage", { runId });
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
    displayName,
    baseUrl,
    apiKeyReference,
  });
}

export function updateProvider(config: ProviderConfig): Promise<void> {
  return invokeCommand<void>("update_provider", { config });
}

export function deleteProvider(id: ProviderId): Promise<void> {
  return invokeCommand<void>("delete_provider", { id });
}

export function setProviderSecret(apiKeyReference: string, apiKey: string): Promise<void> {
  return invokeCommand<void>("set_provider_secret", { apiKeyReference, apiKey });
}

/** Detected local Codex / Kimi / Grok CLI login (no secrets). */
export type CliAuthSource = {
  id: string;
  kind: ProviderKind;
  label: string;
  path: string;
  auth_mode: string;
  available: boolean;
  preview: string;
  hint: string | null;
};

/** Scan ~/.codex, ~/.grok, ~/.kimi-code for session credentials. */
export function listCliAuthSources(): Promise<CliAuthSource[]> {
  return invokeCommand<CliAuthSource[]>("list_cli_auth_sources_cmd");
}

/**
 * Import a CLI session into Portico (stores access token, creates provider + models).
 * Matches Codex / Kimi Code / Grok Build login state — not platform API-key paste.
 */
export function importCliAuthSource(sourceId: string): Promise<ProviderConfig> {
  return invokeCommand<ProviderConfig>("import_cli_auth_source_cmd", { sourceId });
}

export function deleteProviderSecret(apiKeyReference: string): Promise<void> {
  return invokeCommand<void>("delete_provider_secret", { apiKeyReference });
}

export function listModels(providerId?: ProviderId): Promise<ModelInfo[]> {
  return invokeCommand<ModelInfo[]>("list_models", { providerId: providerId ?? null });
}

export function createModel(
  providerId: ProviderId,
  modelName: string,
  displayName: string,
  capabilities: ModelCapability,
): Promise<ModelInfo> {
  return invokeCommand<ModelInfo>("create_model", {
    providerId,
    modelName,
    displayName,
    capabilities,
  });
}

export function deleteModel(id: ModelId): Promise<void> {
  return invokeCommand<void>("delete_model", { id });
}

export function setActiveModel(
  scope: ModelSelectionScope,
  workspaceId: WorkspaceId | null,
  threadId: ThreadId | null,
  providerId: ProviderId,
  modelId: ModelId,
): Promise<ActiveModelSelection> {
  return invokeCommand<ActiveModelSelection>("set_active_model", {
    scope,
    workspaceId,
    threadId,
    providerId,
    modelId,
  });
}

export function getActiveModel(
  scope: ModelSelectionScope,
  workspaceId: WorkspaceId | null = null,
  threadId: ThreadId | null = null,
): Promise<ActiveModelSelection | null> {
  return invokeCommand<ActiveModelSelection | null>("get_active_model", {
    scope,
    workspaceId,
    threadId,
  });
}

export function resolveActiveModel(
  workspaceId: WorkspaceId,
  threadId: ThreadId,
): Promise<ActiveModelSelection> {
  return invokeCommand<ActiveModelSelection>("resolve_active_model", { workspaceId, threadId });
}

export function testProviderConnection(
  providerId: ProviderId,
  modelId: ModelId,
): Promise<ProviderHealth> {
  return invokeCommand<ProviderHealth>("test_provider_connection", { providerId, modelId });
}

export function getProviderHealth(
  providerId: ProviderId,
  modelId: ModelId,
): Promise<ProviderHealth | null> {
  return invokeCommand<ProviderHealth | null>("get_provider_health", { providerId, modelId });
}

export function getRunModelSnapshot(runId: AgentRunId): Promise<RunModelSnapshot | null> {
  return invokeCommand<RunModelSnapshot | null>("get_run_model_snapshot", { runId });
}

// Security commands

export function listAuditLog(
  workspaceId?: WorkspaceId | null,
  threadId?: ThreadId | null,
  runId?: AgentRunId | null,
): Promise<AuditLogEntry[]> {
  return invokeCommand<AuditLogEntry[]>("list_audit_log", {
    workspaceId: workspaceId ?? null,
    threadId: threadId ?? null,
    runId: runId ?? null,
  });
}

export function approveRequest(id: ApprovalRequestId): Promise<void> {
  return invokeCommand<void>("approve_request", { id });
}

export function listPendingApprovals(runId?: AgentRunId | null): Promise<ApprovalRequest[]> {
  return invokeCommand<ApprovalRequest[]>("list_pending_approvals", {
    runId: runId ?? null,
  });
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
    workspaceId: workspaceId ?? null,
    threadId: threadId ?? null,
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
    workspaceId,
    threadId,
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

export function loadInstructions(workspaceId: WorkspaceId): Promise<InstructionFile[]> {
  return invokeCommand<InstructionFile[]>("load_instructions", { workspaceId });
}

// Context Inspector command

export function inspectContext(
  runId: AgentRunId,
  threadId: ThreadId,
  workspaceId: WorkspaceId,
  query: string,
): Promise<ContextSummary> {
  return invokeCommand<ContextSummary>("inspect_context", {
    runId,
    threadId,
    workspaceId,
    query,
  });
}

// RAG commands

export function searchRag(workspaceId: WorkspaceId, query: string, topN = 5): Promise<RagChunk[]> {
  return invokeCommand<RagChunk[]>("search_rag", {
    workspaceId,
    query,
    topN,
  });
}

/** Rebuild RAG from disk scan; returns number of documents indexed. */
export function rebuildRagIndex(workspaceId: WorkspaceId): Promise<number> {
  return invokeCommand<number>("rebuild_rag_index", { workspaceId });
}

export interface FeatureCapabilities {
  core_agent_workflow: boolean;
  multi_agent_orchestration: boolean;
  context_injection: boolean;
  workspace_indexer: boolean;
  advanced_file_tools: boolean;
  skill_invocation: boolean;
  native_automation: boolean;
  mcp_agent_tools: boolean;
  terminal: boolean;
  git_mutation: boolean;
  automations: boolean;
  notes: string[];
}

/** Backend-authoritative capability matrix. */
export function getFeatureCapabilities(): Promise<FeatureCapabilities> {
  return invokeCommand<FeatureCapabilities>("get_feature_capabilities");
}

// Plugin commands

export interface PorticoUserDirs {
  home: string;
  plugins: string;
  plugins_installed: string;
  plugins_available: string;
}

export interface AvailablePluginPackage {
  folder_name: string;
  path: string;
  display_name: string;
  plugin_name: string;
  version: string;
  description: string;
  plugin_id: string;
}

export function installPlugin(manifest: PluginManifest): Promise<PluginManifest> {
  return invokeCommand<PluginManifest>("install_plugin", { manifest });
}

export function installPluginPackage(sourcePath: string): Promise<PluginManifest> {
  return invokeCommand<PluginManifest>("install_plugin_package", { sourcePath });
}

/** Install from `~/.portico/plugins/available/{folderName}`. */
export function installAvailablePluginPackage(folderName: string): Promise<PluginManifest> {
  return invokeCommand<PluginManifest>("install_available_plugin_package", { folderName });
}

/** One-click install a bundled catalog package (no folder picker). */
export function installCatalogPlugin(packageName: string): Promise<PluginManifest> {
  return invokeCommand<PluginManifest>("install_catalog_plugin", { packageName });
}

/** Built-in GitHub / package sources from `examples/plugins/sources.json`. */
export interface PluginSourceEntry {
  id: string;
  name: string;
  display_name: string;
  github?: string | null;
  ref?: string | null;
  kind: string;
  package_path?: string | null;
  capabilities?: string[];
  license?: string | null;
  notes?: string | null;
}

export function listPluginSources(): Promise<PluginSourceEntry[]> {
  return invokeCommand<PluginSourceEntry[]>("list_plugin_sources");
}

/**
 * Closed-loop install: catalog name or GitHub URL → clone/build/package → install.
 * Example: `https://github.com/markdown-viewer/markdown-viewer-extension`
 */
export function installPluginFromGithub(
  source: string,
  gitRef?: string | null,
): Promise<PluginManifest> {
  return invokeCommand<PluginManifest>("install_plugin_from_github", {
    source,
    gitRef: gitRef ?? null,
  });
}

/** Ensure bundled packages appear under ~/.portico/plugins/available. */
export function seedCatalogPlugins(): Promise<number> {
  return invokeCommand<number>("seed_catalog_plugins");
}

export function listAvailablePluginPackages(): Promise<AvailablePluginPackage[]> {
  return invokeCommand<AvailablePluginPackage[]>("list_available_plugin_packages");
}

export function getPorticoUserDirs(): Promise<PorticoUserDirs> {
  return invokeCommand<PorticoUserDirs>("get_portico_user_dirs");
}

/** Open `~/.portico/plugins`, `available`, or `installed` in the OS file manager. */
export function openPorticoPluginsDir(which?: "available" | "installed" | "plugins"): Promise<void> {
  return invokeCommand<void>("open_portico_plugins_dir", { which: which ?? null });
}

export function listPlugins(): Promise<PluginManifest[]> {
  return invokeCommand<PluginManifest[]>("list_plugins");
}

/** Read installed plugin HTML entrypoint for host-side srcdoc boot. */
export function readPluginEntrypointHtml(
  installPath: string,
  entrypoint: string,
): Promise<string> {
  return invokeCommand<string>("read_plugin_entrypoint_html", { installPath, entrypoint });
}

/** Read a small package-relative text asset (provider.js / styles.css). */
export function readPluginPackageTextFile(
  installPath: string,
  relativePath: string,
): Promise<string> {
  return invokeCommand<string>("read_plugin_package_text_file", {
    installPath,
    relativePath,
  });
}

export function enablePlugin(id: PluginId, enabled: boolean): Promise<void> {
  return invokeCommand<void>("enable_plugin", { id, enabled });
}

export function uninstallPlugin(id: PluginId): Promise<void> {
  return invokeCommand<void>("uninstall_plugin", { id });
}

// Skill catalog commands. Invocation is intentionally unavailable until the
// backend provides an executable, permission-gated skill runtime.

export function listSkills(pluginId?: PluginId): Promise<Skill[]> {
  return invokeCommand<Skill[]>("list_skills", { pluginId: pluginId ?? null });
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
  return invokeCommand<McpToolInfo[]>("list_mcp_tools", { serverId: serverId ?? null });
}

export function invokeMcpTool(
  workspaceId: WorkspaceId,
  toolName: string,
  arguments_: Record<string, unknown>,
  serverId?: number,
): Promise<unknown> {
  return invokeCommand<unknown>("invoke_mcp_tool", {
    workspaceId,
    serverId: serverId ?? null,
    name: toolName,
    arguments: arguments_,
  });
}

export function refreshMcpTools(): Promise<void> {
  return invokeCommand<void>("refresh_mcp_tools");
}

// Multi-agent orchestration (memory-conditioned)

export function listAgents(): Promise<AgentDefinition[]> {
  return invokeCommand<AgentDefinition[]>("list_agents");
}

export function recallWorkflowPatterns(
  task: string,
  workspaceId?: WorkspaceId | null,
): Promise<PatternHint[]> {
  return invokeCommand<PatternHint[]>("recall_workflow_patterns", {
    task,
    workspaceId: workspaceId ?? null,
  });
}

export function previewOrchestrationPlan(
  parentRunId: AgentRunId,
  task: string,
): Promise<OrchestrationPlan> {
  return invokeCommand<OrchestrationPlan>("preview_orchestration_plan", { parentRunId, task });
}

export function startOrchestration(
  workspaceId: WorkspaceId,
  threadId: ThreadId,
  task: string,
  workflowId?: string | null,
): Promise<Orchestration> {
  return invokeCommand<Orchestration>("start_orchestration", {
    workspaceId,
    threadId,
    task,
    workflowId: workflowId ?? null,
  });
}

export function listBundledWorkflows(): Promise<
  Array<{ id: string; title: string; summary: string }>
> {
  return invokeCommand("list_bundled_workflows");
}

export function listWorkflowTemplates(
  workspaceId?: WorkspaceId | null,
): Promise<WorkflowTemplate[]> {
  return invokeCommand<WorkflowTemplate[]>("list_workflow_templates", {
    workspaceId: workspaceId ?? null,
  });
}

export function saveWorkflowTemplate(template: WorkflowTemplate): Promise<WorkflowTemplate> {
  return invokeCommand<WorkflowTemplate>("save_workflow_template", { template });
}

export function getWorkflowTemplate(templateId: WorkflowTemplateId): Promise<WorkflowTemplate> {
  return invokeCommand<WorkflowTemplate>("get_workflow_template", { templateId });
}

export function deleteWorkflowTemplate(templateId: WorkflowTemplateId): Promise<void> {
  return invokeCommand<void>("delete_workflow_template", { templateId });
}

export function getOrchestration(orchestrationId: OrchestrationId): Promise<Orchestration> {
  return invokeCommand<Orchestration>("get_orchestration", { orchestrationId });
}

export function listThreadOrchestrations(threadId: ThreadId): Promise<Orchestration[]> {
  return invokeCommand<Orchestration[]>("list_thread_orchestrations", { threadId });
}

export function cancelOrchestration(orchestrationId: OrchestrationId): Promise<Orchestration> {
  return invokeCommand<Orchestration>("cancel_orchestration", { orchestrationId });
}

export function retryOrchestrationStage(
  orchestrationId: OrchestrationId,
  stageId: string,
): Promise<Orchestration> {
  return invokeCommand<Orchestration>("retry_orchestration_stage", {
    orchestrationId,
    stageId,
  });
}

export function getOrchestrationProgress(
  orchestrationId: OrchestrationId,
): Promise<import("./schemas").OrchestrationProgress> {
  return invokeCommand("get_orchestration_progress", { orchestrationId });
}

export function continueOrchestration(
  orchestrationId: OrchestrationId,
): Promise<Orchestration> {
  return invokeCommand<Orchestration>("continue_orchestration", { orchestrationId });
}

export function muteWorkflowPattern(patternId: WorkflowPatternId): Promise<void> {
  return invokeCommand<void>("mute_workflow_pattern", { patternId });
}

export function listWorkflowPatterns(
  scope: MemoryScope,
  workspaceId?: WorkspaceId | null,
): Promise<WorkflowPattern[]> {
  return invokeCommand<WorkflowPattern[]>("list_workflow_patterns", {
    scope,
    workspaceId: workspaceId ?? null,
  });
}

// Automation commands

export function listAutomations(workspaceId?: WorkspaceId | null): Promise<Automation[]> {
  return invokeCommand<Automation[]>("list_automations", { workspaceId: workspaceId ?? null });
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
    workspaceId,
    name,
    description,
    trigger,
    cronExpr,
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
    workspaceId: workspaceId ?? null,
    unreadOnly: unreadOnly ?? false,
  });
}

export function markNotificationRead(id: NotificationId): Promise<Notification> {
  return invokeCommand<Notification>("mark_notification_read", { id });
}

export function dismissNotification(id: NotificationId): Promise<void> {
  return invokeCommand<void>("dismiss_notification", { id });
}

export function sendTestNotification(
  workspaceId: WorkspaceId,
  title: string,
  body: string,
): Promise<Notification> {
  return invokeCommand<Notification>("send_test_notification", { workspaceId, title, body });
}

// Background task commands

export function listBackgroundTasks(workspaceId?: WorkspaceId | null): Promise<BackgroundTask[]> {
  return invokeCommand<BackgroundTask[]>("list_background_tasks", {
    workspaceId: workspaceId ?? null,
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
    workspaceId,
    url,
    title: title ?? null,
  });
}

export function closeBrowserWindow(workspaceId: WorkspaceId, id: BrowserWindowId): Promise<void> {
  return invokeCommand<void>("close_browser_window", { workspaceId, id });
}

export function listBrowserWindows(): Promise<BrowserWindowInfo[]> {
  return invokeCommand<BrowserWindowInfo[]>("list_browser_windows");
}

export function browserUseAction(
  workspaceId: WorkspaceId,
  id: BrowserWindowId,
  action: BrowserAction,
): Promise<string> {
  return invokeCommand<string>("browser_use_action", { workspaceId, id, action });
}

// Desktop automation commands

export function captureScreen(workspaceId: WorkspaceId): Promise<DesktopCapture> {
  return invokeCommand<DesktopCapture>("capture_screen", { workspaceId });
}

export function moveMouse(workspaceId: WorkspaceId, x: number, y: number): Promise<void> {
  return invokeCommand<void>("move_mouse", { workspaceId, x, y });
}

export function clickMouse(workspaceId: WorkspaceId): Promise<void> {
  return invokeCommand<void>("click_mouse", { workspaceId });
}

export function typeText(workspaceId: WorkspaceId, text: string): Promise<void> {
  return invokeCommand<void>("type_text", { workspaceId, text });
}

export function focusApp(workspaceId: WorkspaceId, name: string): Promise<void> {
  return invokeCommand<void>("focus_app", { workspaceId, name });
}

// Artifact preview command

export function previewArtifact(workspaceId: WorkspaceId, path: string): Promise<ArtifactPreview> {
  return invokeCommand<ArtifactPreview>("preview_artifact", { workspaceId, path });
}

// Diagnostics / migration / updater commands

export function collectDiagnosticsBundle(): Promise<DiagnosticsBundle> {
  return invokeCommand<DiagnosticsBundle>("collect_diagnostics_bundle");
}

export function listMigrations(): Promise<MigrationInfo[]> {
  return invokeCommand<MigrationInfo[]>("list_migrations");
}

export function rollbackLastMigration(): Promise<void> {
  return invokeCommand<void>("rollback_last_migration");
}


// Canvas / mind-map

export function getOrCreateProjectCanvas(workspaceId: WorkspaceId): Promise<Canvas> {
  return invokeCommand<Canvas>("get_or_create_project_canvas", { workspaceId });
}

export function getOrCreateThreadCanvas(
  workspaceId: WorkspaceId,
  threadId: ThreadId,
): Promise<Canvas> {
  return invokeCommand<Canvas>("get_or_create_thread_canvas", { workspaceId, threadId });
}

export function getCanvasSnapshot(canvasId: CanvasId): Promise<CanvasSnapshot> {
  return invokeCommand<CanvasSnapshot>("get_canvas_snapshot", { canvasId });
}

export function updateCanvasViewport(
  canvasId: CanvasId,
  viewportJson: string,
): Promise<Canvas> {
  return invokeCommand<Canvas>("update_canvas_viewport", { canvasId, viewportJson });
}

export function upsertCanvasNode(node: CanvasNode): Promise<void> {
  return invokeCommand<void>("upsert_canvas_node", { node });
}

export function deleteCanvasNode(nodeId: CanvasNodeId): Promise<void> {
  return invokeCommand<void>("delete_canvas_node", { nodeId });
}

export function upsertCanvasEdge(edge: CanvasEdge): Promise<void> {
  return invokeCommand<void>("upsert_canvas_edge", { edge });
}

export function deleteCanvasEdge(edgeId: CanvasEdgeId): Promise<void> {
  return invokeCommand<void>("delete_canvas_edge", { edgeId });
}

export function upsertCanvasLink(link: CanvasLink): Promise<void> {
  return invokeCommand<void>("upsert_canvas_link", { link });
}

export function deleteCanvasLink(linkId: CanvasLinkId): Promise<void> {
  return invokeCommand<void>("delete_canvas_link", { linkId });
}

/** Heuristically organize workspace conversations into canvas insight nodes. */
export function extractCanvasInsights(
  canvasId: CanvasId,
  mode: "heuristic" = "heuristic",
): Promise<CanvasSnapshot> {
  return invokeCommand<CanvasSnapshot>("extract_canvas_insights", { canvasId, mode });
}

/** Template-decompose a free-text goal into Stage nodes under a Goal. */
export function decomposeCanvasGoal(
  canvasId: CanvasId,
  goalText: string,
  parentNodeId?: CanvasNodeId | null,
): Promise<CanvasSnapshot> {
  return invokeCommand<CanvasSnapshot>("decompose_canvas_goal", {
    canvasId,
    goalText,
    parentNodeId: parentNodeId ?? null,
  });
}

/** Persist stage launch bookkeeping after send/orchestration. */
export function markCanvasStageLaunched(
  nodeId: CanvasNodeId,
  threadId: ThreadId,
  runId?: string | null,
): Promise<CanvasNode> {
  return invokeCommand<CanvasNode>("mark_canvas_stage_launched", {
    nodeId,
    threadId,
    runId: runId ?? null,
  });
}

/** Set goal/stage status from the UI. */
export function setCanvasNodeStatus(
  nodeId: CanvasNodeId,
  status: string,
): Promise<CanvasNode> {
  return invokeCommand<CanvasNode>("set_canvas_node_status", { nodeId, status });
}

/** Write run outcome back onto Stage/Goal nodes launched from the canvas. */
export function reconcileCanvasStagesFromRun(
  workspaceId: WorkspaceId,
  threadId: ThreadId,
  runId: string,
  outcome: "done" | "blocked" | "in_progress" | string,
): Promise<CanvasNode[]> {
  return invokeCommand<CanvasNode[]>("reconcile_canvas_stages_from_run", {
    workspaceId,
    threadId,
    runId,
    outcome,
  });
}

/** Re-link insights to goal stages after extract (also called server-side). */
export function refreshCanvasGoalLinks(canvasId: CanvasId): Promise<CanvasSnapshot> {
  return invokeCommand<CanvasSnapshot>("refresh_canvas_goal_links", { canvasId });
}
