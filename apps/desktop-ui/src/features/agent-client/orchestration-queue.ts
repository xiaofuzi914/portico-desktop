/**
 * Pure queue helpers for composer multi-role / catalog / DAG starts.
 * Busy-channel drains must preserve workflowId so catalog templates are not
 * downgraded to adaptive (null) starts.
 */

export type ComposerQueueMode = "send" | "multi-role";

export type ComposerQueuedTask = Readonly<{
  id: string;
  content: string;
  mode: ComposerQueueMode;
  /** Catalog key or template UUID; only used when mode is multi-role. */
  workflowId: string | null;
}>;

/** Build a queue item for a multi-role / template start. */
export function queueMultiRoleTask(
  content: string,
  workflowId: string | null,
  id: string,
): ComposerQueuedTask {
  return {
    id,
    content: content.trim(),
    mode: "multi-role",
    workflowId: workflowId ?? null,
  };
}

/** Build a queue item for default single-agent send. */
export function queueSendTask(content: string, id: string): ComposerQueuedTask {
  return {
    id,
    content: content.trim(),
    mode: "send",
    workflowId: null,
  };
}

/**
 * Resolve args passed to startOrchestration when draining a queue item.
 * Catalog / DAG starts must keep their workflowId (not force adaptive null).
 */
export function resolveQueuedOrchestrationStart(item: ComposerQueuedTask): {
  task: string;
  workflowId: string | null;
} | null {
  if (item.mode !== "multi-role") return null;
  const task = item.content.trim();
  if (!task) return null;
  return {
    task,
    workflowId: item.workflowId ?? null,
  };
}
