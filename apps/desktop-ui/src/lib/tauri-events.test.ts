import { describe, expect, it, vi } from "vitest";
import {
  RuntimeEventStore,
  isWorkspaceMutatingTool,
  shouldRefreshWorkspaceFiles,
} from "./tauri-events";
import type { AgentRunId, RuntimeEvent } from "./schemas";

function statusEvent(runId: AgentRunId): RuntimeEvent {
  return {
    kind: "RunStatusChanged",
    data: { run_id: runId, status: "Running" },
  };
}

describe("RuntimeEventStore", () => {
  it("stores events per run", () => {
    const store = new RuntimeEventStore();
    const runA = "run-a" as AgentRunId;
    const runB = "run-b" as AgentRunId;

    store.addEvent(statusEvent(runA));
    store.addEvent(statusEvent(runA));
    store.addEvent(statusEvent(runB));

    expect(store.getEvents(runA)).toHaveLength(2);
    expect(store.getEvents(runB)).toHaveLength(1);
    expect(store.getEvents()).toHaveLength(3);
  });

  it("notifies subscribers", () => {
    const store = new RuntimeEventStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    const first = statusEvent("run-1" as AgentRunId);
    store.addEvent(first);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenLastCalledWith(first);

    store.clearEvents();
    expect(listener).toHaveBeenCalledTimes(2);

    unsubscribe();
    store.addEvent(statusEvent("run-2" as AgentRunId));
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("detects workspace-mutating tools for folder refresh", () => {
    expect(isWorkspaceMutatingTool("fs_write")).toBe(true);
    expect(isWorkspaceMutatingTool("fs_read")).toBe(false);
    expect(
      shouldRefreshWorkspaceFiles({
        kind: "ToolCompleted",
        data: { run_id: "r" as AgentRunId, tool_name: "fs_write", result: {} },
      }),
    ).toBe(true);
    expect(
      shouldRefreshWorkspaceFiles({
        kind: "ToolCompleted",
        data: { run_id: "r" as AgentRunId, tool_name: "fs_read", result: {} },
      }),
    ).toBe(false);
    expect(
      shouldRefreshWorkspaceFiles({
        kind: "RunCompleted",
        data: { run_id: "r" as AgentRunId },
      }),
    ).toBe(true);
  });

  it("clears a single run", () => {
    const store = new RuntimeEventStore();
    const runA = "run-a" as AgentRunId;
    const runB = "run-b" as AgentRunId;

    store.addEvent(statusEvent(runA));
    store.addEvent(statusEvent(runB));
    store.clearRun(runA);

    expect(store.getEvents(runA)).toHaveLength(0);
    expect(store.getEvents(runB)).toHaveLength(1);
  });

  it("keeps the most recent 8 runs and evicts the oldest", () => {
    const store = new RuntimeEventStore();
    const runIds = Array.from({ length: 9 }, (_, i) => String(i + 1).padStart(2, "0")).map(
      (id) => `run-${id}` as AgentRunId,
    );

    for (const runId of runIds) {
      store.addEvent(statusEvent(runId));
    }

    expect(store.getEvents(runIds[0])).toHaveLength(0);
    for (let i = 1; i < runIds.length; i++) {
      expect(store.getEvents(runIds[i])).toHaveLength(1);
    }
    expect(store.getEvents()).toHaveLength(8);
  });

  it("updates access order when an existing run receives an event", () => {
    const store = new RuntimeEventStore();
    const first = "run-01" as AgentRunId;
    const others = Array.from(
      { length: 7 },
      (_, i) => `run-${String(i + 2).padStart(2, "0")}` as AgentRunId,
    );

    store.addEvent(statusEvent(first));
    for (const runId of others) {
      store.addEvent(statusEvent(runId));
    }

    // Touch the oldest run so it becomes the most recently used.
    store.addEvent(statusEvent(first));

    const next = "run-new" as AgentRunId;
    store.addEvent(statusEvent(next));

    // The second-oldest run (run-02) should have been evicted instead of first.
    expect(store.getEvents(first)).toHaveLength(2);
    expect(store.getEvents(others[0])).toHaveLength(0);
  });
});
