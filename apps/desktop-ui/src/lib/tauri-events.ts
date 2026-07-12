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

type Listener = (event?: RuntimeEvent) => void;

const MAX_RUN_SHARDS = 8;

function eventRunId(event: RuntimeEvent): AgentRunId | undefined {
  if (event.kind === "RunStarted") {
    return event.data.run.id;
  }
  return "run_id" in event.data ? event.data.run_id : undefined;
}

/** Tools that mutate the workspace tree shown in the inspector files panel. */
export function isWorkspaceMutatingTool(toolName: string): boolean {
  const name = toolName.trim().toLowerCase();
  if (!name) return false;
  if (
    name === "fs_write" ||
    name === "fs_edit" ||
    name === "write_file" ||
    name === "edit_file" ||
    name === "apply_patch" ||
    name === "delete_file" ||
    name === "fs_delete" ||
    name === "fs_mkdir" ||
    name === "fs_move" ||
    name === "fs_rename"
  ) {
    return true;
  }
  // Broad match for future tool aliases
  if (name.startsWith("fs_") && /(write|edit|delete|mkdir|move|rename|create)/.test(name)) {
    return true;
  }
  return false;
}

/** Whether a runtime event should refresh the project folder listing. */
export function shouldRefreshWorkspaceFiles(event: RuntimeEvent): boolean {
  if (event.kind === "ToolCompleted" && isWorkspaceMutatingTool(event.data.tool_name)) {
    return true;
  }
  if (event.kind === "ArtifactCreated") {
    return true;
  }
  // Safety net after a turn: catches writes whose tool name we don't recognize yet.
  if (event.kind === "RunCompleted") {
    return true;
  }
  return false;
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
    this.emit(event);
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

  private emit(event?: RuntimeEvent): void {
    for (const listener of this.listeners) {
      listener(event);
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
