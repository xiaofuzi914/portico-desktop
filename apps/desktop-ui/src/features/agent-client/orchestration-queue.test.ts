import { describe, expect, it } from "vitest";
import {
  queueMultiRoleTask,
  queueSendTask,
  resolveQueuedOrchestrationStart,
} from "./orchestration-queue";

describe("orchestration-queue", () => {
  it("preserves catalog workflowId when queueing multi-role starts", () => {
    const item = queueMultiRoleTask(
      "  review auth module  ",
      "multi-lens-review",
      "q1",
    );
    expect(item.mode).toBe("multi-role");
    expect(item.content).toBe("review auth module");
    expect(item.workflowId).toBe("multi-lens-review");

    const start = resolveQueuedOrchestrationStart(item);
    expect(start).toEqual({
      task: "review auth module",
      workflowId: "multi-lens-review",
    });
  });

  it("preserves template UUID for DAG-edited starts", () => {
    const uuid = "550e8400-e29b-41d4-a716-4466554400aa";
    const item = queueMultiRoleTask("iterate on the design", uuid, "q2");
    const start = resolveQueuedOrchestrationStart(item);
    expect(start?.workflowId).toBe(uuid);
  });

  it("keeps adaptive starts as null workflowId without dropping mode", () => {
    const item = queueMultiRoleTask("general multi-role task", null, "q3");
    expect(resolveQueuedOrchestrationStart(item)).toEqual({
      task: "general multi-role task",
      workflowId: null,
    });
  });

  it("does not resolve send queue items as orchestration starts", () => {
    const send = queueSendTask("hello agent", "s1");
    expect(send.workflowId).toBeNull();
    expect(resolveQueuedOrchestrationStart(send)).toBeNull();
  });
});
