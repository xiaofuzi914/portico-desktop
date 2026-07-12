import type { AgentRunId, Message, RunEvent, RuntimeEvent, ThreadId } from "@/lib/schemas";

export type ConversationBlockKind =
  "message" | "tool" | "approval" | "artifact" | "status" | "error" | "diagnostic";

export type ConversationBlockTone = "default" | "muted" | "success" | "warning" | "danger";

export interface ConversationBlock {
  id: string;
  sequence: number;
  kind: ConversationBlockKind;
  title: string;
  body: string;
  tone: ConversationBlockTone;
  createdAt: string;
  raw: RunEvent;
}

export function mapMessageToBlock(message: Message): ConversationBlock {
  const isUser = message.role === "User";
  const isSystem = message.role === "System";
  return {
    id: `message-${message.id}`,
    sequence: Date.parse(message.created_at),
    kind: isSystem ? "error" : "message",
    title: isUser ? "You" : message.role === "Assistant" ? "Assistant" : "Run failed",
    body: message.content,
    tone: isSystem ? "danger" : isUser ? "default" : "muted",
    createdAt: message.created_at,
    raw: {
      id: -Date.parse(message.created_at),
      run_id: message.run_id ?? ("unknown" as AgentRunId),
      thread_id: message.thread_id,
      sequence: Date.parse(message.created_at),
      event_type: "Message",
      payload: { role: message.role, content: message.content },
      created_at: message.created_at,
    },
  };
}

interface MessagePayload {
  role?: string;
  content?: string;
}

function payloadRecord(payload: unknown): Record<string, unknown> {
  return typeof payload === "object" && payload !== null
    ? (payload as Record<string, unknown>)
    : {};
}

function payloadText(payload: unknown): string {
  if (typeof payload === "string") return payload;
  const record = payloadRecord(payload);
  if (typeof record.content === "string") return record.content;
  if (typeof record.message === "string") return record.message;
  return JSON.stringify(payload, null, 2);
}

function titleCase(value: string): string {
  if (!value) return "Event";
  return value.slice(0, 1).toUpperCase() + value.slice(1);
}

function runtimeEventRunId(event: RuntimeEvent): AgentRunId {
  const data = event.data as Record<string, unknown>;
  if ("run_id" in data && typeof data.run_id === "string") {
    return data.run_id as AgentRunId;
  }
  const run = data.run;
  if (run && typeof run === "object" && "id" in run && typeof run.id === "string") {
    return run.id as AgentRunId;
  }
  return "unknown" as AgentRunId;
}

function runtimeEventThreadId(event: RuntimeEvent): ThreadId {
  const data = event.data as Record<string, unknown>;
  const run = data.run;
  if (run && typeof run === "object" && "thread_id" in run) {
    return (run as { thread_id: ThreadId }).thread_id;
  }
  return "unknown" as ThreadId;
}

function runtimeEventCreatedAt(event: RuntimeEvent): string {
  const data = event.data as Record<string, unknown>;
  const run = data.run;
  if (run && typeof run === "object" && "created_at" in run) {
    return (run as { created_at: string }).created_at;
  }
  return new Date().toISOString();
}

export function mapRunEventToBlock(event: RunEvent): ConversationBlock {
  const payload = payloadRecord(event.payload) as MessagePayload;
  const eventType = event.event_type;

  if (eventType.toLowerCase().includes("error") || eventType === "RunFailed") {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "error",
      title: "Error",
      body: payloadText(event.payload),
      tone: "danger",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType.toLowerCase().includes("tool")) {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "tool",
      title: "Tool Call",
      body: payloadText(event.payload),
      tone: "muted",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType.toLowerCase().includes("approval")) {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "approval",
      title: "Approval Required",
      body: payloadText(event.payload),
      tone: "warning",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType.toLowerCase().includes("artifact")) {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "artifact",
      title: "Artifact",
      body: payloadText(event.payload),
      tone: "success",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType === "Message") {
    const role = typeof payload.role === "string" ? payload.role : "assistant";
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "message",
      title: titleCase(role),
      body: payloadText(event.payload),
      tone: "default",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType.startsWith("Run")) {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "status",
      title: eventType,
      body: payloadText(event.payload),
      tone: eventType === "RunCompleted" ? "success" : "muted",
      createdAt: event.created_at,
      raw: event,
    };
  }

  return {
    id: `event-${event.id}`,
    sequence: event.sequence,
    kind: "diagnostic",
    title: eventType,
    body: payloadText(event.payload),
    tone: "muted",
    createdAt: event.created_at,
    raw: event,
  };
}

export function runtimeEventToRunEvent(event: RuntimeEvent, index: number): RunEvent {
  return {
    id: -index,
    run_id: runtimeEventRunId(event),
    thread_id: runtimeEventThreadId(event),
    sequence: index,
    event_type: event.kind,
    payload: event,
    created_at: runtimeEventCreatedAt(event),
  };
}

export function mergeRunEvents(persisted: RunEvent[], live: RuntimeEvent[]): RunEvent[] {
  const liveOffset = persisted.length + 1;
  const liveEvents = live.map((event, index) => runtimeEventToRunEvent(event, liveOffset + index));
  return [...persisted, ...liveEvents].sort((a, b) => a.sequence - b.sequence);
}
