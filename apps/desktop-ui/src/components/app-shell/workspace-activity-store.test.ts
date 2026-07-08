import { beforeEach, describe, expect, it } from "vitest";
import { asAgentRunId, asWorkspaceId } from "@/lib/schemas";
import { readRunningWorkspaceIds, updateWorkspaceRunActivity } from "./workspace-activity-store";

describe("workspace activity store", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("tracks workspaces with active runs", () => {
    const runId = asAgentRunId("550e8400-e29b-41d4-a716-446655440000");
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440001");

    updateWorkspaceRunActivity(runId, workspaceId, "Running");

    expect(readRunningWorkspaceIds().has(workspaceId)).toBe(true);
  });

  it("removes a workspace run when it reaches a terminal status", () => {
    const runId = asAgentRunId("550e8400-e29b-41d4-a716-446655440000");
    const workspaceId = asWorkspaceId("550e8400-e29b-41d4-a716-446655440001");

    updateWorkspaceRunActivity(runId, workspaceId, "Running");
    updateWorkspaceRunActivity(runId, workspaceId, "Completed");

    expect(readRunningWorkspaceIds().has(workspaceId)).toBe(false);
  });
});
