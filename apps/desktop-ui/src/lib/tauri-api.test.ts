import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const mockInvoke = invoke as unknown as ReturnType<typeof vi.fn>;

import {
  addMcpServer,
  browserUseAction,
  cancelBackgroundTask,
  cancelSubagent,
  clickMouse,
  closeBrowserWindow,
  collectDiagnosticsBundle,
  createAutomation,
  createMemory,
  createTerminal,
  createWorkspace,
  createWorktree,
  deleteAutomation,
  deleteMemory,
  deleteWorktree,
  dismissNotification,
  enablePlugin,
  executeSubagents,
  executeTerminalCommand,
  focusApp,
  captureScreen,
  gitBranch,
  gitCommit,
  gitDiff,
  gitPush,
  gitStage,
  gitStatus,
  gitUnstage,
  inspectContext,
  installPlugin,
  invokeMcpTool,
  invokeSkill,
  listAgents,
  listAutomations,
  listBackgroundTasks,
  listBrowserWindows,
  listMcpServers,
  listMcpTools,
  listMemories,
  listMigrations,
  listNotifications,
  listPlugins,
  listSkills,
  listWorktrees,
  loadInstructions,
  markNotificationRead,
  moveMouse,
  openBrowserWindow,
  planSubagents,
  previewArtifact,
  readTerminalHistory,
  removeMcpServer,
  rollbackLastMigration,
  runAutomationNow,
  searchRag,
  sendTestNotification,
  setWorkspacePaths,
  trustWorkspace,
  typeText,
  uninstallPlugin,
  updateAutomation,
  updateMemory,
  uploadDiagnosticsBundle,
} from "./tauri-api";
import {
  asAgentRunId,
  asAutomationId,
  asBackgroundTaskId,
  asBrowserWindowId,
  asDiagnosticsBundleId,
  asMemoryId,
  asNotificationId,
  asPluginId,
  asSkillId,
  asTerminalId,
  asThreadId,
  asWorktreeId,
  asWorkspaceId,
} from "./schemas";
import type { BrowserAction } from "./schemas";

function mockOk<T>(data: T) {
  mockInvoke.mockResolvedValueOnce({ success: true, data });
}

function mockErr(message: string) {
  mockInvoke.mockResolvedValueOnce({ success: false, error: message });
}

describe("tauri-api phase 6 commands", () => {
  it("creates a workspace with Tauri command argument names", async () => {
    const workspace = {
      id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440000"),
      name: "Test",
      root_path: "/tmp/test",
      trusted: false,
      allowed_read_paths: [],
      allowed_write_paths: [],
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(workspace);

    const result = await createWorkspace("Test", "/tmp/test", false);

    expect(result).toEqual(workspace);
    expect(mockInvoke).toHaveBeenCalledWith("create_workspace", {
      name: "Test",
      rootPath: "/tmp/test",
      trusted: false,
    });
  });

  it("trusts a workspace", async () => {
    const workspace = {
      id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440000"),
      name: "Test",
      root_path: "/tmp/test",
      trusted: true,
      allowed_read_paths: [],
      allowed_write_paths: [],
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(workspace);
    const result = await trustWorkspace(workspace.id, true);
    expect(result).toEqual(workspace);
    expect(mockInvoke).toHaveBeenCalledWith("trust_workspace", { id: workspace.id, trusted: true });
  });

  it("sets workspace allowed paths", async () => {
    mockOk(null);
    const id = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    await setWorkspacePaths(id, ["/read"], ["/write"]);
    expect(mockInvoke).toHaveBeenCalledWith("set_workspace_paths", {
      id,
      read_paths: ["/read"],
      write_paths: ["/write"],
    });
  });

  it("creates a worktree", async () => {
    const worktree = {
      id: asWorktreeId("550e8400-e29b-41d4-a716-446655440000"),
      workspace_id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440001"),
      thread_id: asThreadId("550e8400-e29b-41d4-a716-446655440002"),
      name: "feature",
      path: "/tmp/test/feature",
      created_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(worktree);
    const result = await createWorktree(worktree.workspace_id, worktree.thread_id, "feature");
    expect(result).toEqual(worktree);
    expect(mockInvoke).toHaveBeenCalledWith("create_worktree", {
      workspace_id: worktree.workspace_id,
      thread_id: worktree.thread_id,
      name: "feature",
    });
  });

  it("lists worktrees", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk([]);
    const result = await listWorktrees(workspaceId);
    expect(result).toEqual([]);
    expect(mockInvoke).toHaveBeenCalledWith("list_worktrees", { workspace_id: workspaceId });
  });

  it("deletes a worktree", async () => {
    const id = asWorktreeId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(null);
    await deleteWorktree(id);
    expect(mockInvoke).toHaveBeenCalledWith("delete_worktree", { id });
  });

  it("fetches git status", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk("M src/main.rs");
    const result = await gitStatus(workspaceId, "/repo");
    expect(result).toBe("M src/main.rs");
    expect(mockInvoke).toHaveBeenCalledWith("git_status", {
      workspace_id: workspaceId,
      repo_path: "/repo",
    });
  });

  it("fetches git diff", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk("diff --git");
    const result = await gitDiff(workspaceId, "/repo");
    expect(result).toBe("diff --git");
    expect(mockInvoke).toHaveBeenCalledWith("git_diff", {
      workspace_id: workspaceId,
      repo_path: "/repo",
    });
  });

  it("stages and unstages files", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(null);
    await gitStage(workspaceId, "/repo", ["src/main.rs"]);
    expect(mockInvoke).toHaveBeenCalledWith("git_stage", {
      workspace_id: workspaceId,
      repo_path: "/repo",
      paths: ["src/main.rs"],
    });

    mockOk(null);
    await gitUnstage(workspaceId, "/repo", ["src/main.rs"]);
    expect(mockInvoke).toHaveBeenCalledWith("git_unstage", {
      workspace_id: workspaceId,
      repo_path: "/repo",
      paths: ["src/main.rs"],
    });
  });

  it("commits with a message", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk("abc123");
    const result = await gitCommit(workspaceId, "/repo", "Initial commit");
    expect(result).toBe("abc123");
    expect(mockInvoke).toHaveBeenCalledWith("git_commit", {
      workspace_id: workspaceId,
      repo_path: "/repo",
      message: "Initial commit",
    });
  });

  it("creates and lists branches", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk("Switched to branch 'feature'");
    const result = await gitBranch(workspaceId, "/repo", "feature");
    expect(result).toBe("Switched to branch 'feature'");
    expect(mockInvoke).toHaveBeenCalledWith("git_branch", {
      workspace_id: workspaceId,
      repo_path: "/repo",
      name: "feature",
    });

    mockOk("main");
    const current = await gitBranch(workspaceId, "/repo");
    expect(current).toBe("main");
    expect(mockInvoke).toHaveBeenCalledWith("git_branch", {
      workspace_id: workspaceId,
      repo_path: "/repo",
      name: null,
    });
  });

  it("pushes to default remote", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk("Everything up-to-date");
    const result = await gitPush(workspaceId, "/repo");
    expect(result).toBe("Everything up-to-date");
    expect(mockInvoke).toHaveBeenCalledWith("git_push", {
      workspace_id: workspaceId,
      repo_path: "/repo",
      remote: null,
    });
  });

  it("creates a terminal session", async () => {
    const id = asTerminalId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(id);
    const result = await createTerminal(asThreadId("550e8400-e29b-41d4-a716-446655440001"));
    expect(result).toBe(id);
    expect(mockInvoke).toHaveBeenCalledWith("create_terminal", {
      thread_id: "550e8400-e29b-41d4-a716-446655440001",
    });
  });

  it("executes a terminal command", async () => {
    const id = asTerminalId("550e8400-e29b-41d4-a716-446655440000");
    const output = { stdout: "hello", stderr: "", exit_code: 0 };
    mockOk(output);
    const result = await executeTerminalCommand(id, "echo hello", "/repo");
    expect(result).toEqual(output);
    expect(mockInvoke).toHaveBeenCalledWith("execute_terminal_command", {
      id,
      command: "echo hello",
      cwd: "/repo",
    });
  });

  it("reads terminal history", async () => {
    const id = asTerminalId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(["echo hello", "hello"]);
    const result = await readTerminalHistory(id);
    expect(result).toEqual(["echo hello", "hello"]);
    expect(mockInvoke).toHaveBeenCalledWith("read_terminal_history", { id });
  });

  it("throws on error responses", async () => {
    mockErr("permission denied");
    await expect(
      trustWorkspace(asWorkspaceId("550e8400-e29b-41d4-a716-446655440000"), true),
    ).rejects.toThrow("permission denied");
  });
});

describe("tauri-api phase 7 commands", () => {
  it("lists memories", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk([]);
    const result = await listMemories("Workspace", workspaceId);
    expect(result).toEqual([]);
    expect(mockInvoke).toHaveBeenCalledWith("list_memories", {
      scope: "Workspace",
      workspace_id: workspaceId,
      thread_id: null,
    });
  });

  it("creates a memory", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const memory = {
      id: asMemoryId("550e8400-e29b-41d4-a716-446655440001"),
      scope: "Workspace" as const,
      workspace_id: workspaceId,
      thread_id: null,
      key: "preference",
      value: "dark mode",
      sensitive: false,
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(memory);
    const result = await createMemory(
      "Workspace",
      workspaceId,
      null,
      "preference",
      "dark mode",
      false,
    );
    expect(result).toEqual(memory);
    expect(mockInvoke).toHaveBeenCalledWith("create_memory", {
      scope: "Workspace",
      workspace_id: workspaceId,
      thread_id: null,
      key: "preference",
      value: "dark mode",
      sensitive: false,
    });
  });

  it("updates a memory", async () => {
    const id = asMemoryId("550e8400-e29b-41d4-a716-446655440000");
    const memory = {
      id,
      scope: "Workspace" as const,
      workspace_id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440001"),
      thread_id: null,
      key: "preference",
      value: "light mode",
      sensitive: false,
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(memory);
    const result = await updateMemory(id, "light mode");
    expect(result).toEqual(memory);
    expect(mockInvoke).toHaveBeenCalledWith("update_memory", { id, value: "light mode" });
  });

  it("deletes a memory", async () => {
    const id = asMemoryId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(null);
    await deleteMemory(id);
    expect(mockInvoke).toHaveBeenCalledWith("delete_memory", { id });
  });

  it("loads instructions", async () => {
    mockOk([{ path: "/tmp/AGENTS.md", content: "# Instructions", scope: "workspace" }]);
    const result = await loadInstructions("/tmp");
    expect(result).toEqual([
      { path: "/tmp/AGENTS.md", content: "# Instructions", scope: "workspace" },
    ]);
    expect(mockInvoke).toHaveBeenCalledWith("load_instructions", { workspace_root: "/tmp" });
  });

  it("inspects context", async () => {
    const runId = asAgentRunId("550e8400-e29b-41d4-a716-446655440000");
    const threadId = asThreadId("550e8400-e29b-41d4-a716-446655440001");
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440002");
    const summary = {
      run_id: runId,
      thread_id: threadId,
      instructions: [],
      memories: [],
      rag_chunks: [],
      estimated_tokens: 0,
      privacy_flags: [],
    };
    mockOk(summary);
    const result = await inspectContext(runId, threadId, workspaceId, "/tmp", "test");
    expect(result).toEqual(summary);
    expect(mockInvoke).toHaveBeenCalledWith("inspect_context", {
      run_id: runId,
      thread_id: threadId,
      workspace_id: workspaceId,
      workspace_root: "/tmp",
      query: "test",
    });
  });

  it("searches RAG", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const chunks = [
      {
        id: 1,
        document_path: "/tmp/doc.md",
        chunk_index: 0,
        content: "hello",
        score: 0.9,
      },
    ];
    mockOk(chunks);
    const result = await searchRag(workspaceId, "hello", 3);
    expect(result).toEqual(chunks);
    expect(mockInvoke).toHaveBeenCalledWith("search_rag", {
      workspace_id: workspaceId,
      query: "hello",
      top_n: 3,
    });
  });
});

describe("tauri-api phase 8 commands", () => {
  it("installs and lists plugins", async () => {
    const manifest = {
      id: asPluginId("550e8400-e29b-41d4-a716-446655440000"),
      name: "test-plugin",
      version: "1.0.0",
      display_name: "Test Plugin",
      description: "A test plugin",
      skills: ["greet"],
      tools: ["read_file"],
      permissions: { network: [], filesystem: "none" as const },
      enabled: true,
      installed_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(manifest);
    const installed = await installPlugin(manifest);
    expect(installed).toEqual(manifest);
    expect(mockInvoke).toHaveBeenCalledWith("install_plugin", { manifest });

    mockOk([manifest]);
    const result = await listPlugins();
    expect(result).toEqual([manifest]);
    expect(mockInvoke).toHaveBeenCalledWith("list_plugins", undefined);
  });

  it("toggles and uninstalls plugins", async () => {
    const id = asPluginId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(null);
    await enablePlugin(id, false);
    expect(mockInvoke).toHaveBeenCalledWith("enable_plugin", { id, enabled: false });

    mockOk(null);
    await uninstallPlugin(id);
    expect(mockInvoke).toHaveBeenCalledWith("uninstall_plugin", { id });
  });

  it("lists and invokes skills", async () => {
    const pluginId = asPluginId("550e8400-e29b-41d4-a716-446655440000");
    const skill = {
      id: asSkillId("550e8400-e29b-41d4-a716-446655440001"),
      plugin_id: pluginId,
      name: "greet",
      description: "Greet the user",
      trigger_description: "When the user says hello",
      instruction_file: null,
      required_tools: [],
    };

    mockOk([skill]);
    const skills = await listSkills(pluginId);
    expect(skills).toEqual([skill]);
    expect(mockInvoke).toHaveBeenCalledWith("list_skills", { plugin_id: pluginId });

    mockOk({ status: "ok" });
    const result = await invokeSkill(skill.id, { name: "world" });
    expect(result).toEqual({ status: "ok" });
    expect(mockInvoke).toHaveBeenCalledWith("invoke_skill", {
      skill_id: skill.id,
      arguments: { name: "world" },
    });
  });

  it("manages MCP servers", async () => {
    const config = {
      id: 1,
      name: "filesystem",
      transport: "Stdio" as const,
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      url: null,
      env: {},
      enabled: true,
    };

    mockOk([config]);
    const servers = await listMcpServers();
    expect(servers).toEqual([config]);
    expect(mockInvoke).toHaveBeenCalledWith("list_mcp_servers", undefined);

    mockOk(config);
    const newConfig = {
      name: config.name,
      transport: config.transport,
      command: config.command,
      args: config.args,
      url: config.url,
      env: config.env,
      enabled: config.enabled,
    };
    const added = await addMcpServer(newConfig);
    expect(added).toEqual(config);
    expect(mockInvoke).toHaveBeenCalledWith("add_mcp_server", { config: newConfig });

    mockOk(null);
    await removeMcpServer(config.id);
    expect(mockInvoke).toHaveBeenCalledWith("remove_mcp_server", { id: config.id });
  });

  it("lists and invokes MCP tools", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const tool = {
      name: "read_file",
      description: "Read a file",
      input_schema: { type: "object" },
    };

    mockOk([tool]);
    const tools = await listMcpTools(1);
    expect(tools).toEqual([tool]);
    expect(mockInvoke).toHaveBeenCalledWith("list_mcp_tools", { server_id: 1 });

    mockOk({ content: "hello" });
    const result = await invokeMcpTool(workspaceId, "read_file", { path: "/tmp/test.txt" }, 1);
    expect(result).toEqual({ content: "hello" });
    expect(mockInvoke).toHaveBeenCalledWith("invoke_mcp_tool", {
      workspace_id: workspaceId,
      server_id: 1,
      name: "read_file",
      arguments: { path: "/tmp/test.txt" },
    });
  });
});

describe("tauri-api phase 9 commands", () => {
  it("lists agents", async () => {
    const agents = [
      {
        name: "SecurityReviewer",
        description: "Reviews code for security issues",
        system_instructions: "You are a security reviewer.",
        allowed_tools: ["read_file"],
        default_model_policy: "default",
        default_permission_scope: "Run",
      },
    ];
    mockOk(agents);
    const result = await listAgents();
    expect(result).toEqual(agents);
    expect(mockInvoke).toHaveBeenCalledWith("list_agents", undefined);
  });

  it("plans subagents", async () => {
    const parentRunId = asAgentRunId("550e8400-e29b-41d4-a716-446655440000");
    const plan = {
      parent_run_id: parentRunId,
      subagents: [],
    };
    mockOk(plan);
    const result = await planSubagents(parentRunId, "review this code");
    expect(result).toEqual(plan);
    expect(mockInvoke).toHaveBeenCalledWith("plan_subagents", {
      parent_run_id: parentRunId,
      task: "review this code",
    });
  });

  it("executes subagents", async () => {
    const parentRunId = asAgentRunId("550e8400-e29b-41d4-a716-446655440000");
    const subagentRunId = asAgentRunId("550e8400-e29b-41d4-a716-446655440001");
    const plan = {
      parent_run_id: parentRunId,
      subagents: [
        {
          id: subagentRunId,
          parent_run_id: parentRunId,
          agent_name: "Reviewer",
          status: "Completed" as const,
          task_description: "Review",
          output_summary: "Looks good" as string | null,
          created_at: "2026-01-01T00:00:00.000Z",
          completed_at: "2026-01-01T00:00:01.000Z" as string | null,
        },
      ],
    };
    mockOk(plan.subagents);
    const result = await executeSubagents(plan);
    expect(result).toEqual(plan.subagents);
    expect(mockInvoke).toHaveBeenCalledWith("execute_subagents", { plan });
  });

  it("cancels a subagent", async () => {
    const subagentRunId = asAgentRunId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(null);
    await cancelSubagent(subagentRunId);
    expect(mockInvoke).toHaveBeenCalledWith("cancel_subagent", { subagent_run_id: subagentRunId });
  });
});

describe("tauri-api phase 10 commands", () => {
  it("lists automations", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const automation = {
      id: asAutomationId("550e8400-e29b-41d4-a716-446655440001"),
      workspace_id: workspaceId,
      name: "Nightly sync",
      description: "Runs every night",
      trigger: "Scheduled" as const,
      cron_expr: "0 2 * * *",
      enabled: true,
      next_run_at: "2026-01-02T02:00:00.000Z",
      last_run_at: null,
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk([automation]);
    const result = await listAutomations(workspaceId);
    expect(result).toEqual([automation]);
    expect(mockInvoke).toHaveBeenCalledWith("list_automations", { workspace_id: workspaceId });
  });

  it("lists automations across workspaces", async () => {
    mockOk([]);
    const result = await listAutomations();
    expect(result).toEqual([]);
    expect(mockInvoke).toHaveBeenCalledWith("list_automations", { workspace_id: null });
  });

  it("creates an automation", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const automation = {
      id: asAutomationId("550e8400-e29b-41d4-a716-446655440001"),
      workspace_id: workspaceId,
      name: "Nightly sync",
      description: "Runs every night",
      trigger: "Scheduled" as const,
      cron_expr: "0 2 * * *",
      enabled: true,
      next_run_at: null,
      last_run_at: null,
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(automation);
    const result = await createAutomation(
      workspaceId,
      "Nightly sync",
      "Runs every night",
      "Scheduled",
      "0 2 * * *",
      true,
    );
    expect(result).toEqual(automation);
    expect(mockInvoke).toHaveBeenCalledWith("create_automation", {
      workspace_id: workspaceId,
      name: "Nightly sync",
      description: "Runs every night",
      trigger: "Scheduled",
      cron_expr: "0 2 * * *",
      enabled: true,
    });
  });

  it("updates an automation", async () => {
    const automation = {
      id: asAutomationId("550e8400-e29b-41d4-a716-446655440001"),
      workspace_id: asWorkspaceId("550e8400-e29b-41d4-a716-446655440000"),
      name: "Nightly sync",
      description: "Runs every night",
      trigger: "Scheduled" as const,
      cron_expr: "0 3 * * *",
      enabled: false,
      next_run_at: null,
      last_run_at: null,
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(automation);
    const result = await updateAutomation(automation);
    expect(result).toEqual(automation);
    expect(mockInvoke).toHaveBeenCalledWith("update_automation", { automation });
  });

  it("deletes an automation", async () => {
    const id = asAutomationId("550e8400-e29b-41d4-a716-446655440001");
    mockOk(null);
    await deleteAutomation(id);
    expect(mockInvoke).toHaveBeenCalledWith("delete_automation", { id });
  });

  it("runs an automation now", async () => {
    const id = asAutomationId("550e8400-e29b-41d4-a716-446655440001");
    mockOk(null);
    await runAutomationNow(id);
    expect(mockInvoke).toHaveBeenCalledWith("run_automation_now", { id });
  });

  it("lists notifications", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const notification = {
      id: asNotificationId("550e8400-e29b-41d4-a716-446655440001"),
      workspace_id: workspaceId,
      thread_id: null,
      run_id: null,
      title: "Hello",
      body: "World",
      category: "System" as const,
      read: false,
      created_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk([notification]);
    const result = await listNotifications(workspaceId, true);
    expect(result).toEqual([notification]);
    expect(mockInvoke).toHaveBeenCalledWith("list_notifications", {
      workspace_id: workspaceId,
      unread_only: true,
    });
  });

  it("marks a notification as read", async () => {
    const id = asNotificationId("550e8400-e29b-41d4-a716-446655440001");
    const notification = {
      id,
      workspace_id: null,
      thread_id: null,
      run_id: null,
      title: "Hello",
      body: "World",
      category: "System" as const,
      read: true,
      created_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(notification);
    const result = await markNotificationRead(id);
    expect(result).toEqual(notification);
    expect(mockInvoke).toHaveBeenCalledWith("mark_notification_read", { id });
  });

  it("dismisses a notification", async () => {
    const id = asNotificationId("550e8400-e29b-41d4-a716-446655440001");
    mockOk(null);
    await dismissNotification(id);
    expect(mockInvoke).toHaveBeenCalledWith("dismiss_notification", { id });
  });

  it("sends a test notification", async () => {
    const notification = {
      id: asNotificationId("550e8400-e29b-41d4-a716-446655440001"),
      workspace_id: null,
      thread_id: null,
      run_id: null,
      title: "Test",
      body: "Notification",
      category: "System" as const,
      read: false,
      created_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(notification);
    const result = await sendTestNotification("Test", "Notification");
    expect(result).toEqual(notification);
    expect(mockInvoke).toHaveBeenCalledWith("send_test_notification", {
      title: "Test",
      body: "Notification",
    });
  });

  it("lists background tasks", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const task = {
      id: asBackgroundTaskId("550e8400-e29b-41d4-a716-446655440001"),
      workspace_id: workspaceId,
      thread_id: null,
      run_id: null,
      task_kind: "ScheduledJob" as const,
      payload: null,
      status: "Running" as const,
      priority: 1,
      attempts: 0,
      max_attempts: 3,
      scheduled_at: null,
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk([task]);
    const result = await listBackgroundTasks(workspaceId);
    expect(result).toEqual([task]);
    expect(mockInvoke).toHaveBeenCalledWith("list_background_tasks", { workspace_id: workspaceId });
  });

  it("cancels a background task", async () => {
    const id = asBackgroundTaskId("550e8400-e29b-41d4-a716-446655440001");
    mockOk(null);
    await cancelBackgroundTask(id);
    expect(mockInvoke).toHaveBeenCalledWith("cancel_background_task", { id });
  });
});

describe("tauri-api phase 11 commands", () => {
  const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");

  it("opens and lists browser windows", async () => {
    const info = {
      id: asBrowserWindowId("win-1"),
      url: "https://example.com",
      title: "Example",
    };

    mockOk(info);
    const opened = await openBrowserWindow(workspaceId, "https://example.com", "Example");
    expect(opened).toEqual(info);
    expect(mockInvoke).toHaveBeenCalledWith("open_browser_window", {
      workspace_id: workspaceId,
      url: "https://example.com",
      title: "Example",
    });

    mockOk([info]);
    const list = await listBrowserWindows();
    expect(list).toEqual([info]);
    expect(mockInvoke).toHaveBeenCalledWith("list_browser_windows", undefined);
  });

  it("closes a browser window", async () => {
    const id = asBrowserWindowId("win-1");
    mockOk(null);
    await closeBrowserWindow(workspaceId, id);
    expect(mockInvoke).toHaveBeenCalledWith("close_browser_window", {
      workspace_id: workspaceId,
      id,
    });
  });

  it("sends browser use actions", async () => {
    const id = asBrowserWindowId("win-1");
    const actions = [
      { kind: "Click" as const, selector: "#btn" },
      { kind: "Type" as const, selector: "#input", text: "hello" },
      { kind: "ExtractVisibleText" as const },
      { kind: "Wait" as const, ms: 1000 },
      { kind: "Screenshot" as const },
    ] satisfies BrowserAction[];

    for (const action of actions) {
      mockOk("ok");
      const result = await browserUseAction(workspaceId, id, action);
      expect(result).toBe("ok");
      expect(mockInvoke).toHaveBeenCalledWith("browser_use_action", {
        workspace_id: workspaceId,
        id,
        action,
      });
    }
  });

  it("captures the screen", async () => {
    const capture = {
      image_base64: "base64image",
      width: 1920,
      height: 1080,
    };
    mockOk(capture);
    const result = await captureScreen(workspaceId);
    expect(result).toEqual(capture);
    expect(mockInvoke).toHaveBeenCalledWith("capture_screen", { workspace_id: workspaceId });
  });

  it("controls desktop input", async () => {
    mockOk(null);
    await moveMouse(workspaceId, 100, 200);
    expect(mockInvoke).toHaveBeenCalledWith("move_mouse", {
      workspace_id: workspaceId,
      x: 100,
      y: 200,
    });

    mockOk(null);
    await clickMouse(workspaceId);
    expect(mockInvoke).toHaveBeenCalledWith("click_mouse", { workspace_id: workspaceId });

    mockOk(null);
    await typeText(workspaceId, "hello world");
    expect(mockInvoke).toHaveBeenCalledWith("type_text", {
      workspace_id: workspaceId,
      text: "hello world",
    });

    mockOk(null);
    await focusApp(workspaceId, "Finder");
    expect(mockInvoke).toHaveBeenCalledWith("focus_app", {
      workspace_id: workspaceId,
      name: "Finder",
    });
  });

  it("previews an artifact", async () => {
    const preview = {
      path: "/tmp/report.pdf",
      mime_type: "application/pdf",
      content_base64: "base64pdf",
      size_bytes: 4096,
    };
    mockOk(preview);
    const result = await previewArtifact(workspaceId, "/tmp/report.pdf");
    expect(result).toEqual(preview);
    expect(mockInvoke).toHaveBeenCalledWith("preview_artifact", {
      workspace_id: workspaceId,
      path: "/tmp/report.pdf",
    });
  });
});

describe("tauri-api phase 12 commands", () => {
  it("collects and uploads a diagnostics bundle", async () => {
    const bundle = {
      id: asDiagnosticsBundleId("bundle-1"),
      created_at: "2026-01-01T00:00:00.000Z",
      log_path: "/tmp/diagnostics/bundle.zip",
      audit_summary_path: "/tmp/diagnostics/audit.json",
      app_version: "0.1.0",
      os_info: "macOS 15.0",
      redacted: true,
      size_bytes: 10240,
    };

    mockOk(bundle);
    const collected = await collectDiagnosticsBundle();
    expect(collected).toEqual(bundle);
    expect(mockInvoke).toHaveBeenCalledWith("collect_diagnostics_bundle", undefined);

    mockOk(null);
    await uploadDiagnosticsBundle(bundle.id);
    expect(mockInvoke).toHaveBeenCalledWith("upload_diagnostics_bundle", {
      bundle_id: bundle.id,
    });
  });

  it("lists migrations", async () => {
    const migrations = [
      {
        version: 1,
        name: "initial_schema",
        applied_at: "2026-01-01T00:00:00.000Z",
        checksum: "sha256:abc123",
      },
    ];
    mockOk(migrations);
    const result = await listMigrations();
    expect(result).toEqual(migrations);
    expect(mockInvoke).toHaveBeenCalledWith("list_migrations", undefined);
  });

  it("rolls back the last migration", async () => {
    mockOk(null);
    await rollbackLastMigration();
    expect(mockInvoke).toHaveBeenCalledWith("rollback_last_migration", undefined);
  });
});
