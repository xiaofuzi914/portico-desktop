import { describe, expect, it } from "vitest";
import { mapRunEventToBlock, mergeRunEvents } from "./event-view-models";
import type { AgentRunId, RunEvent, RuntimeEvent } from "@/lib/schemas";

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
