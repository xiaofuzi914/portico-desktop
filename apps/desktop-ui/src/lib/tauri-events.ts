import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { parseRuntimeEvent } from "@/lib/tauri-api";
import type { AgentRunId, RuntimeEvent } from "@/lib/schemas";

declare global {
  interface Window {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  }
}

function isTauriAvailable(): boolean {
  return (
    typeof window !== "undefined" &&
    (window.__TAURI__ !== undefined || window.__TAURI_INTERNALS__ !== undefined)
  );
}

type Listener = () => void;

const MAX_RUN_SHARDS = 8;

function eventRunId(event: RuntimeEvent): AgentRunId | undefined {
  if (event.kind === "RunStarted") {
    return event.data.run.id;
  }
  return event.data.run_id;
}

export class RuntimeEventStore {
  private runs = new Map<AgentRunId, RuntimeEvent[]>();
  private runAccessOrder: AgentRunId[] = [];
  private listeners = new Set<Listener>();

  getEvents(runId?: AgentRunId): RuntimeEvent[] {
    if (runId) {
      return this.runs.get(runId) ?? [];
    }
    return Array.from(this.runs.values()).flat();
  }

  addEvent(event: RuntimeEvent): void {
    const runId = eventRunId(event);
    if (!runId) return;

    const existing = this.runs.get(runId) ?? [];
    this.runs.set(runId, [...existing, event]);
    this.touchRun(runId);
    this.enforceRunLimit();
    this.emit();
  }

  clearEvents(): void {
    this.runs.clear();
    this.runAccessOrder = [];
    this.emit();
  }

  clearRun(runId: AgentRunId): void {
    this.runs.delete(runId);
    this.runAccessOrder = this.runAccessOrder.filter((id) => id !== runId);
    this.emit();
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private touchRun(runId: AgentRunId): void {
    this.runAccessOrder = this.runAccessOrder.filter((id) => id !== runId);
    this.runAccessOrder.push(runId);
  }

  private enforceRunLimit(): void {
    while (this.runAccessOrder.length > MAX_RUN_SHARDS) {
      const oldest = this.runAccessOrder.shift();
      if (oldest) {
        this.runs.delete(oldest);
      }
    }
  }

  private emit(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }
}

export const runtimeEventStore = new RuntimeEventStore();

/**
 * Subscribe to the Tauri `portico:event` channel and push validated events
 * into the global store. Returns an unlisten function.
 */
export async function listenToRuntimeEvents(): Promise<UnlistenFn> {
  if (!isTauriAvailable()) {
    return () => {};
  }

  const unlisten = await listen<unknown>("portico:event", (event) => {
    const parsed = parseRuntimeEvent(event.payload);
    if (parsed) {
      runtimeEventStore.addEvent(parsed);
    }
  });
  return unlisten;
}

/**
 * Select events for a specific run from the global store.
 */
export function selectEventsForRun(
  events: RuntimeEvent[],
  runId: AgentRunId,
): RuntimeEvent[] {
  return events.filter((event) => eventRunId(event) === runId);
}

/**
 * React hook that returns runtime events for a specific run, updated live.
 */
export function useRuntimeEvents(runId: AgentRunId | undefined): RuntimeEvent[] {
  const [events, setEvents] = useState<RuntimeEvent[]>([]);

  useEffect(() => {
    if (!runId) return;

    const update = () => {
      setEvents(runtimeEventStore.getEvents(runId));
    };

    update();
    return runtimeEventStore.subscribe(update);
  }, [runId]);

  return events;
}
