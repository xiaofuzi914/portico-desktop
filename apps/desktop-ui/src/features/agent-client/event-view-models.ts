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
  /** Chat author — set only for kind "message"; drives the user-bubble layout. */
  role?: "user" | "assistant";
  createdAt: string;
  raw: RunEvent;
}

/** Role rank when timestamps collide: user first, then assistant, then system. */
function messageRoleRank(role: Message["role"] | undefined): number {
  if (role === "User") return 0;
  if (role === "Assistant") return 1;
  return 2;
}

/**
 * Map a durable message into a timeline block.
 * @param index 0-based index in the server-ordered list (created_at ASC, id ASC).
 *               Used as a stable sequence so same-second User/Assistant never swap.
 */
/**
 * System rows are overloaded: branch context seeds, tool-approval notes, and
 * real run failures. Only treat failure-shaped text as errors so 开子会话
 * context is not shown as a red "Run failed" bubble before the first answer.
 */
function classifySystemMessage(content: string): {
  kind: ConversationBlockKind;
  title: string;
  tone: ConversationBlockTone;
} {
  const trimmed = content.trim();
  const lower = trimmed.toLowerCase();
  const isFailure =
    lower.startsWith("run failed:") ||
    lower.startsWith("run failed：") ||
    lower.includes("provider_unavailable") ||
    lower.includes("provider_selection") ||
    /^error[:：]/i.test(trimmed);

  if (isFailure) {
    return { kind: "error", title: "Run failed", tone: "danger" };
  }

  // Branch context seed (【从会话…发散】) and other injected notes.
  if (
    trimmed.startsWith("【从会话") ||
    trimmed.startsWith("【会话上下文") ||
    lower.includes("parent session") ||
    lower.includes("父会话") ||
    lower.includes("划词发散")
  ) {
    return { kind: "status", title: "Context", tone: "muted" };
  }

  return { kind: "status", title: "System", tone: "muted" };
}

export function mapMessageToBlock(message: Message, index = 0): ConversationBlock {
  const isUser = message.role === "User";
  const isSystem = message.role === "System";
  // Prefer list order over Date.parse alone — SQLite timestamps often share the
  // same second for user+assistant of one turn, which used to invert the chat.
  const sequence = index * 10 + messageRoleRank(message.role);
  const systemMeta = isSystem ? classifySystemMessage(message.content) : null;
  return {
    id: `message-${message.id}`,
    sequence,
    kind: systemMeta?.kind ?? "message",
    title: isUser
      ? "You"
      : message.role === "Assistant"
        ? "Assistant"
        : (systemMeta?.title ?? "System"),
    body: message.content,
    tone: systemMeta?.tone ?? (isUser ? "default" : "muted"),
    role: isSystem ? undefined : isUser ? "user" : "assistant",
    createdAt: message.created_at,
    raw: {
      id: -sequence,
      run_id: message.run_id ?? ("unknown" as AgentRunId),
      thread_id: message.thread_id,
      sequence,
      event_type: "Message",
      payload: { role: message.role, content: message.content },
      created_at: message.created_at,
    },
  };
}

/**
 * Sort conversation blocks for display. Durable messages keep server order via
 * sequence; streaming blocks always sort after their run's durable messages.
 */
export function sortConversationBlocks(blocks: ConversationBlock[]): ConversationBlock[] {
  return [...blocks].sort((a, b) => {
    if (a.sequence !== b.sequence) return a.sequence - b.sequence;
    return a.id.localeCompare(b.id);
  });
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
      role: role.toLowerCase() === "user" ? "user" : "assistant",
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
