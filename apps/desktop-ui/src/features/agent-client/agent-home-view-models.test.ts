import { describe, expect, it } from "vitest";
import { buildAgentHomeViewModel } from "./agent-home-view-models";
import type { Thread, Workspace } from "@/lib/schemas";
import { asThreadId, asWorkspaceId } from "@/lib/schemas";

const workspace = {
  id: asWorkspaceId("workspace-1"),
  name: "Portico",
  root_path: "/repo/portico",
  trusted: true,
  allowed_read_paths: [],
  allowed_write_paths: [],
  created_at: "2026-07-07T00:00:00.000Z",
  updated_at: "2026-07-07T00:00:00.000Z",
} satisfies Workspace;

const olderThread = {
  id: asThreadId("thread-old"),
  workspace_id: asWorkspaceId("workspace-1"),
  title: "Older thread",
  created_at: "2026-07-07T00:00:00.000Z",
  updated_at: "2026-07-07T01:00:00.000Z",
} satisfies Thread;

const newerThread = {
  ...olderThread,
  id: asThreadId("thread-new"),
  title: "Newer thread",
  updated_at: "2026-07-07T02:00:00.000Z",
} satisfies Thread;

describe("agent home view models", () => {
  it("returns a focused empty state when no projects exist", () => {
    expect(buildAgentHomeViewModel([], [])).toEqual({
      mode: "empty",
      activeWorkspace: undefined,
      recentThreads: [],
      primaryAction: "create-project",
    });
  });

  it("prioritizes the selected workspace and newest threads without mutating inputs", () => {
    const threads = [olderThread, newerThread];
    const viewModel = buildAgentHomeViewModel([workspace], threads, "workspace-1");

    expect(viewModel.mode).toBe("workspace");
    expect(viewModel.activeWorkspace?.id).toBe("workspace-1");
    expect(viewModel.recentThreads.map((thread) => thread.id)).toEqual(["thread-new", "thread-old"]);
    expect(viewModel.primaryAction).toBe("continue-thread");
    expect(threads.map((thread) => thread.id)).toEqual(["thread-old", "thread-new"]);
  });

  it("nudges toward thread creation when a project has no threads", () => {
    const viewModel = buildAgentHomeViewModel([workspace], [], "workspace-1");

    expect(viewModel.mode).toBe("workspace");
    expect(viewModel.primaryAction).toBe("create-thread");
  });
});
