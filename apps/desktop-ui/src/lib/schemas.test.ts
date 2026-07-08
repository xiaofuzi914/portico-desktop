import { describe, expect, it } from "vitest";
import { parseRuntimeEvent } from "@/lib/tauri-api";
import {
  agentRunStatusSchema,
  artifactPreviewSchema,
  asAgentRunId,
  asAutomationId,
  asBackgroundTaskId,
  asBrowserWindowId,
  asDiagnosticsBundleId,
  asMemoryId,
  asModelId,
  asNotificationId,
  asPluginId,
  asProviderId,
  asSkillId,
  asThreadId,
  asWorktreeId,
  asWorkspaceId,
  auditLogEntrySchema,
  automationSchema,
  automationTriggerSchema,
  backgroundTaskSchema,
  browserActionSchema,
  browserWindowInfoSchema,
  contextSummarySchema,
  desktopCaptureSchema,
  diagnosticsBundleSchema,
  instructionFileSchema,
  mcpServerConfigSchema,
  mcpToolInfoSchema,
  memoryItemSchema,
  memoryScopeSchema,
  migrationInfoSchema,
  modelCapabilitySchema,
  modelInfoSchema,
  notificationCategorySchema,
  notificationSchema,
  orchestrationPlanSchema,
  permissionDecisionSchema,
  permissionRequestSchema,
  permissionResultSchema,
  permissionRuleSchema,
  permissionScopeSchema,
  pluginManifestSchema,
  pluginPermissionsSchema,
  providerConfigSchema,
  providerKindSchema,
  ragChunkSchema,
  retryPolicySchema,
  runtimeEventSchema,
  skillSchema,
  subagentRunSchema,
  taskKindSchema,
  terminalOutputSchema,
  usageBudgetSchema,
  usageRecordSchema,
  worktreeSchema,
  workspaceSchema,
} from "@/lib/schemas";

describe("schemas", () => {
  it("validates a workspace", () => {
    const result = workspaceSchema.safeParse({
      id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440000"),
      name: "Test Workspace",
      root_path: "/tmp/test",
      trusted: true,
      allowed_read_paths: ["/tmp/test/read"],
      allowed_write_paths: ["/tmp/test/write"],
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.allowed_read_paths).toEqual(["/tmp/test/read"]);
      expect(result.data.allowed_write_paths).toEqual(["/tmp/test/write"]);
    }
  });

  it("defaults workspace allowed paths to empty arrays", () => {
    const result = workspaceSchema.safeParse({
      id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440000"),
      name: "Test Workspace",
      root_path: "/tmp/test",
      trusted: true,
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.allowed_read_paths).toEqual([]);
      expect(result.data.allowed_write_paths).toEqual([]);
    }
  });

  it("validates a worktree", () => {
    const result = worktreeSchema.safeParse({
      id: asWorktreeId("550e8400-e29b-41d4-a716-446655440000"),
      workspace_id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440001"),
      thread_id: asThreadId("550e8400-e29b-41d4-a716-446655440002"),
      name: "feature-branch",
      path: "/tmp/test/feature-branch",
      created_at: "2026-01-01T00:00:00.000Z",
    });
    expect(result.success).toBe(true);
  });

  it("validates terminal output", () => {
    const result = terminalOutputSchema.safeParse({
      stdout: "hello",
      stderr: "",
      exit_code: 0,
    });
    expect(result.success).toBe(true);
  });

  it("rejects an invalid status", () => {
    const result = agentRunStatusSchema.safeParse("invalid");
    expect(result.success).toBe(false);
  });

  it("validates all runtime event kinds", () => {
    const runId = asAgentRunId("550e8400-e29b-41d4-a716-446655440000");
    const threadId = asThreadId("550e8400-e29b-41d4-a716-446655440001");

    const events = [
      {
        kind: "RunStarted",
        data: {
          run: {
            id: runId,
            thread_id: threadId,
            workspace_id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440002"),
            status: "Running",
            created_at: "2026-01-01T00:00:00.000Z",
            started_at: null,
            completed_at: null,
          },
        },
      },
      { kind: "RunStatusChanged", data: { run_id: runId, status: "Completed" } },
      { kind: "MessageDelta", data: { run_id: runId, content: "Hello" } },
      { kind: "MessageCompleted", data: { run_id: runId, content: "Hello world" } },
      { kind: "ToolRequested", data: { run_id: runId, tool_name: "read_file", arguments: {} } },
      {
        kind: "ToolApprovalRequired",
        data: { run_id: runId, request_id: 1, action: "write", resource: "/tmp" },
      },
      { kind: "ToolStarted", data: { run_id: runId, tool_name: "read_file" } },
      { kind: "ToolCompleted", data: { run_id: runId, tool_name: "read_file", result: {} } },
      { kind: "ToolFailed", data: { run_id: runId, tool_name: "read_file", error: "oops" } },
      {
        kind: "ArtifactCreated",
        data: {
          run_id: runId,
          artifact: {
            id: 1,
            run_id: runId,
            name: "log",
            mime_type: "text/plain",
            path: null,
            content_preview: null,
          },
        },
      },
      { kind: "RunFailed", data: { run_id: runId, error: "failed" } },
      { kind: "RunCompleted", data: { run_id: runId } },
    ];

    for (const event of events) {
      const result = runtimeEventSchema.safeParse(event);
      expect(result.success, `expected ${event.kind} to parse`).toBe(true);
      expect(parseRuntimeEvent(event)?.kind).toBe(event.kind);
    }
  });

  it("rejects unknown event kinds", () => {
    const result = parseRuntimeEvent({ kind: "Unknown", data: {} });
    expect(result).toBeUndefined();
  });

  it("validates permission schemas", () => {
    expect(permissionScopeSchema.safeParse("Once").success).toBe(true);
    expect(permissionScopeSchema.safeParse("Global").success).toBe(true);
    expect(permissionScopeSchema.safeParse("Forever").success).toBe(false);

    expect(permissionDecisionSchema.safeParse("Allow").success).toBe(true);
    expect(permissionDecisionSchema.safeParse("Ask").success).toBe(true);
    expect(permissionDecisionSchema.safeParse("Deny").success).toBe(true);
    expect(permissionDecisionSchema.safeParse("Maybe").success).toBe(false);

    const allowed = permissionResultSchema.safeParse("Allowed");
    expect(allowed.success).toBe(true);

    const denied = permissionResultSchema.safeParse({ Denied: { reason: "private network" } });
    expect(denied.success).toBe(true);

    expect(permissionResultSchema.safeParse({ Allowed: null }).success).toBe(false);
    expect(permissionResultSchema.safeParse("Denied").success).toBe(false);

    expect(
      permissionRequestSchema.safeParse({
        workspace_id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440000"),
        thread_id: asThreadId("550e8400-e29b-41d4-a716-446655440001"),
        run_id: asAgentRunId("550e8400-e29b-41d4-a716-446655440002"),
        action: "filesystem.write",
        resource: "/tmp/test.txt",
        trusted_workspace: true,
      }).success,
    ).toBe(true);

    expect(
      permissionRequestSchema.safeParse({
        workspace_id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440000"),
        action: "shell.exec",
        resource: "rm -rf /",
        trusted_workspace: false,
      }).success,
    ).toBe(true);

    expect(
      permissionRuleSchema.safeParse({
        action_pattern: "filesystem.*",
        resource_pattern: "*",
        decision: "Ask",
        scope: "Workspace",
      }).success,
    ).toBe(true);
  });

  it("validates audit log entries", () => {
    const entry = {
      id: 1,
      run_id: asAgentRunId("550e8400-e29b-41d4-a716-446655440000"),
      thread_id: asThreadId("550e8400-e29b-41d4-a716-446655440001"),
      workspace_id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440002"),
      action: "filesystem.write",
      resource: "/tmp/test.txt",
      decision: "Ask",
      reason: null,
      created_at: "2026-01-01T00:00:00.000Z",
    };
    expect(auditLogEntrySchema.safeParse(entry).success).toBe(true);

    expect(
      auditLogEntrySchema.safeParse({
        ...entry,
        run_id: null,
        thread_id: null,
        workspace_id: null,
      }).success,
    ).toBe(true);
  });

  it("validates provider and model registry schemas", () => {
    const providerId = asProviderId("550e8400-e29b-41d4-a716-446655440000");
    const modelId = asModelId("550e8400-e29b-41d4-a716-446655440001");
    const runId = asAgentRunId("550e8400-e29b-41d4-a716-446655440002");

    expect(providerKindSchema.safeParse("OpenAI").success).toBe(true);
    expect(
      retryPolicySchema.safeParse({ max_retries: 3, initial_backoff_ms: 100, max_backoff_ms: 5000 })
        .success,
    ).toBe(true);

    const provider = {
      id: providerId,
      kind: "Anthropic" as const,
      display_name: "Anthropic",
      base_url: null,
      api_key_reference: "keychain://anthropic",
      organization_id: null,
      project_id: null,
      default_headers: {},
      timeout_ms: 60000,
      retry_policy: { max_retries: 3, initial_backoff_ms: 100, max_backoff_ms: 5000 },
      fallback_provider_ids: [],
      enabled: true,
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    expect(providerConfigSchema.safeParse(provider).success).toBe(true);

    const capabilities = {
      supports_streaming: true,
      supports_tools: true,
      supports_json_schema: false,
      supports_vision: true,
      supports_pdf: false,
      supports_system_prompt: true,
      supports_embeddings: false,
      max_context_tokens: 200000,
      input_price_per_1k: 0.003,
      output_price_per_1k: 0.015,
    };
    expect(modelCapabilitySchema.safeParse(capabilities).success).toBe(true);

    expect(
      modelInfoSchema.safeParse({
        id: modelId,
        provider_id: providerId,
        provider_name: "Anthropic",
        model_name: "claude-3-5-sonnet",
        display_name: "Claude 3.5 Sonnet",
        capabilities,
      }).success,
    ).toBe(true);

    expect(usageBudgetSchema.safeParse({ per_run_usd: 1.0, daily_usd: 10.0 }).success).toBe(true);

    expect(
      usageRecordSchema.safeParse({
        id: 1,
        run_id: runId,
        provider_id: providerId,
        model_id: modelId,
        input_tokens: 1000,
        output_tokens: 500,
        cost_usd: 0.012,
        created_at: "2026-01-01T00:00:00.000Z",
      }).success,
    ).toBe(true);
  });

  it("validates memory schemas", () => {
    expect(memoryScopeSchema.safeParse("Workspace").success).toBe(true);
    expect(memoryScopeSchema.safeParse("Invalid").success).toBe(false);

    const memoryId = asMemoryId("550e8400-e29b-41d4-a716-446655440000");
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440001");
    const threadId = asThreadId("550e8400-e29b-41d4-a716-446655440002");

    expect(
      memoryItemSchema.safeParse({
        id: memoryId,
        scope: "Workspace",
        workspace_id: workspaceId,
        thread_id: threadId,
        key: "user_preference",
        value: "dark mode",
        sensitive: false,
        created_at: "2026-01-01T00:00:00.000Z",
        updated_at: "2026-01-01T00:00:00.000Z",
      }).success,
    ).toBe(true);

    expect(
      instructionFileSchema.safeParse({
        path: "/tmp/AGENTS.md",
        content: "# Instructions",
        scope: "workspace",
      }).success,
    ).toBe(true);

    expect(
      ragChunkSchema.safeParse({
        id: 1,
        document_path: "/tmp/doc.md",
        chunk_index: 0,
        content: "hello world",
        score: 0.95,
      }).success,
    ).toBe(true);

    expect(
      contextSummarySchema.safeParse({
        run_id: asAgentRunId("550e8400-e29b-41d4-a716-446655440003"),
        thread_id: threadId,
        instructions: [{ path: "/tmp/AGENTS.md", content: "# Instructions", scope: "workspace" }],
        memories: [
          {
            id: memoryId,
            scope: "Workspace",
            workspace_id: workspaceId,
            thread_id: null,
            key: "pref",
            value: "dark mode",
            sensitive: true,
            created_at: "2026-01-01T00:00:00.000Z",
            updated_at: "2026-01-01T00:00:00.000Z",
          },
        ],
        rag_chunks: [
          {
            id: 1,
            document_path: "/tmp/doc.md",
            chunk_index: 0,
            content: "hello",
            score: 0.9,
          },
        ],
        estimated_tokens: 1024,
        privacy_flags: ["sensitive_memory"],
      }).success,
    ).toBe(true);
  });

  it("validates plugin and skill schemas", () => {
    const pluginId = asPluginId("550e8400-e29b-41d4-a716-446655440000");
    const skillId = asSkillId("550e8400-e29b-41d4-a716-446655440001");

    expect(
      pluginPermissionsSchema.safeParse({
        network: ["*.example.com"],
        filesystem: "read",
      }).success,
    ).toBe(true);

    expect(
      pluginPermissionsSchema.safeParse({
        network: [],
        filesystem: "admin",
      }).success,
    ).toBe(false);

    expect(
      pluginManifestSchema.safeParse({
        id: pluginId,
        name: "test-plugin",
        version: "1.0.0",
        display_name: "Test Plugin",
        description: "A test plugin",
        skills: ["greet"],
        tools: ["read_file"],
        permissions: { network: [], filesystem: "none" },
        enabled: true,
        installed_at: "2026-01-01T00:00:00.000Z",
      }).success,
    ).toBe(true);

    expect(
      skillSchema.safeParse({
        id: skillId,
        plugin_id: pluginId,
        name: "greet",
        description: "Greet the user",
        trigger_description: "When the user says hello",
        instruction_file: "/tmp/instructions.md",
        required_tools: ["read_file"],
      }).success,
    ).toBe(true);

    expect(
      skillSchema.safeParse({
        id: skillId,
        plugin_id: pluginId,
        name: "greet",
        description: "Greet the user",
        trigger_description: "When the user says hello",
        instruction_file: null,
        required_tools: [],
      }).success,
    ).toBe(true);
  });

  it("validates orchestration schemas", () => {
    const runId = asAgentRunId("550e8400-e29b-41d4-a716-446655440000");
    const parentRunId = asAgentRunId("550e8400-e29b-41d4-a716-446655440001");

    expect(
      subagentRunSchema.safeParse({
        id: runId,
        parent_run_id: parentRunId,
        agent_name: "SecurityReviewer",
        status: "Running",
        task_description: "Review for security issues",
        output_summary: null,
        created_at: "2026-01-01T00:00:00.000Z",
        completed_at: null,
      }).success,
    ).toBe(true);

    expect(
      orchestrationPlanSchema.safeParse({
        parent_run_id: parentRunId,
        subagents: [
          {
            id: runId,
            parent_run_id: parentRunId,
            agent_name: "Reviewer",
            status: "Queued",
            task_description: "Review the plan",
            output_summary: null,
            created_at: "2026-01-01T00:00:00.000Z",
            completed_at: null,
          },
        ],
      }).success,
    ).toBe(true);
  });

  it("validates MCP server and tool schemas", () => {
    expect(
      mcpServerConfigSchema.safeParse({
        id: 1,
        name: "filesystem",
        transport: "Stdio",
        command: "npx",
        args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
        url: null,
        env: { NODE_ENV: "production" },
        enabled: true,
      }).success,
    ).toBe(true);

    expect(
      mcpServerConfigSchema.safeParse({
        id: 2,
        name: "remote",
        transport: "Http",
        command: null,
        args: [],
        url: "http://localhost:3000/sse",
        env: {},
        enabled: false,
      }).success,
    ).toBe(true);

    expect(
      mcpToolInfoSchema.safeParse({
        name: "read_file",
        description: "Read a file",
        input_schema: { type: "object", properties: {} },
      }).success,
    ).toBe(true);
  });

  it("validates background task schemas", () => {
    expect(taskKindSchema.safeParse("AgentRun").success).toBe(true);
    expect(taskKindSchema.safeParse("ScheduledJob").success).toBe(true);
    expect(taskKindSchema.safeParse("Invalid").success).toBe(false);

    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const taskId = asBackgroundTaskId("550e8400-e29b-41d4-a716-446655440001");

    expect(
      backgroundTaskSchema.safeParse({
        id: taskId,
        workspace_id: workspaceId,
        thread_id: null,
        run_id: null,
        task_kind: "ScheduledJob",
        payload: { key: "value" },
        status: "Running",
        priority: 1,
        attempts: 0,
        max_attempts: 3,
        scheduled_at: "2026-01-01T00:00:00.000Z",
        created_at: "2026-01-01T00:00:00.000Z",
        updated_at: "2026-01-01T00:00:00.000Z",
      }).success,
    ).toBe(true);
  });

  it("validates automation schemas", () => {
    expect(automationTriggerSchema.safeParse("Scheduled").success).toBe(true);
    expect(automationTriggerSchema.safeParse("WebhookReserved").success).toBe(true);
    expect(automationTriggerSchema.safeParse("Invalid").success).toBe(false);

    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const automationId = asAutomationId("550e8400-e29b-41d4-a716-446655440001");

    expect(
      automationSchema.safeParse({
        id: automationId,
        workspace_id: workspaceId,
        name: "Nightly sync",
        description: "Runs every night",
        trigger: "Scheduled",
        cron_expr: "0 2 * * *",
        enabled: true,
        next_run_at: "2026-01-02T02:00:00.000Z",
        last_run_at: null,
        created_at: "2026-01-01T00:00:00.000Z",
        updated_at: "2026-01-01T00:00:00.000Z",
      }).success,
    ).toBe(true);

    expect(
      automationSchema.safeParse({
        id: automationId,
        workspace_id: workspaceId,
        name: "Manual routine",
        description: "",
        trigger: "ManualRoutine",
        cron_expr: null,
        enabled: false,
        next_run_at: null,
        last_run_at: null,
        created_at: "2026-01-01T00:00:00.000Z",
        updated_at: "2026-01-01T00:00:00.000Z",
      }).success,
    ).toBe(true);
  });

  it("validates notification schemas", () => {
    expect(notificationCategorySchema.safeParse("System").success).toBe(true);
    expect(notificationCategorySchema.safeParse("ApprovalRequired").success).toBe(true);
    expect(notificationCategorySchema.safeParse("Invalid").success).toBe(false);

    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const notificationId = asNotificationId("550e8400-e29b-41d4-a716-446655440001");

    expect(
      notificationSchema.safeParse({
        id: notificationId,
        workspace_id: workspaceId,
        thread_id: null,
        run_id: null,
        title: "Approval required",
        body: "A tool requires approval",
        category: "ApprovalRequired",
        read: false,
        created_at: "2026-01-01T00:00:00.000Z",
      }).success,
    ).toBe(true);
  });

  it("validates browser and desktop automation schemas", () => {
    const windowId = asBrowserWindowId("550e8400-e29b-41d4-a716-446655440000");

    expect(
      browserWindowInfoSchema.safeParse({
        id: windowId,
        url: "https://example.com",
        title: "Example",
      }).success,
    ).toBe(true);

    expect(browserActionSchema.safeParse({ kind: "Click", selector: "#btn" }).success).toBe(true);
    expect(
      browserActionSchema.safeParse({ kind: "Type", selector: "#input", text: "hello" }).success,
    ).toBe(true);
    expect(browserActionSchema.safeParse({ kind: "ExtractVisibleText" }).success).toBe(true);
    expect(browserActionSchema.safeParse({ kind: "Wait", ms: 500 }).success).toBe(true);
    expect(browserActionSchema.safeParse({ kind: "Screenshot" }).success).toBe(true);
    expect(browserActionSchema.safeParse({ kind: "Invalid" }).success).toBe(false);

    expect(
      desktopCaptureSchema.safeParse({
        image_base64: "abc",
        width: 1920,
        height: 1080,
      }).success,
    ).toBe(true);

    expect(
      artifactPreviewSchema.safeParse({
        path: "/tmp/report.pdf",
        mime_type: "application/pdf",
        content_base64: "base64",
        size_bytes: 1024,
      }).success,
    ).toBe(true);
  });

  it("validates diagnostics and migration schemas", () => {
    const bundleId = asDiagnosticsBundleId("bundle-1");

    expect(
      diagnosticsBundleSchema.safeParse({
        id: bundleId,
        created_at: "2026-01-01T00:00:00.000Z",
        log_path: "/tmp/diagnostics/bundle.zip",
        audit_summary_path: "/tmp/diagnostics/audit.json",
        app_version: "0.1.0",
        os_info: "macOS 15.0",
        redacted: true,
        size_bytes: 10240,
      }).success,
    ).toBe(true);

    expect(
      migrationInfoSchema.safeParse({
        version: 1,
        name: "initial_schema",
        applied_at: "2026-01-01T00:00:00.000Z",
        checksum: "sha256:abc123",
      }).success,
    ).toBe(true);
  });
});
