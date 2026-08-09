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

export const messageIdSchema = z.string().min(1).brand<"MessageId">();
export type MessageId = z.infer<typeof messageIdSchema>;

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
  "Xai",
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

export const modelSelectionScopeSchema = z.enum(["Global", "Workspace", "Thread"]);
export type ModelSelectionScope = z.infer<typeof modelSelectionScopeSchema>;

export const activeModelSelectionSchema = z.object({
  scope: modelSelectionScopeSchema,
  workspace_id: workspaceIdSchema.nullable(),
  thread_id: threadIdSchema.nullable(),
  provider_id: providerIdSchema,
  model_id: modelIdSchema,
  provider_name: z.string(),
  model_name: z.string(),
  updated_at: z.string().datetime(),
});
export type ActiveModelSelection = z.infer<typeof activeModelSelectionSchema>;

export const providerHealthStatusSchema = z.enum([
  "Checking",
  "Ready",
  "Degraded",
  "InvalidCredentials",
  "Unsupported",
]);
export type ProviderHealthStatus = z.infer<typeof providerHealthStatusSchema>;

export const providerHealthSchema = z.object({
  provider_id: providerIdSchema,
  model_id: modelIdSchema,
  status: providerHealthStatusSchema,
  error_code: z.string().nullable(),
  message: z.string().nullable(),
  checked_at: z.string().datetime(),
});
export type ProviderHealth = z.infer<typeof providerHealthSchema>;

export const runModelSnapshotSchema = z.object({
  run_id: agentRunIdSchema,
  provider_id: providerIdSchema,
  model_id: modelIdSchema,
  provider_name: z.string(),
  model_name: z.string(),
  provider_config_updated_at: z.string().datetime(),
  created_at: z.string().datetime(),
  selection_reason: z.string().nullable().optional(),
  thinking_mode: z.string().nullable().optional(),
  thinking_degraded: z.boolean().optional(),
});
export type RunModelSnapshot = z.infer<typeof runModelSnapshotSchema>;

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
  "Interrupted",
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
  /** Parent session when this thread was branched (mind-map relationship). */
  parent_thread_id: threadIdSchema.nullable(),
  /** Soft-delete timestamp; active sessions have null. Purged after 30 days. */
  archived_at: z.string().datetime().nullable(),
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

export const messageRoleSchema = z.enum(["User", "Assistant", "System"]);
export type MessageRole = z.infer<typeof messageRoleSchema>;

export const messageSchema = z.object({
  id: messageIdSchema,
  thread_id: threadIdSchema,
  run_id: agentRunIdSchema.nullable(),
  role: messageRoleSchema,
  content: z.string(),
  client_request_id: z.string().nullable(),
  created_at: z.string().datetime(),
});
export type Message = z.infer<typeof messageSchema>;

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

export const pluginCapabilitySchema = z.enum([
  "markdown.preview",
  "markdown.export.html",
  "markdown.export.docx",
  "markdown.export.pdf",
]);
export type PluginCapability = z.infer<typeof pluginCapabilitySchema>;

export const pluginManifestSchema = z.object({
  id: pluginIdSchema,
  name: z.string().min(1),
  version: z.string().min(1),
  display_name: z.string().min(1),
  description: z.string(),
  skills: z.array(z.string()),
  tools: z.array(z.string()),
  entrypoint: z.string().min(1).nullable().optional(),
  capabilities: z.array(pluginCapabilitySchema).default([]),
  install_path: z.string().min(1).nullable().optional(),
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

export const modelTierSchema = z.enum(["fast", "balanced", "strong"]);
export type ModelTier = z.infer<typeof modelTierSchema>;

export const thinkingModeSchema = z.enum(["off", "on", "auto"]);
export type ThinkingMode = z.infer<typeof thinkingModeSchema>;

export const writeIsolationSchema = z.enum(["none", "prefer_worktree", "require_worktree"]);
export type WriteIsolation = z.infer<typeof writeIsolationSchema>;

export const retryClassSchema = z.enum(["transient", "idempotent_only", "never"]);
export type RetryClass = z.infer<typeof retryClassSchema>;

export const reasoningEffortSchema = z.enum(["low", "medium", "high"]);
export type ReasoningEffort = z.infer<typeof reasoningEffortSchema>;

export const runExecutionSpecSchema = z.object({
  role: z.string(),
  allowed_tools: z.array(z.string()),
  model_tier: modelTierSchema.default("balanced"),
  thinking_mode: thinkingModeSchema.default("auto"),
  reasoning_effort: reasoningEffortSchema.nullable().optional(),
  timeout_ms: z.number().int().nonnegative(),
  soft_timeout_ms: z.number().int().nonnegative().nullable().optional(),
  max_tool_steps: z.number().int().nonnegative(),
  retry_class: retryClassSchema.default("transient"),
  write_isolation: writeIsolationSchema.default("none"),
});
export type RunExecutionSpec = z.infer<typeof runExecutionSpecSchema>;

export const agentDefinitionSchema = z.object({
  name: z.string().min(1),
  description: z.string(),
  system_instructions: z.string(),
  allowed_tools: z.array(z.string()),
  default_model_policy: z.string(),
  default_permission_scope: permissionScopeSchema,
  model_tier: modelTierSchema.optional(),
  thinking_default: thinkingModeSchema.optional(),
  timeout_ms: z.number().int().nonnegative().optional(),
  write_isolation: writeIsolationSchema.optional(),
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
  retry_count: z.number().int().nonnegative().optional(),
  last_error_code: z.string().nullable().optional(),
});
export type SubagentRun = z.infer<typeof subagentRunSchema>;

export const workflowPatternIdSchema = z.string().uuid().brand<"WorkflowPatternId">();
export type WorkflowPatternId = z.infer<typeof workflowPatternIdSchema>;

export const workflowPatternStatusSchema = z.enum([
  "active",
  "suggested",
  "muted",
  "rejected",
  "Active",
  "Suggested",
  "Muted",
  "Rejected",
]);
export type WorkflowPatternStatus = z.infer<typeof workflowPatternStatusSchema>;

export const workflowPatternSchema = z.object({
  id: workflowPatternIdSchema,
  // Inline enum — memoryScopeSchema is declared later in this module.
  scope: z.enum(["Session", "Thread", "Workspace", "User"]),
  workspace_id: workspaceIdSchema.nullable(),
  name: z.string(),
  summary: z.string(),
  trigger_text: z.string(),
  preferred_roles: z.array(z.string()),
  collaboration_style: z.string(),
  strength: z.number(),
  success_count: z.number(),
  failure_count: z.number(),
  last_used_at: z.string().datetime().nullable().optional(),
  status: workflowPatternStatusSchema,
  fingerprint: z.string().nullable().optional(),
  evidence_count: z.number().optional().default(0),
  confidence: z.number().optional().default(0),
  last_success_at: z.string().datetime().nullable().optional(),
  last_failure_at: z.string().datetime().nullable().optional(),
  tool_strategy: z.string().optional().default(""),
  output_kind: z.string().optional().default(""),
  task_kind: z.string().optional().default(""),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type WorkflowPattern = z.infer<typeof workflowPatternSchema>;

export const patternHintSchema = z.object({
  id: workflowPatternIdSchema,
  name: z.string(),
  summary: z.string(),
  preferred_roles: z.array(z.string()),
  collaboration_style: z.string(),
  strength: z.number(),
  score: z.number(),
});
export type PatternHint = z.infer<typeof patternHintSchema>;

export const orchestrationStageKindSchema = z.enum(["single", "foreach", "reduce", "loop"]);
export type OrchestrationStageKind = z.infer<typeof orchestrationStageKindSchema>;

export const orchestrationStageStatusSchema = z.enum([
  "Pending",
  "Running",
  "Completed",
  "Failed",
  "Cancelled",
  "Skipped",
]);
export type OrchestrationStageStatus = z.infer<typeof orchestrationStageStatusSchema>;

export const orchestrationStageTaskSchema = z.object({
  id: z.string(),
  item_index: z.number().int().nullable().optional(),
  label: z.string(),
  status: orchestrationStageStatusSchema,
  subagent_id: agentRunIdSchema.nullable().optional(),
  output_summary: z.string().nullable().optional(),
  output_payload: z.string().nullable().optional(),
  attempt: z.number().int().nonnegative().optional(),
  last_error_code: z.string().nullable().optional(),
});
export type OrchestrationStageTask = z.infer<typeof orchestrationStageTaskSchema>;

export const orchestrationStageSchema = z.object({
  id: z.string(),
  kind: orchestrationStageKindSchema,
  title: z.string(),
  agent_name: z.string(),
  status: orchestrationStageStatusSchema,
  prompt_template: z.string(),
  depends_on: z.array(z.string()).default([]),
  foreach_path: z.string().nullable().optional(),
  body_stage_ids: z.array(z.string()).optional().default([]),
  max_iterations: z.number().int().nullable().optional(),
  stop_flag_path: z.string().nullable().optional(),
  current_iteration: z.number().int().nullable().optional(),
  tasks: z.array(orchestrationStageTaskSchema).default([]),
  output_payload: z.string().nullable().optional(),
  error_message: z.string().nullable().optional(),
  execution_spec: runExecutionSpecSchema.nullable().optional(),
});
export type OrchestrationStage = z.infer<typeof orchestrationStageSchema>;

export const workflowTemplateIdSchema = z.string().uuid().brand<"WorkflowTemplateId">();
export type WorkflowTemplateId = z.infer<typeof workflowTemplateIdSchema>;

export const workflowTemplateSchema = z.object({
  id: workflowTemplateIdSchema,
  catalog_key: z.string().nullable().optional(),
  title: z.string(),
  summary: z.string(),
  stages: z.array(orchestrationStageSchema).default([]),
  builtin: z.boolean().default(false),
  workspace_id: workspaceIdSchema.nullable().optional(),
  created_at: z.string(),
  updated_at: z.string(),
});
export type WorkflowTemplate = z.infer<typeof workflowTemplateSchema>;

export const orchestrationPlanSchema = z.object({
  parent_run_id: agentRunIdSchema,
  subagents: z.array(subagentRunSchema),
  pattern_ids: z.array(workflowPatternIdSchema).default([]),
  planning_rationale: z.string().default(""),
  stages: z.array(orchestrationStageSchema).default([]),
  workflow_id: z.string().nullable().optional(),
  workflow_title: z.string().nullable().optional(),
});
export type OrchestrationPlan = z.infer<typeof orchestrationPlanSchema>;

export const bundledWorkflowSchema = z.object({
  id: z.string(),
  title: z.string(),
  summary: z.string(),
});
export type BundledWorkflow = z.infer<typeof bundledWorkflowSchema>;

export const orchestrationIdSchema = z.string().uuid().brand<"OrchestrationId">();
export type OrchestrationId = z.infer<typeof orchestrationIdSchema>;

export const orchestrationStatusSchema = z.enum([
  "Planning",
  "Running",
  "Completed",
  "PartialCompleted",
  "Interrupted",
  "Failed",
  "Cancelled",
]);
export type OrchestrationStatus = z.infer<typeof orchestrationStatusSchema>;

export const orchestrationStageProgressSchema = z.object({
  id: z.string(),
  title: z.string(),
  agent_name: z.string(),
  status: z.string(),
  model_tier: z.string().nullable().optional(),
  thinking_mode: z.string().nullable().optional(),
  attempt: z.number().int().nonnegative(),
  error_code: z.string().nullable().optional(),
  error_message: z.string().nullable().optional(),
  tasks_completed: z.number().int().nonnegative(),
  tasks_total: z.number().int().nonnegative(),
  can_retry: z.boolean(),
  allowed_tools: z.array(z.string()).default([]),
});
export type OrchestrationStageProgress = z.infer<typeof orchestrationStageProgressSchema>;

export const orchestrationProgressSchema = z.object({
  id: orchestrationIdSchema,
  status: orchestrationStatusSchema,
  percent: z.number().int().min(0).max(100),
  current_stage_id: z.string().nullable().optional(),
  stages: z.array(orchestrationStageProgressSchema).default([]),
  can_retry_stage_ids: z.array(z.string()).default([]),
  can_continue: z.boolean(),
  result_summary: z.string().nullable().optional(),
  soft_timeout_warned: z.boolean().default(false),
});
export type OrchestrationProgress = z.infer<typeof orchestrationProgressSchema>;

export const orchestrationSchema = z.object({
  id: orchestrationIdSchema,
  parent_run_id: agentRunIdSchema,
  workspace_id: workspaceIdSchema,
  thread_id: threadIdSchema,
  task: z.string(),
  status: orchestrationStatusSchema,
  plan: orchestrationPlanSchema,
  pattern_ids: z.array(workflowPatternIdSchema).default([]),
  result_summary: z.string().nullable(),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
  completed_at: z.string().datetime().nullable(),
});
export type Orchestration = z.infer<typeof orchestrationSchema>;

// Memory / context schemas

export const memoryIdSchema = z.string().min(1).brand<"MemoryId">();
export type MemoryId = z.infer<typeof memoryIdSchema>;

export function asMemoryId(id: string): MemoryId {
  return id as MemoryId;
}

export const memoryScopeSchema = z.enum(["Session", "Thread", "Workspace", "User"]);
export type MemoryScope = z.infer<typeof memoryScopeSchema>;

export const memoryKindSchema = z.enum([
  "UserPreference",
  "WorkspaceConvention",
  "StableFact",
  "DeliveryPreference",
  "ToolPreference",
  "NegativeConstraint",
]);
export type MemoryKind = z.infer<typeof memoryKindSchema>;

export const memoryItemSchema = z.object({
  id: memoryIdSchema,
  scope: memoryScopeSchema,
  workspace_id: workspaceIdSchema.nullable(),
  thread_id: threadIdSchema.nullable(),
  key: z.string().min(1),
  value: z.string(),
  sensitive: z.boolean(),
  kind: memoryKindSchema.nullable().optional(),
  source_run_id: agentRunIdSchema.nullable().optional(),
  confidence: z.number().nullable().optional(),
  last_used_at: z.string().datetime().nullable().optional(),
  use_count: z.number().optional().default(0),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type MemoryItem = z.infer<typeof memoryItemSchema>;

export const candidateStatusSchema = z.enum(["Proposed", "Accepted", "Rejected", "Expired"]);
export type CandidateStatus = z.infer<typeof candidateStatusSchema>;

export const memoryCandidateIdSchema = z.string().uuid().brand<"MemoryCandidateId">();
export type MemoryCandidateId = z.infer<typeof memoryCandidateIdSchema>;

export const memoryCandidateSchema = z.object({
  id: memoryCandidateIdSchema,
  run_id: agentRunIdSchema,
  workspace_id: workspaceIdSchema.nullable().optional(),
  thread_id: threadIdSchema.nullable().optional(),
  scope: memoryScopeSchema,
  kind: memoryKindSchema,
  key: z.string(),
  value: z.string(),
  fingerprint: z.string(),
  confidence: z.number(),
  sensitive: z.boolean(),
  evidence: z.array(z.string()),
  status: candidateStatusSchema,
  extractor_version: z.number().int(),
  created_at: z.string().datetime(),
  reviewed_at: z.string().datetime().nullable().optional(),
});
export type MemoryCandidate = z.infer<typeof memoryCandidateSchema>;

export const runFeedbackRatingSchema = z.enum(["Helpful", "NotHelpful"]);
export type RunFeedbackRating = z.infer<typeof runFeedbackRatingSchema>;

export const runFeedbackSchema = z.object({
  run_id: agentRunIdSchema,
  rating: runFeedbackRatingSchema,
  comment: z.string().nullable().optional(),
  created_at: z.string().datetime(),
});
export type RunFeedback = z.infer<typeof runFeedbackSchema>;

export const behaviorPolicySchema = z.object({
  response_language: z.string().nullable().optional(),
  response_style: z.string().nullable().optional(),
  explore_before_edit: z.boolean().optional().default(false),
  run_tests_after_edit: z.boolean().optional().default(false),
  preferred_test_commands: z.array(z.string()).optional().default([]),
  preferred_output_format: z.string().nullable().optional(),
  memory_ids: z.array(memoryIdSchema).optional().default([]),
  pattern_ids: z.array(workflowPatternIdSchema).optional().default([]),
  negative_constraints: z.array(z.string()).optional().default([]),
});
export type BehaviorPolicy = z.infer<typeof behaviorPolicySchema>;

export const learningQueueStatusSchema = z.object({
  queued: z.number().int().nonnegative(),
  running: z.number().int().nonnegative(),
  failed: z.number().int().nonnegative(),
  completed_recent: z.number().int().nonnegative(),
});
export type LearningQueueStatus = z.infer<typeof learningQueueStatusSchema>;

export const ragIndexStatusSchema = z.object({
  workspace_id: workspaceIdSchema,
  embedding_provider_id: z.string(),
  dimension: z.number(),
  indexed: z.number(),
  stale: z.number(),
  failed: z.number(),
  needs_reindex: z.number(),
  total_documents: z.number(),
  last_indexed_at: z.string().datetime().nullable().optional(),
});
export type RagIndexStatus = z.infer<typeof ragIndexStatusSchema>;

export const ragRefreshResultSchema = z.object({
  workspace_id: workspaceIdSchema,
  scanned: z.number(),
  added: z.number(),
  updated: z.number(),
  removed: z.number(),
  unchanged: z.number(),
  failed: z.number(),
});
export type RagRefreshResult = z.infer<typeof ragRefreshResultSchema>;

export const runLearningSummarySchema = z.object({
  run_id: agentRunIdSchema,
  experience: z.any().nullable().optional(),
  candidates: z.array(memoryCandidateSchema).optional().default([]),
  feedback: runFeedbackSchema.nullable().optional(),
  memory_ids_used: z.array(memoryIdSchema).optional().default([]),
  pattern_ids_used: z.array(workflowPatternIdSchema).optional().default([]),
  behavior_policy: behaviorPolicySchema.nullable().optional(),
  outbound_manifest: z
    .object({
      provider_kind: z.string(),
      local_provider: z.boolean(),
      message_count: z.number().int(),
      memory_ids: z.array(memoryIdSchema),
      rag_paths: z.array(z.string()),
      total_bytes: z.number(),
      sensitive_content_blocked: z.boolean(),
    })
    .nullable()
    .optional(),
  recall_scores: z.array(z.tuple([memoryIdSchema, z.number()])).optional().default([]),
});
export type RunLearningSummary = z.infer<typeof runLearningSummarySchema>;

export const privacyModeSchema = z.enum([
  "FullyLocal",
  "LocalStorageCloudInference",
  "CloudInferenceAndEmbedding",
]);
export type PrivacyMode = z.infer<typeof privacyModeSchema>;

export const traceRetentionModeSchema = z.enum([
  "FullLocalTrace",
  "RedactedTrace",
  "MetadataOnly",
]);
export type TraceRetentionMode = z.infer<typeof traceRetentionModeSchema>;

export const privacySettingsSchema = z.object({
  privacy_mode: privacyModeSchema,
  trace_retention: traceRetentionModeSchema,
  auto_discover_candidates: z.boolean(),
  remote_model_extraction: z.boolean(),
  auto_promote_patterns: z.boolean(),
  auto_promote_threshold: z.number().int().positive(),
});
export type PrivacySettings = z.infer<typeof privacySettingsSchema>;

export const learningOverviewSchema = z.object({
  pending_candidates: z.number(),
  confirmed_preferences: z.number(),
  active_patterns: z.number(),
  suggested_patterns: z.number(),
  recent_candidate_summaries: z.array(z.string()),
  recent_memory_keys: z.array(z.string()),
  learning_queue: learningQueueStatusSchema,
  sensitive_encryption_enabled: z.boolean(),
  local_storage: z.boolean(),
});
export type LearningOverview = z.infer<typeof learningOverviewSchema>;

export const contextItemDispositionSchema = z.enum([
  "Sent",
  "BlockedSensitive",
  "TrimmedByBudget",
  "NotRelevant",
  "DisabledForRun",
  "LocalOnly",
]);
export type ContextItemDisposition = z.infer<typeof contextItemDispositionSchema>;

export const contextSnapshotItemSchema = z.object({
  kind: z.string(),
  title: z.string(),
  summary: z.string(),
  disposition: contextItemDispositionSchema,
  reason: z.string().nullable().optional(),
  score: z.number().nullable().optional(),
  memory_id: memoryIdSchema.nullable().optional(),
  pattern_id: workflowPatternIdSchema.nullable().optional(),
  path: z.string().nullable().optional(),
});
export type ContextSnapshotItem = z.infer<typeof contextSnapshotItemSchema>;

export const runContextSnapshotSchema = z.object({
  run_id: agentRunIdSchema,
  memory_ids: z.array(memoryIdSchema).optional().default([]),
  pattern_ids: z.array(workflowPatternIdSchema).optional().default([]),
  behavior_policy: behaviorPolicySchema.nullable().optional(),
  outbound_manifest: z
    .object({
      provider_kind: z.string(),
      local_provider: z.boolean(),
      message_count: z.number().int(),
      memory_ids: z.array(memoryIdSchema),
      rag_paths: z.array(z.string()),
      total_bytes: z.number(),
      sensitive_content_blocked: z.boolean(),
    })
    .nullable()
    .optional(),
  recall_scores: z.array(z.tuple([memoryIdSchema, z.number()])).optional().default([]),
  items: z.array(contextSnapshotItemSchema).optional().default([]),
  learning: runLearningSummarySchema.nullable().optional(),
});
export type RunContextSnapshot = z.infer<typeof runContextSnapshotSchema>;

export const learningDataExportSchema = z.object({
  exported_at: z.string().datetime(),
  memories: z.array(memoryItemSchema),
  candidates: z.array(memoryCandidateSchema),
  patterns: z.array(workflowPatternSchema),
  privacy: privacySettingsSchema,
  schema_version: z.number().int(),
});
export type LearningDataExport = z.infer<typeof learningDataExportSchema>;

export const workflowPatternPatchSchema = z.object({
  name: z.string().nullable().optional(),
  summary: z.string().nullable().optional(),
  trigger_text: z.string().nullable().optional(),
  preferred_roles: z.array(z.string()).nullable().optional(),
  collaboration_style: z.string().nullable().optional(),
  tool_strategy: z.string().nullable().optional(),
  output_kind: z.string().nullable().optional(),
});
export type WorkflowPatternPatch = z.infer<typeof workflowPatternPatchSchema>;

export const workflowPatternEvidenceSchema = z.object({
  pattern_id: workflowPatternIdSchema,
  success_count: z.number(),
  failure_count: z.number(),
  evidence_count: z.number(),
  confidence: z.number(),
  last_success_at: z.string().datetime().nullable().optional(),
  last_failure_at: z.string().datetime().nullable().optional(),
  last_used_at: z.string().datetime().nullable().optional(),
  status: workflowPatternStatusSchema,
});
export type WorkflowPatternEvidence = z.infer<typeof workflowPatternEvidenceSchema>;

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
  recalled_memory_ids: z.array(memoryIdSchema).optional().default([]),
  pattern_ids: z.array(workflowPatternIdSchema).optional().default([]),
  behavior_policy: behaviorPolicySchema.nullable().optional(),
  outbound_manifest: z
    .object({
      provider_kind: z.string(),
      local_provider: z.boolean(),
      message_count: z.number().int(),
      memory_ids: z.array(memoryIdSchema),
      rag_paths: z.array(z.string()),
      total_bytes: z.number(),
      sensitive_content_blocked: z.boolean(),
    })
    .nullable()
    .optional(),
  context_budget: z
    .object({
      model_context_tokens: z.number(),
      reserved_output_tokens: z.number(),
      reserved_tool_tokens: z.number(),
      transcript_tokens: z.number(),
      instruction_tokens: z.number(),
      memory_tokens: z.number(),
      rag_tokens: z.number(),
    })
    .nullable()
    .optional(),
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


// Canvas / mind-map schemas

export const canvasIdSchema = z.string().uuid().brand<"CanvasId">();
export type CanvasId = z.infer<typeof canvasIdSchema>;

export const canvasNodeIdSchema = z.string().uuid().brand<"CanvasNodeId">();
export type CanvasNodeId = z.infer<typeof canvasNodeIdSchema>;

export const canvasEdgeIdSchema = z.string().uuid().brand<"CanvasEdgeId">();
export type CanvasEdgeId = z.infer<typeof canvasEdgeIdSchema>;

export const canvasLinkIdSchema = z.string().uuid().brand<"CanvasLinkId">();
export type CanvasLinkId = z.infer<typeof canvasLinkIdSchema>;

export function asCanvasId(id: string): CanvasId {
  return id as CanvasId;
}

export function asCanvasNodeId(id: string): CanvasNodeId {
  return id as CanvasNodeId;
}

export function asCanvasEdgeId(id: string): CanvasEdgeId {
  return id as CanvasEdgeId;
}

export function asCanvasLinkId(id: string): CanvasLinkId {
  return id as CanvasLinkId;
}

export const canvasKindSchema = z.enum(["Project", "Thread"]);
export type CanvasKind = z.infer<typeof canvasKindSchema>;

export const canvasNodeKindSchema = z.enum([
  "Insight",
  "Goal",
  "Stage",
  "ThreadCluster",
  "Note",
]);
export type CanvasNodeKind = z.infer<typeof canvasNodeKindSchema>;

export const canvasNodeStatusSchema = z.enum([
  "Todo",
  "InProgress",
  "Done",
  "Blocked",
  "Stale",
]);
export type CanvasNodeStatus = z.infer<typeof canvasNodeStatusSchema>;

export const canvasNodeSourceSchema = z.enum(["Auto", "User", "Agent"]);
export type CanvasNodeSource = z.infer<typeof canvasNodeSourceSchema>;

export const canvasEdgeKindSchema = z.enum(["Parent", "Related", "DerivedFrom", "Blocks"]);
export type CanvasEdgeKind = z.infer<typeof canvasEdgeKindSchema>;

export const canvasLinkRefTypeSchema = z.enum(["Thread", "Message", "Run", "Orchestration"]);
export type CanvasLinkRefType = z.infer<typeof canvasLinkRefTypeSchema>;

export const canvasSchema = z.object({
  id: canvasIdSchema,
  workspace_id: workspaceIdSchema,
  thread_id: threadIdSchema.nullable(),
  title: z.string(),
  kind: canvasKindSchema,
  viewport_json: z.string(),
  revision: z.number().int(),
  last_extracted_at: z.string().datetime().nullable(),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type Canvas = z.infer<typeof canvasSchema>;

export const canvasNodeSchema = z.object({
  id: canvasNodeIdSchema,
  canvas_id: canvasIdSchema,
  kind: canvasNodeKindSchema,
  title: z.string(),
  summary: z.string(),
  status: canvasNodeStatusSchema,
  parent_id: canvasNodeIdSchema.nullable(),
  position_x: z.number(),
  position_y: z.number(),
  layout_rank: z.number().int(),
  source: canvasNodeSourceSchema,
  payload_json: z.string(),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
});
export type CanvasNode = z.infer<typeof canvasNodeSchema>;

export const canvasEdgeSchema = z.object({
  id: canvasEdgeIdSchema,
  canvas_id: canvasIdSchema,
  from_id: canvasNodeIdSchema,
  to_id: canvasNodeIdSchema,
  kind: canvasEdgeKindSchema,
  label: z.string().nullable(),
  created_at: z.string().datetime(),
});
export type CanvasEdge = z.infer<typeof canvasEdgeSchema>;

export const canvasLinkSchema = z.object({
  id: canvasLinkIdSchema,
  node_id: canvasNodeIdSchema,
  ref_type: canvasLinkRefTypeSchema,
  ref_id: z.string(),
  snippet: z.string().nullable(),
  created_at: z.string().datetime(),
});
export type CanvasLink = z.infer<typeof canvasLinkSchema>;

export const canvasSnapshotSchema = z.object({
  canvas: canvasSchema,
  nodes: z.array(canvasNodeSchema),
  edges: z.array(canvasEdgeSchema),
  links: z.array(canvasLinkSchema),
});
export type CanvasSnapshot = z.infer<typeof canvasSnapshotSchema>;
