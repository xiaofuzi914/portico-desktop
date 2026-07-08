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

class RuntimeEventStore {
  private events: RuntimeEvent[] = [];
  private listeners = new Set<Listener>();

  getEvents(): RuntimeEvent[] {
    return this.events;
  }

  addEvent(event: RuntimeEvent): void {
    this.events = [...this.events, event];
    this.emit();
  }

  clearEvents(): void {
    this.events = [];
    this.emit();
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
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
export function selectEventsForRun(events: RuntimeEvent[], runId: AgentRunId): RuntimeEvent[] {
  return events.filter((event) => {
    const data = event.data as Record<string, unknown> | undefined;
    return data?.run_id === runId;
  });
}

/**
 * React hook that returns runtime events for a specific run, updated live.
 */
export function useRuntimeEvents(runId: AgentRunId | undefined): RuntimeEvent[] {
  const [events, setEvents] = useState<RuntimeEvent[]>([]);

  useEffect(() => {
    if (!runId) return;

    const update = () => {
      setEvents(selectEventsForRun(runtimeEventStore.getEvents(), runId));
    };

    update();
    return runtimeEventStore.subscribe(update);
  }, [runId]);

  return events;
}
