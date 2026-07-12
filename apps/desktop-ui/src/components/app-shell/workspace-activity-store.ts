import {
  asWorkspaceId,
  type AgentRunId,
  type AgentRunStatus,
  type WorkspaceId,
} from "@/lib/schemas";

const workspaceRunActivityStorageKey = "portico.workspaceRunActivity";
const workspaceRunActivityEventName = "portico:workspace-run-activity";

interface WorkspaceRunActivity {
  workspaceId: string;
  status: AgentRunStatus;
  updatedAt: string;
}

const activeRunStatuses = new Set<AgentRunStatus>([
  "Queued",
  "Running",
  "WaitingApproval",
  "Paused",
]);

export function updateWorkspaceRunActivity(
  runId: AgentRunId,
  workspaceId: WorkspaceId,
  status: AgentRunStatus,
) {
  const current = readWorkspaceRunActivity();
  const next = activeRunStatuses.has(status)
    ? {
        ...current,
        [runId]: {
          workspaceId,
          status,
          updatedAt: new Date().toISOString(),
        },
      }
    : Object.fromEntries(Object.entries(current).filter(([id]) => id !== runId));

  writeWorkspaceRunActivity(next);
  dispatchWorkspaceRunActivityChanged();
}

export function readRunningWorkspaceIds(): Set<WorkspaceId> {
  return new Set(
    Object.values(readWorkspaceRunActivity())
      .filter((activity) => activeRunStatuses.has(activity.status))
      .map((activity) => asWorkspaceId(activity.workspaceId)),
  );
}

export function subscribeWorkspaceRunActivityChanged(listener: () => void): () => void {
  if (typeof window === "undefined") return () => {};

  const handleStorage = (event: StorageEvent) => {
    if (event.key === workspaceRunActivityStorageKey) listener();
  };
  window.addEventListener(workspaceRunActivityEventName, listener);
  window.addEventListener("storage", handleStorage);

  return () => {
    window.removeEventListener(workspaceRunActivityEventName, listener);
    window.removeEventListener("storage", handleStorage);
  };
}

function readWorkspaceRunActivity(): Record<string, WorkspaceRunActivity> {
  if (typeof window === "undefined") return {};
  try {
    const rawValue = window.localStorage.getItem(workspaceRunActivityStorageKey);
    if (!rawValue) return {};
    const parsedValue = JSON.parse(rawValue) as unknown;
    if (!parsedValue || typeof parsedValue !== "object" || Array.isArray(parsedValue)) return {};

    return Object.fromEntries(
      Object.entries(parsedValue).filter((entry): entry is [string, WorkspaceRunActivity] => {
        const [, value] = entry;
        return isWorkspaceRunActivity(value);
      }),
    );
  } catch {
    return {};
  }
}

function writeWorkspaceRunActivity(value: Record<string, WorkspaceRunActivity>) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(workspaceRunActivityStorageKey, JSON.stringify(value));
  } catch {
    // Running-state ordering is advisory; storage failures should not break navigation.
  }
}

function dispatchWorkspaceRunActivityChanged() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new Event(workspaceRunActivityEventName));
}

function isWorkspaceRunActivity(value: unknown): value is WorkspaceRunActivity {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const candidate = value as Partial<WorkspaceRunActivity>;
  return (
    typeof candidate.workspaceId === "string" &&
    typeof candidate.updatedAt === "string" &&
    typeof candidate.status === "string" &&
    activeRunStatuses.has(candidate.status as AgentRunStatus)
  );
}
