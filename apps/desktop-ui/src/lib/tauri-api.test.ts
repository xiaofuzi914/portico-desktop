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
  clickMouse,
  closeBrowserWindow,
  collectDiagnosticsBundle,
  createAutomation,
  createMemory,
  createModel,
  createProvider,
  createTerminal,
  createThread,
  createWorkspace,
  createWorktree,
  deleteAutomation,
  deleteMemory,
  deleteProviderSecret,
  deleteThread,
  deleteWorktree,
  dismissNotification,
  enablePlugin,
  executeTerminalCommand,
  focusApp,
  getActiveModel,
  getProviderHealth,
  getRunModelSnapshot,
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
  installPluginPackage,
  invokeCommand,
  invokeMcpTool,
  listAgents,
  listAutomations,
  listBackgroundTasks,
  listBrowserWindows,
  listMcpServers,
  listMessages,
  listRuns,
  listMcpTools,
  listMemories,
  listMigrations,
  listNotifications,
  listPendingApprovals,
  listPlugins,
  listModels,
  listSkills,
  listThreads,
  listWorktrees,
  listWorkspaceFiles,
  openWorkspaceFolder,
  previewWorkspaceMarkdown,
  loadInstructions,
  markNotificationRead,
  moveMouse,
  openBrowserWindow,
  startOrchestration,
  previewArtifact,
  readTerminalHistory,
  rebuildRagIndex,
  refreshMcpTools,
  removeMcpServer,
  rollbackLastMigration,
  runAutomationNow,
  searchRag,
  setActiveModel,
  sendMessage,
  sendTestNotification,
  setProviderSecret,
  setWorkspacePaths,
  startRun,
  submitMessage,
  testProviderConnection,
  trustWorkspace,
  typeText,
  uninstallPlugin,
  updateAutomation,
  updateMemory,
} from "./tauri-api";
import {
  asAgentRunId,
  asAutomationId,
  asBackgroundTaskId,
  asBrowserWindowId,
  asDiagnosticsBundleId,
  asMemoryId,
  asModelId,
  asNotificationId,
  asProviderId,
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

describe("tauri-api transport contract", () => {
  it("unwraps successful payloads and accepts null data for void commands", async () => {
    mockOk("hello");
    await expect(invokeCommand<string>("greet", { name: "Portico" })).resolves.toBe("hello");

    mockOk(null);
    // Rust unit responses serialize as data:null; treat as successful void.
    await expect(invokeCommand<void>("save", { value: "ok" })).resolves.toBeUndefined();
  });

  it("surfaces envelope failures and transport rejections", async () => {
    mockErr("permission denied");
    await expect(invokeCommand<void>("write_file")).rejects.toThrow("permission denied");

    mockInvoke.mockRejectedValueOnce(new Error("bridge unavailable"));
    await expect(invokeCommand<void>("write_file")).rejects.toThrow("bridge unavailable");
  });

  it("uses camelCase for the workspace-thread-run golden path", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const threadId = asThreadId("550e8400-e29b-41d4-a716-446655440001");
    const runId = asAgentRunId("550e8400-e29b-41d4-a716-446655440002");

    mockOk(null);
    await setWorkspacePaths(workspaceId, ["/read"], ["/write"]);
    expect(mockInvoke).toHaveBeenLastCalledWith("set_workspace_paths", {
      id: workspaceId,
      readPaths: ["/read"],
      writePaths: ["/write"],
    });

    mockOk({});
    await startRun(workspaceId, threadId);
    expect(mockInvoke).toHaveBeenLastCalledWith("start_run", { workspaceId, threadId });

    mockOk(null);
    await submitMessage(runId, "hello");
    expect(mockInvoke).toHaveBeenLastCalledWith("submit_message", { runId, content: "hello" });

    mockOk({});
    await sendMessage(threadId, "second turn", "request-2");
    expect(mockInvoke).toHaveBeenLastCalledWith("send_message", {
      threadId,
      content: "second turn",
      clientRequestId: "request-2",
    });

    mockOk([]);
    await listMessages(threadId);
    expect(mockInvoke).toHaveBeenLastCalledWith("list_messages", { threadId });

    mockOk([]);
    await listRuns(threadId);
    expect(mockInvoke).toHaveBeenLastCalledWith("list_runs", { threadId });

    mockOk([]);
    await listPendingApprovals(runId);
    expect(mockInvoke).toHaveBeenLastCalledWith("list_pending_approvals", { runId });
  });
});

describe("tauri-api phase 6 commands", () => {
  it("uses Tauri camelCase argument names for session commands", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");

    mockOk({});
    await createThread(workspaceId, "新会话");
    expect(mockInvoke).toHaveBeenCalledWith("create_thread", {
      workspaceId,
      title: "新会话",
    });

    mockOk([]);
    await listThreads(workspaceId);
    expect(mockInvoke).toHaveBeenCalledWith("list_threads", { workspaceId });

    const threadId = asThreadId("550e8400-e29b-41d4-a716-446655440001");
    mockOk(null);
    await deleteThread(workspaceId, threadId);
    expect(mockInvoke).toHaveBeenCalledWith("delete_thread", { workspaceId, id: threadId });
  });

  it("lists files inside a workspace directory", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk([]);

    await listWorkspaceFiles(workspaceId, "src");

    expect(mockInvoke).toHaveBeenCalledWith("list_workspace_files", {
      id: workspaceId,
      relativePath: "src",
    });
  });

  it("opens a workspace folder in the OS file manager", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(null);

    await openWorkspaceFolder(workspaceId, "src");

    expect(mockInvoke).toHaveBeenCalledWith("open_workspace_folder", {
      id: workspaceId,
      relativePath: "src",
    });
  });

  it("previews a Markdown file inside a workspace", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const preview = {
      path: "/tmp/README.md",
      mime_type: "text/markdown",
      content_base64: "IyBIZWxsbw==",
      size_bytes: 7,
    };
    mockOk(preview);

    await expect(previewWorkspaceMarkdown(workspaceId, "README.md")).resolves.toEqual(preview);
    expect(mockInvoke).toHaveBeenCalledWith("preview_workspace_markdown", {
      id: workspaceId,
      relativePath: "README.md",
    });
  });

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

    const result = await createWorkspace("Test", "/tmp/test");

    expect(result).toEqual(workspace);
    expect(mockInvoke).toHaveBeenCalledWith("create_workspace", {
      name: "Test",
      rootPath: "/tmp/test",
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
      readPaths: ["/read"],
      writePaths: ["/write"],
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
      workspaceId: worktree.workspace_id,
      threadId: worktree.thread_id,
      name: "feature",
    });
  });

  it("lists worktrees", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk([]);
    const result = await listWorktrees(workspaceId);
    expect(result).toEqual([]);
    expect(mockInvoke).toHaveBeenCalledWith("list_worktrees", { workspaceId });
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
      workspaceId,
      repoPath: "/repo",
    });
  });

  it("fetches git diff", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk("diff --git");
    const result = await gitDiff(workspaceId, "/repo");
    expect(result).toBe("diff --git");
    expect(mockInvoke).toHaveBeenCalledWith("git_diff", {
      workspaceId,
      repoPath: "/repo",
    });
  });

  it("stages and unstages files", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(null);
    await gitStage(workspaceId, "/repo", ["src/main.rs"]);
    expect(mockInvoke).toHaveBeenCalledWith("git_stage", {
      workspaceId,
      repoPath: "/repo",
      paths: ["src/main.rs"],
    });

    mockOk(null);
    await gitUnstage(workspaceId, "/repo", ["src/main.rs"]);
    expect(mockInvoke).toHaveBeenCalledWith("git_unstage", {
      workspaceId,
      repoPath: "/repo",
      paths: ["src/main.rs"],
    });
  });

  it("commits with a message", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk("abc123");
    const result = await gitCommit(workspaceId, "/repo", "Initial commit");
    expect(result).toBe("abc123");
    expect(mockInvoke).toHaveBeenCalledWith("git_commit", {
      workspaceId,
      repoPath: "/repo",
      message: "Initial commit",
    });
  });

  it("creates and lists branches", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk("Switched to branch 'feature'");
    const result = await gitBranch(workspaceId, "/repo", "feature");
    expect(result).toBe("Switched to branch 'feature'");
    expect(mockInvoke).toHaveBeenCalledWith("git_branch", {
      workspaceId,
      repoPath: "/repo",
      name: "feature",
    });

    mockOk("main");
    const current = await gitBranch(workspaceId, "/repo");
    expect(current).toBe("main");
    expect(mockInvoke).toHaveBeenCalledWith("git_branch", {
      workspaceId,
      repoPath: "/repo",
      name: null,
    });
  });

  it("pushes to default remote", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk("Everything up-to-date");
    const result = await gitPush(workspaceId, "/repo");
    expect(result).toBe("Everything up-to-date");
    expect(mockInvoke).toHaveBeenCalledWith("git_push", {
      workspaceId,
      repoPath: "/repo",
      remote: null,
    });
  });

  it("creates a terminal session", async () => {
    const id = asTerminalId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(id);
    const result = await createTerminal(asThreadId("550e8400-e29b-41d4-a716-446655440001"));
    expect(result).toBe(id);
    expect(mockInvoke).toHaveBeenCalledWith("create_terminal", {
      threadId: "550e8400-e29b-41d4-a716-446655440001",
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
      workspaceId,
      threadId: null,
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
      workspaceId,
      threadId: null,
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
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440002");
    mockOk([{ path: "/tmp/AGENTS.md", content: "# Instructions", scope: "workspace" }]);
    const result = await loadInstructions(workspaceId);
    expect(result).toEqual([
      { path: "/tmp/AGENTS.md", content: "# Instructions", scope: "workspace" },
    ]);
    expect(mockInvoke).toHaveBeenCalledWith("load_instructions", { workspaceId });
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
    const result = await inspectContext(runId, threadId, workspaceId, "test");
    expect(result).toEqual(summary);
    expect(mockInvoke).toHaveBeenCalledWith("inspect_context", {
      runId,
      threadId,
      workspaceId,
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
      workspaceId,
      query: "hello",
      topN: 3,
    });
  });
});

describe("tauri-api model registry commands", () => {
  it("uses Tauri camelCase argument names for providers", async () => {
    mockOk({});

    await createProvider("DeepSeek", "DeepSeek", "https://api.deepseek.com", "deepseek-default");

    expect(mockInvoke).toHaveBeenCalledWith("create_provider", {
      kind: "DeepSeek",
      displayName: "DeepSeek",
      baseUrl: "https://api.deepseek.com",
      apiKeyReference: "deepseek-default",
    });
  });

  it("uses Tauri camelCase argument names for model commands", async () => {
    const providerId = asProviderId("550e8400-e29b-41d4-a716-446655440000");
    const capabilities = {
      supports_streaming: true,
      supports_tools: true,
      supports_json_schema: false,
      supports_vision: false,
      supports_pdf: false,
      supports_system_prompt: true,
      supports_embeddings: false,
      max_context_tokens: 64_000,
      input_price_per_1k: null,
      output_price_per_1k: null,
    };

    mockOk([]);
    await listModels(providerId);
    expect(mockInvoke).toHaveBeenCalledWith("list_models", { providerId });

    mockOk({});
    await createModel(providerId, "deepseek-chat", "DeepSeek Chat", capabilities);
    expect(mockInvoke).toHaveBeenCalledWith("create_model", {
      providerId,
      modelName: "deepseek-chat",
      displayName: "DeepSeek Chat",
      capabilities,
    });
  });

  it("selects, probes, and snapshots the active model with camelCase arguments", async () => {
    const providerId = asProviderId("550e8400-e29b-41d4-a716-446655440000");
    const modelId = asModelId("550e8400-e29b-41d4-a716-446655440001");
    const runId = asAgentRunId("550e8400-e29b-41d4-a716-446655440002");

    mockOk({});
    await setActiveModel("Global", null, null, providerId, modelId);
    expect(mockInvoke).toHaveBeenCalledWith("set_active_model", {
      scope: "Global",
      workspaceId: null,
      threadId: null,
      providerId,
      modelId,
    });

    mockOk(null);
    await getActiveModel("Global");
    expect(mockInvoke).toHaveBeenCalledWith("get_active_model", {
      scope: "Global",
      workspaceId: null,
      threadId: null,
    });

    mockOk({ status: "Ready" });
    await testProviderConnection(providerId, modelId);
    expect(mockInvoke).toHaveBeenCalledWith("test_provider_connection", {
      providerId,
      modelId,
    });

    mockOk(null);
    await getProviderHealth(providerId, modelId);
    expect(mockInvoke).toHaveBeenCalledWith("get_provider_health", {
      providerId,
      modelId,
    });

    mockOk(null);
    await getRunModelSnapshot(runId);
    expect(mockInvoke).toHaveBeenCalledWith("get_run_model_snapshot", { runId });
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
      entrypoint: null,
      capabilities: [],
      install_path: null,
      permissions: { network: [], filesystem: "none" as const },
      enabled: true,
      installed_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(manifest);
    const installed = await installPlugin(manifest);
    expect(installed).toEqual(manifest);
    expect(mockInvoke).toHaveBeenCalledWith("install_plugin", { manifest });

    mockOk(manifest);
    await expect(installPluginPackage("/tmp/provider")).resolves.toEqual(manifest);
    expect(mockInvoke).toHaveBeenCalledWith("install_plugin_package", {
      sourcePath: "/tmp/provider",
    });

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

  it("lists skills as catalog metadata", async () => {
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
    expect(mockInvoke).toHaveBeenCalledWith("list_skills", { pluginId });
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
    expect(mockInvoke).toHaveBeenCalledWith("list_mcp_tools", { serverId: 1 });

    mockOk({ content: "hello" });
    const result = await invokeMcpTool(workspaceId, "read_file", { path: "/tmp/test.txt" }, 1);
    expect(result).toEqual({ content: "hello" });
    expect(mockInvoke).toHaveBeenCalledWith("invoke_mcp_tool", {
      workspaceId,
      serverId: 1,
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

  it("starts multi-agent orchestration", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const threadId = asThreadId("550e8400-e29b-41d4-a716-446655440001");
    const parentRunId = asAgentRunId("550e8400-e29b-41d4-a716-446655440002");
    const orchestration = {
      id: "550e8400-e29b-41d4-a716-446655440003",
      parent_run_id: parentRunId,
      workspace_id: workspaceId,
      thread_id: threadId,
      task: "review this code",
      status: "Completed",
      plan: {
        parent_run_id: parentRunId,
        subagents: [],
        pattern_ids: [],
        planning_rationale: "test",
      },
      pattern_ids: [],
      result_summary: "done",
      created_at: "2026-01-01T00:00:00.000Z",
      updated_at: "2026-01-01T00:00:00.000Z",
      completed_at: "2026-01-01T00:00:00.000Z",
    };
    mockOk(orchestration);
    const result = await startOrchestration(workspaceId, threadId, "review this code");
    expect(result).toEqual(orchestration);
    expect(mockInvoke).toHaveBeenCalledWith("start_orchestration", {
      workspaceId,
      threadId,
      task: "review this code",
    });
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
    expect(mockInvoke).toHaveBeenCalledWith("list_automations", { workspaceId });
  });

  it("lists automations across workspaces", async () => {
    mockOk([]);
    const result = await listAutomations();
    expect(result).toEqual([]);
    expect(mockInvoke).toHaveBeenCalledWith("list_automations", { workspaceId: null });
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
      workspaceId,
      name: "Nightly sync",
      description: "Runs every night",
      trigger: "Scheduled",
      cronExpr: "0 2 * * *",
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
      workspaceId,
      unreadOnly: true,
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
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    const result = await sendTestNotification(workspaceId, "Test", "Notification");
    expect(result).toEqual(notification);
    expect(mockInvoke).toHaveBeenCalledWith("send_test_notification", {
      workspaceId,
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
    expect(mockInvoke).toHaveBeenCalledWith("list_background_tasks", { workspaceId });
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
      workspaceId,
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
      workspaceId,
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
        workspaceId,
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
    expect(mockInvoke).toHaveBeenCalledWith("capture_screen", { workspaceId });
  });

  it("controls desktop input", async () => {
    mockOk(null);
    await moveMouse(workspaceId, 100, 200);
    expect(mockInvoke).toHaveBeenCalledWith("move_mouse", {
      workspaceId,
      x: 100,
      y: 200,
    });

    mockOk(null);
    await clickMouse(workspaceId);
    expect(mockInvoke).toHaveBeenCalledWith("click_mouse", { workspaceId });

    mockOk(null);
    await typeText(workspaceId, "hello world");
    expect(mockInvoke).toHaveBeenCalledWith("type_text", {
      workspaceId,
      text: "hello world",
    });

    mockOk(null);
    await focusApp(workspaceId, "Finder");
    expect(mockInvoke).toHaveBeenCalledWith("focus_app", {
      workspaceId,
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
      workspaceId,
      path: "/tmp/report.pdf",
    });
  });
});

describe("tauri-api phase 12 commands", () => {
  it("collects a diagnostics bundle without exposing a fake upload command", async () => {
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

describe("tauri-api provider secret commands", () => {
  it("sets a provider secret", async () => {
    mockOk(null);
    await setProviderSecret("openai-default", "sk-secret");
    expect(mockInvoke).toHaveBeenCalledWith("set_provider_secret", {
      apiKeyReference: "openai-default",
      apiKey: "sk-secret",
    });
  });

  it("deletes a provider secret", async () => {
    mockOk(null);
    await deleteProviderSecret("openai-default");
    expect(mockInvoke).toHaveBeenCalledWith("delete_provider_secret", {
      apiKeyReference: "openai-default",
    });
  });
});

describe("tauri-api refresh and rebuild commands", () => {
  it("refreshes MCP tools", async () => {
    mockOk(null);
    await refreshMcpTools();
    expect(mockInvoke).toHaveBeenCalledWith("refresh_mcp_tools", undefined);
  });

  it("rebuilds the RAG index for a workspace", async () => {
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
    mockOk(12);
    const count = await rebuildRagIndex(workspaceId);
    expect(count).toBe(12);
    expect(mockInvoke).toHaveBeenCalledWith("rebuild_rag_index", {
      workspaceId,
    });
  });
});
