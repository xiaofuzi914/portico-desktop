import { z } from "zod";

/**
 * Frontend-only schemas. No API keys or backend secrets appear here.
 */

export const workspaceIdSchema = z.string().min(1).brand<"WorkspaceId">();
export type WorkspaceId = z.infer<typeof workspaceIdSchema>;

export const threadIdSchema = z.string().min(1).brand<"ThreadId">();
export type ThreadId = z.infer<typeof threadIdSchema>;

export const agentRunIdSchema = z.string().min(1).brand<"AgentRunId">();
export type AgentRunId = z.infer<typeof agentRunIdSchema>;

export const worktreeIdSchema = z.string().min(1).brand<"WorktreeId">();
export type WorktreeId = z.infer<typeof worktreeIdSchema>;

export const terminalIdSchema = z.string().min(1).brand<"TerminalId">();
export type TerminalId = z.infer<typeof terminalIdSchema>;

export function asWorkspaceId(id: string): WorkspaceId {
  return id as WorkspaceId;
}

export function asThreadId(id: string): ThreadId {
  return id as ThreadId;
}

export function asAgentRunId(id: string): AgentRunId {
  return id as AgentRunId;
}

export function asWorktreeId(id: string): WorktreeId {
  return id as WorktreeId;
}

export function asTerminalId(id: string): TerminalId {
  return id as TerminalId;
}

export const providerIdSchema = z.string().min(1).brand<"ProviderId">();
export type ProviderId = z.infer<typeof providerIdSchema>;

export const modelIdSchema = z.string().min(1).brand<"ModelId">();
export type ModelId = z.infer<typeof modelIdSchema>;

export function asProviderId(id: string): ProviderId {
  return id as ProviderId;
}

export function asModelId(id: string): ModelId {
  return id as ModelId;
}

export const providerKindSchema = z.enum([
  "OpenAI",
  "Anthropic",
  "Moonshot",
  "DeepSeek",
  "Google",
  "Groq",
  "OpenRouter",
  "AzureOpenAI",
  "Ollama",
  "Custom",
]);
export type ProviderKind = z.infer<typeof providerKindSchema>;

export const retryPolicySchema = z.object({
  max_retries: z.number().int().nonnegative(),
  initial_backoff_ms: z.number().int().nonnegative(),
  max_backoff_ms: z.number().int().nonnegative(),
});
export type RetryPolicy = z.infer<typeof retryPolicySchema>;

export const providerConfigSchema = z.object({
  id: providerIdSchema,
  kind: providerKindSchema,
  display_name: z.string().min(1),
  base_url: z.string().nullable(),
  api_key_reference: z.string(),
  organization_id: z.string().nullable(),
  project_id: z.string().nullable(),
  default_headers: z.record(z.string(), z.string()),
  timeout_ms: z.number().int().nonnegative(),
  retry_policy: retryPolicySchema,
  fallback_provider_ids: z.array(providerIdSchema),
  enabled: z.boolean(),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type ProviderConfig = z.infer<typeof providerConfigSchema>;

export const modelCapabilitySchema = z.object({
  supports_streaming: z.boolean(),
  supports_tools: z.boolean(),
  supports_json_schema: z.boolean(),
  supports_vision: z.boolean(),
  supports_pdf: z.boolean(),
  supports_system_prompt: z.boolean(),
  supports_embeddings: z.boolean(),
  max_context_tokens: z.number().int().nonnegative().nullable(),
  input_price_per_1k: z.number().nonnegative().nullable(),
  output_price_per_1k: z.number().nonnegative().nullable(),
});
export type ModelCapability = z.infer<typeof modelCapabilitySchema>;

export const modelInfoSchema = z.object({
  id: modelIdSchema,
  provider_id: providerIdSchema,
  provider_name: z.string(),
  model_name: z.string().min(1),
  display_name: z.string().min(1),
  capabilities: modelCapabilitySchema,
});
export type ModelInfo = z.infer<typeof modelInfoSchema>;

export const usageBudgetSchema = z.object({
  per_run_usd: z.number().nonnegative().nullable(),
  daily_usd: z.number().nonnegative().nullable(),
});
export type UsageBudget = z.infer<typeof usageBudgetSchema>;

export const usageSummarySchema = z.object({
  daily_usage_usd: z.number().nonnegative(),
  per_run_budget_usd: z.number().nonnegative().nullable(),
  daily_budget_usd: z.number().nonnegative().nullable(),
});
export type UsageSummary = z.infer<typeof usageSummarySchema>;

export const usageRecordSchema = z.object({
  id: z.number().int(),
  run_id: agentRunIdSchema,
  provider_id: providerIdSchema,
  model_id: modelIdSchema,
  input_tokens: z.number().int().nonnegative(),
  output_tokens: z.number().int().nonnegative(),
  cost_usd: z.number(),
  created_at: z.string().datetime(),
});
export type UsageRecord = z.infer<typeof usageRecordSchema>;

export const agentRunStatusSchema = z.enum([
  "Queued",
  "Running",
  "WaitingApproval",
  "Paused",
  "Cancelled",
  "Failed",
  "Completed",
]);
export type AgentRunStatus = z.infer<typeof agentRunStatusSchema>;

export const workspaceSchema = z.object({
  id: workspaceIdSchema,
  name: z.string().min(1),
  root_path: z.string(),
  trusted: z.boolean(),
  allowed_read_paths: z.array(z.string()).default([]),
  allowed_write_paths: z.array(z.string()).default([]),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type Workspace = z.infer<typeof workspaceSchema>;

export const worktreeSchema = z.object({
  id: worktreeIdSchema,
  workspace_id: workspaceIdSchema,
  thread_id: threadIdSchema,
  name: z.string().min(1),
  path: z.string(),
  created_at: z.string().datetime(),
});
export type Worktree = z.infer<typeof worktreeSchema>;

export const terminalOutputSchema = z.object({
  stdout: z.string(),
  stderr: z.string(),
  exit_code: z.number().int(),
});
export type TerminalOutput = z.infer<typeof terminalOutputSchema>;

export const threadSchema = z.object({
  id: threadIdSchema,
  workspace_id: workspaceIdSchema,
  title: z.string().min(1),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type Thread = z.infer<typeof threadSchema>;

export const agentRunSchema = z.object({
  id: agentRunIdSchema,
  thread_id: threadIdSchema,
  workspace_id: workspaceIdSchema,
  status: agentRunStatusSchema,
  created_at: z.string().datetime(),
  started_at: z.string().datetime().nullable(),
  completed_at: z.string().datetime().nullable(),
});
export type AgentRun = z.infer<typeof agentRunSchema>;

export const runEventSchema = z.object({
  id: z.number().int(),
  run_id: agentRunIdSchema,
  thread_id: threadIdSchema,
  sequence: z.number().int(),
  event_type: z.string(),
  payload: z.unknown(),
  created_at: z.string().datetime(),
});
export type RunEvent = z.infer<typeof runEventSchema>;

export const artifactSchema = z.object({
  id: z.number().int(),
  run_id: agentRunIdSchema,
  name: z.string(),
  mime_type: z.string(),
  path: z.string().nullable(),
  content_preview: z.string().nullable(),
});
export type Artifact = z.infer<typeof artifactSchema>;

export const approvalRequestIdSchema = z.number().int().brand<"ApprovalRequestId">();
export type ApprovalRequestId = z.infer<typeof approvalRequestIdSchema>;

export function asApprovalRequestId(id: number): ApprovalRequestId {
  return id as ApprovalRequestId;
}

export const approvalRequestStatusSchema = z.enum(["Pending", "Approved", "Denied"]);
export type ApprovalRequestStatus = z.infer<typeof approvalRequestStatusSchema>;

export const approvalRequestSchema = z.object({
  id: approvalRequestIdSchema,
  run_id: agentRunIdSchema,
  workspace_id: workspaceIdSchema,
  thread_id: threadIdSchema,
  action: z.string(),
  resource: z.string(),
  status: approvalRequestStatusSchema,
  created_at: z.string().datetime(),
  resolved_at: z.string().datetime().nullable(),
  resolution_reason: z.string().nullable(),
});
export type ApprovalRequest = z.infer<typeof approvalRequestSchema>;

export const runtimeEventSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("RunStarted"),
    data: z.object({ run: agentRunSchema }),
  }),
  z.object({
    kind: z.literal("RunStatusChanged"),
    data: z.object({ run_id: agentRunIdSchema, status: agentRunStatusSchema }),
  }),
  z.object({
    kind: z.literal("MessageDelta"),
    data: z.object({ run_id: agentRunIdSchema, content: z.string() }),
  }),
  z.object({
    kind: z.literal("MessageCompleted"),
    data: z.object({ run_id: agentRunIdSchema, content: z.string() }),
  }),
  z.object({
    kind: z.literal("ToolRequested"),
    data: z.object({
      run_id: agentRunIdSchema,
      tool_name: z.string(),
      arguments: z.unknown(),
    }),
  }),
  z.object({
    kind: z.literal("ToolApprovalRequired"),
    data: z.object({
      run_id: agentRunIdSchema,
      request_id: z.number().int(),
      action: z.string(),
      resource: z.string(),
    }),
  }),
  z.object({
    kind: z.literal("ToolStarted"),
    data: z.object({ run_id: agentRunIdSchema, tool_name: z.string() }),
  }),
  z.object({
    kind: z.literal("ToolCompleted"),
    data: z.object({
      run_id: agentRunIdSchema,
      tool_name: z.string(),
      result: z.unknown(),
    }),
  }),
  z.object({
    kind: z.literal("ToolFailed"),
    data: z.object({
      run_id: agentRunIdSchema,
      tool_name: z.string(),
      error: z.string(),
    }),
  }),
  z.object({
    kind: z.literal("ArtifactCreated"),
    data: z.object({ run_id: agentRunIdSchema, artifact: artifactSchema }),
  }),
  z.object({
    kind: z.literal("RunFailed"),
    data: z.object({ run_id: agentRunIdSchema, error: z.string() }),
  }),
  z.object({
    kind: z.literal("RunCompleted"),
    data: z.object({ run_id: agentRunIdSchema }),
  }),
]);
export type RuntimeEvent = z.infer<typeof runtimeEventSchema>;

// Security schemas

export const permissionScopeSchema = z.enum(["Once", "Run", "Thread", "Workspace", "Global"]);
export type PermissionScope = z.infer<typeof permissionScopeSchema>;

export const permissionDecisionSchema = z.enum(["Allow", "Ask", "Deny"]);
export type PermissionDecision = z.infer<typeof permissionDecisionSchema>;

/**
 * PermissionResult mirrors the backend enum's default serde JSON shape.
 * Unit variants serialize as strings; struct variants serialize as objects.
 */
export const permissionResultSchema = z.union([
  z.literal("Allowed"),
  z.object({ Ask: z.object({ request: approvalRequestSchema }) }),
  z.object({ Denied: z.object({ reason: z.string() }) }),
]);
export type PermissionResult = z.infer<typeof permissionResultSchema>;

export function isPermissionAllowed(result: PermissionResult): result is "Allowed" {
  return result === "Allowed";
}

export function permissionAskRequest(result: PermissionResult): ApprovalRequest | undefined {
  return typeof result === "object" && "Ask" in result ? result.Ask.request : undefined;
}

export function permissionDeniedReason(result: PermissionResult): string | undefined {
  return typeof result === "object" && "Denied" in result ? result.Denied.reason : undefined;
}

export const permissionRequestSchema = z.object({
  workspace_id: workspaceIdSchema,
  thread_id: threadIdSchema.nullable().optional(),
  run_id: agentRunIdSchema.nullable().optional(),
  action: z.string().min(1),
  resource: z.string().min(1),
  trusted_workspace: z.boolean(),
});
export type PermissionRequest = z.infer<typeof permissionRequestSchema>;

export const permissionRuleSchema = z.object({
  action_pattern: z.string().min(1),
  resource_pattern: z.string().min(1),
  decision: permissionDecisionSchema,
  scope: permissionScopeSchema,
});
export type PermissionRule = z.infer<typeof permissionRuleSchema>;

export const auditEventSchema = z.object({
  workspace_id: workspaceIdSchema,
  thread_id: threadIdSchema.nullable().optional(),
  run_id: agentRunIdSchema.nullable().optional(),
  action: z.string(),
  resource: z.string(),
  outcome: permissionResultSchema,
});
export type AuditEvent = z.infer<typeof auditEventSchema>;

// Background task schemas

export const backgroundTaskIdSchema = z.string().min(1).brand<"BackgroundTaskId">();
export type BackgroundTaskId = z.infer<typeof backgroundTaskIdSchema>;

export function asBackgroundTaskId(id: string): BackgroundTaskId {
  return id as BackgroundTaskId;
}

export const taskKindSchema = z.enum(["AgentRun", "Routine", "ThreadWakeup", "ScheduledJob"]);
export type TaskKind = z.infer<typeof taskKindSchema>;

export const backgroundTaskStatusSchema = z.enum([
  "Queued",
  "Running",
  "Completed",
  "Failed",
  "Cancelled",
]);
export type BackgroundTaskStatus = z.infer<typeof backgroundTaskStatusSchema>;

export const backgroundTaskSchema = z.object({
  id: backgroundTaskIdSchema,
  workspace_id: workspaceIdSchema.nullable(),
  thread_id: threadIdSchema.nullable(),
  run_id: agentRunIdSchema.nullable(),
  task_kind: taskKindSchema,
  payload: z.unknown(),
  status: backgroundTaskStatusSchema,
  priority: z.number().int(),
  attempts: z.number().int().nonnegative(),
  max_attempts: z.number().int().nonnegative(),
  scheduled_at: z.string().datetime().nullable(),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type BackgroundTask = z.infer<typeof backgroundTaskSchema>;

// Automation schemas

export const automationIdSchema = z.string().min(1).brand<"AutomationId">();
export type AutomationId = z.infer<typeof automationIdSchema>;

export function asAutomationId(id: string): AutomationId {
  return id as AutomationId;
}

export const automationTriggerSchema = z.enum([
  "Scheduled",
  "FileChange",
  "GitEvent",
  "ManualRoutine",
  "WebhookReserved",
  "ThreadWakeup",
]);
export type AutomationTrigger = z.infer<typeof automationTriggerSchema>;

export const automationSchema = z.object({
  id: automationIdSchema,
  workspace_id: workspaceIdSchema,
  name: z.string().min(1),
  description: z.string(),
  trigger: automationTriggerSchema,
  cron_expr: z.string().nullable(),
  enabled: z.boolean(),
  next_run_at: z.string().datetime().nullable(),
  last_run_at: z.string().datetime().nullable(),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type Automation = z.infer<typeof automationSchema>;

// Notification schemas

export const notificationIdSchema = z.string().min(1).brand<"NotificationId">();
export type NotificationId = z.infer<typeof notificationIdSchema>;

export function asNotificationId(id: string): NotificationId {
  return id as NotificationId;
}

export const notificationCategorySchema = z.enum([
  "System",
  "InApp",
  "ApprovalRequired",
  "TaskCompleted",
]);
export type NotificationCategory = z.infer<typeof notificationCategorySchema>;

export const notificationSchema = z.object({
  id: notificationIdSchema,
  workspace_id: workspaceIdSchema.nullable(),
  thread_id: threadIdSchema.nullable(),
  run_id: agentRunIdSchema.nullable(),
  title: z.string().min(1),
  body: z.string(),
  category: notificationCategorySchema,
  read: z.boolean(),
  created_at: z.string().datetime(),
});
export type Notification = z.infer<typeof notificationSchema>;

// Plugin / skill / MCP schemas

export const pluginIdSchema = z.string().min(1).brand<"PluginId">();
export type PluginId = z.infer<typeof pluginIdSchema>;

export const skillIdSchema = z.string().min(1).brand<"SkillId">();
export type SkillId = z.infer<typeof skillIdSchema>;

export function asPluginId(id: string): PluginId {
  return id as PluginId;
}

export function asSkillId(id: string): SkillId {
  return id as SkillId;
}

export const filesystemPermissionSchema = z.enum(["none", "read", "write"]);
export type FilesystemPermission = z.infer<typeof filesystemPermissionSchema>;

export const pluginPermissionsSchema = z.object({
  network: z.array(z.string()),
  filesystem: filesystemPermissionSchema,
});
export type PluginPermissions = z.infer<typeof pluginPermissionsSchema>;

export const pluginManifestSchema = z.object({
  id: pluginIdSchema,
  name: z.string().min(1),
  version: z.string().min(1),
  display_name: z.string().min(1),
  description: z.string(),
  skills: z.array(z.string()),
  tools: z.array(z.string()),
  permissions: pluginPermissionsSchema,
  enabled: z.boolean(),
  installed_at: z.string().datetime(),
});
export type PluginManifest = z.infer<typeof pluginManifestSchema>;

export const skillSchema = z.object({
  id: skillIdSchema,
  plugin_id: pluginIdSchema,
  name: z.string().min(1),
  description: z.string(),
  trigger_description: z.string(),
  instruction_file: z.string().nullable(),
  required_tools: z.array(z.string()),
});
export type Skill = z.infer<typeof skillSchema>;

export const mcpTransportSchema = z.enum(["Stdio", "Http"]);
export type McpTransport = z.infer<typeof mcpTransportSchema>;

export const mcpServerConfigSchema = z.object({
  id: z.number().int(),
  name: z.string().min(1),
  transport: mcpTransportSchema,
  command: z.string().nullable(),
  args: z.array(z.string()),
  url: z.string().nullable(),
  env: z.record(z.string(), z.string()),
  enabled: z.boolean(),
});
export type McpServerConfig = z.infer<typeof mcpServerConfigSchema>;

export const mcpToolInfoSchema = z.object({
  name: z.string().min(1),
  description: z.string(),
  input_schema: z.unknown(),
});
export type McpToolInfo = z.infer<typeof mcpToolInfoSchema>;

export const auditLogEntrySchema = z.object({
  id: z.number().int(),
  run_id: agentRunIdSchema.nullable(),
  thread_id: threadIdSchema.nullable(),
  workspace_id: workspaceIdSchema.nullable(),
  action: z.string(),
  resource: z.string(),
  decision: z.string(),
  reason: z.string().nullable(),
  created_at: z.string().datetime(),
});
export type AuditLogEntry = z.infer<typeof auditLogEntrySchema>;

// Orchestration schemas

export const builtInAgentSchema = z.enum([
  "Default",
  "Explorer",
  "Planner",
  "Worker",
  "Reviewer",
  "SecurityReviewer",
  "Tester",
  "Researcher",
  "DocWriter",
]);
export type BuiltInAgent = z.infer<typeof builtInAgentSchema>;

export const agentDefinitionSchema = z.object({
  name: z.string().min(1),
  description: z.string(),
  system_instructions: z.string(),
  allowed_tools: z.array(z.string()),
  default_model_policy: z.string(),
  default_permission_scope: permissionScopeSchema,
});
export type AgentDefinition = z.infer<typeof agentDefinitionSchema>;

export const subagentRunSchema = z.object({
  id: agentRunIdSchema,
  parent_run_id: agentRunIdSchema,
  agent_name: z.string().min(1),
  status: agentRunStatusSchema,
  task_description: z.string(),
  output_summary: z.string().nullable(),
  created_at: z.string().datetime(),
  completed_at: z.string().datetime().nullable(),
});
export type SubagentRun = z.infer<typeof subagentRunSchema>;

export const orchestrationPlanSchema = z.object({
  parent_run_id: agentRunIdSchema,
  subagents: z.array(subagentRunSchema),
});
export type OrchestrationPlan = z.infer<typeof orchestrationPlanSchema>;

// Memory / context schemas

export const memoryIdSchema = z.string().min(1).brand<"MemoryId">();
export type MemoryId = z.infer<typeof memoryIdSchema>;

export function asMemoryId(id: string): MemoryId {
  return id as MemoryId;
}

export const memoryScopeSchema = z.enum(["Session", "Thread", "Workspace", "User"]);
export type MemoryScope = z.infer<typeof memoryScopeSchema>;

export const memoryItemSchema = z.object({
  id: memoryIdSchema,
  scope: memoryScopeSchema,
  workspace_id: workspaceIdSchema.nullable(),
  thread_id: threadIdSchema.nullable(),
  key: z.string().min(1),
  value: z.string(),
  sensitive: z.boolean(),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type MemoryItem = z.infer<typeof memoryItemSchema>;

export const instructionFileSchema = z.object({
  path: z.string(),
  content: z.string(),
  scope: z.string(),
});
export type InstructionFile = z.infer<typeof instructionFileSchema>;

export const ragChunkSchema = z.object({
  id: z.number().int(),
  document_path: z.string(),
  chunk_index: z.number().int().nonnegative(),
  content: z.string(),
  score: z.number(),
});
export type RagChunk = z.infer<typeof ragChunkSchema>;

export const contextSummarySchema = z.object({
  run_id: agentRunIdSchema,
  thread_id: threadIdSchema,
  instructions: z.array(instructionFileSchema),
  memories: z.array(memoryItemSchema),
  rag_chunks: z.array(ragChunkSchema),
  estimated_tokens: z.number().int().nonnegative(),
  privacy_flags: z.array(z.string()),
});
export type ContextSummary = z.infer<typeof contextSummarySchema>;

// Browser / desktop automation schemas

export const browserWindowIdSchema = z.string().min(1).brand<"BrowserWindowId">();
export type BrowserWindowId = z.infer<typeof browserWindowIdSchema>;

export function asBrowserWindowId(id: string): BrowserWindowId {
  return id as BrowserWindowId;
}

export const browserWindowInfoSchema = z.object({
  id: browserWindowIdSchema,
  url: z.string(),
  title: z.string(),
});
export type BrowserWindowInfo = z.infer<typeof browserWindowInfoSchema>;

export const browserActionSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("Click"),
    selector: z.string().min(1),
  }),
  z.object({
    kind: z.literal("Type"),
    selector: z.string().min(1),
    text: z.string(),
  }),
  z.object({
    kind: z.literal("ExtractVisibleText"),
  }),
  z.object({
    kind: z.literal("Wait"),
    ms: z.number().int().nonnegative(),
  }),
  z.object({
    kind: z.literal("Screenshot"),
  }),
]);
export type BrowserAction = z.infer<typeof browserActionSchema>;

export const desktopCaptureSchema = z.object({
  image_base64: z.string(),
  width: z.number().int().nonnegative(),
  height: z.number().int().nonnegative(),
});
export type DesktopCapture = z.infer<typeof desktopCaptureSchema>;

export const artifactPreviewSchema = z.object({
  path: z.string(),
  mime_type: z.string(),
  content_base64: z.string(),
  size_bytes: z.number().int().nonnegative(),
});
export type ArtifactPreview = z.infer<typeof artifactPreviewSchema>;

// Diagnostics / migration / updater schemas

export const diagnosticsBundleIdSchema = z.string().min(1).brand<"DiagnosticsBundleId">();
export type DiagnosticsBundleId = z.infer<typeof diagnosticsBundleIdSchema>;

export function asDiagnosticsBundleId(id: string): DiagnosticsBundleId {
  return id as DiagnosticsBundleId;
}

export const diagnosticsBundleSchema = z.object({
  id: diagnosticsBundleIdSchema,
  created_at: z.string().datetime(),
  log_path: z.string(),
  audit_summary_path: z.string(),
  app_version: z.string(),
  os_info: z.string(),
  redacted: z.boolean(),
  size_bytes: z.number().int().nonnegative(),
});
export type DiagnosticsBundle = z.infer<typeof diagnosticsBundleSchema>;

export const migrationInfoSchema = z.object({
  version: z.number().int(),
  name: z.string(),
  applied_at: z.string().datetime(),
  checksum: z.string(),
});
export type MigrationInfo = z.infer<typeof migrationInfoSchema>;
