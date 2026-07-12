import { describe, expect, it } from "vitest";
import {
  mapMessageToBlock,
  mapRunEventToBlock,
  mergeRunEvents,
  runtimeEventToRunEvent,
} from "./event-view-models";
import type { AgentRunId, Message, RunEvent, RuntimeEvent } from "@/lib/schemas";

const baseEvent: RunEvent = {
  id: 1,
  run_id: "run-1" as RunEvent["run_id"],
  thread_id: "thread-1" as RunEvent["thread_id"],
  sequence: 1,
  event_type: "Message",
  payload: { role: "assistant", content: "Hello" },
  created_at: "2026-07-07T00:00:00.000Z",
};

describe("event view models", () => {
  it("maps message events into conversation blocks", () => {
    expect(mapRunEventToBlock(baseEvent)).toEqual({
      id: "event-1",
      sequence: 1,
      kind: "message",
      title: "Assistant",
      body: "Hello",
      tone: "default",
      createdAt: "2026-07-07T00:00:00.000Z",
      raw: baseEvent,
    });
  });

  it("maps unknown events into diagnostic blocks", () => {
    const event = { ...baseEvent, id: 2, event_type: "SomethingNew", payload: { value: 42 } };

    expect(mapRunEventToBlock(event)).toMatchObject({
      id: "event-2",
      kind: "diagnostic",
      title: "SomethingNew",
      tone: "muted",
    });
  });

  it.each([
    ["ProviderError", "error", "danger", "Error"],
    ["ToolCompleted", "tool", "muted", "Tool Call"],
    ["ApprovalRequested", "approval", "warning", "Approval Required"],
    ["ArtifactCreated", "artifact", "success", "Artifact"],
    ["RunCompleted", "status", "success", "RunCompleted"],
    ["RunStarted", "status", "muted", "RunStarted"],
  ])("maps %s into a %s block", (eventType, kind, tone, title) => {
    expect(
      mapRunEventToBlock({
        ...baseEvent,
        event_type: eventType,
        payload: { message: "details" },
      }),
    ).toMatchObject({ kind, tone, title, body: "details" });
  });

  it("uses safe fallbacks for malformed message payloads", () => {
    expect(mapRunEventToBlock({ ...baseEvent, payload: null })).toMatchObject({
      title: "Assistant",
      body: "null",
    });
    expect(mapRunEventToBlock({ ...baseEvent, payload: "plain text" })).toMatchObject({
      body: "plain text",
    });
  });

  it("renders durable system failures as errors", () => {
    const message: Message = {
      id: "message-1" as Message["id"],
      thread_id: "thread-1" as Message["thread_id"],
      run_id: "run-1" as Message["run_id"],
      role: "System",
      content: "Run failed: No model provider is available.",
      client_request_id: null,
      created_at: "2026-07-07T00:00:00.000Z",
    };

    expect(mapMessageToBlock(message)).toMatchObject({
      kind: "error",
      title: "Run failed",
      tone: "danger",
    });
  });

  it.each([
    ["User", "You", "default"],
    ["Assistant", "Assistant", "muted"],
  ] as const)("maps a %s durable message", (role, title, tone) => {
    const message: Message = {
      id: "message-2" as Message["id"],
      thread_id: "thread-1" as Message["thread_id"],
      run_id: null,
      role,
      content: "content",
      client_request_id: null,
      created_at: "2026-07-07T00:00:00.000Z",
    };

    expect(mapMessageToBlock(message)).toMatchObject({ title, tone, body: "content" });
  });

  it("extracts run metadata from direct and nested runtime events", () => {
    const direct = runtimeEventToRunEvent(
      { kind: "MessageDelta", data: { run_id: "run-direct" } } as RuntimeEvent,
      4,
    );
    expect(direct).toMatchObject({ run_id: "run-direct", thread_id: "unknown", id: -4 });

    const nested = runtimeEventToRunEvent(
      {
        kind: "RunStarted",
        data: {
          run: {
            id: "run-nested",
            thread_id: "thread-nested",
            created_at: "2026-07-07T01:00:00.000Z",
          },
        },
      } as RuntimeEvent,
      5,
    );
    expect(nested).toMatchObject({
      run_id: "run-nested",
      thread_id: "thread-nested",
      created_at: "2026-07-07T01:00:00.000Z",
    });
  });

  it("merges persisted and live events in sequence order without mutating inputs", () => {
    const persisted = [baseEvent];
    const live: RuntimeEvent[] = [
      {
        kind: "RunCompleted",
        data: { run_id: "run-1" as AgentRunId },
      } as RuntimeEvent,
    ];

    const merged = mergeRunEvents(persisted, live);

    expect(merged).toHaveLength(2);
    expect(merged[0]?.id).toBe(1);
    expect(merged[1]?.event_type).toBe("RunCompleted");
    expect(persisted).toHaveLength(1);
  });
});
