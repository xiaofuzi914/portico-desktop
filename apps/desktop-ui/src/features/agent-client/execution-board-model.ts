/**
 * Pure view-models for the execution board (run → stage → task → output).
 * Unit-tested without React; board UI maps these into progressive disclosure.
 */

import type {
  AgentRun,
  Orchestration,
  OrchestrationStage,
  OrchestrationStageTask,
} from "@/lib/schemas";

export type BoardFilter = "all" | "multi" | "single" | "active" | "failed";

export type BoardListItem =
  | {
      kind: "orchestration";
      id: string;
      createdAt: string;
      status: string;
      title: string;
      workflowTitle: string | null;
      stageCount: number;
      completedStages: number;
      orchestration: Orchestration;
    }
  | {
      kind: "run";
      id: string;
      createdAt: string;
      status: string;
      title: string;
      run: AgentRun;
    };

export type BoardStageView = {
  id: string;
  kind: string;
  title: string;
  status: string;
  agentName: string;
  taskCount: number;
  completedTasks: number;
  tasks: BoardTaskView[];
  outputPreview: string | null;
  errorMessage: string | null;
  /** Loop: current iteration (1-based) when running/completed. */
  currentIteration: number | null;
  /** Loop: max iterations cap. */
  maxIterations: number | null;
  /** Loop: body stage ids. */
  bodyStageIds: string[];
  /** Model tier / thinking from execution_spec when present. */
  modelTier: string | null;
  thinkingMode: string | null;
  allowedTools: string[];
  canRetry: boolean;
  attempt: number;
};

export type BoardTaskView = {
  id: string;
  label: string;
  status: string;
  outputPreview: string | null;
  itemIndex: number | null;
};

export type BoardDetailView = {
  kind: "orchestration" | "run";
  title: string;
  status: string;
  workflowTitle: string | null;
  stages: BoardStageView[];
  /** Flat pipeline when no stages (legacy / single-agent). */
  flatSteps: BoardTaskView[];
  resultSummary: string | null;
  rationale: string | null;
};

function isActive(status: string): boolean {
  return (
    status === "Running" ||
    status === "Queued" ||
    status === "Planning" ||
    status === "WaitingApproval" ||
    status === "Paused" ||
    status === "Pending"
  );
}

function truncate(text: string, max = 160): string {
  const t = text.replace(/\s+/g, " ").trim();
  if (t.length <= max) return t;
  return `${t.slice(0, max - 1)}…`;
}

/** Build sorted board list from orchestrations + solo runs. */
export function buildBoardList(
  orchestrations: Orchestration[],
  runs: AgentRun[],
  promptByRunId: Record<string, string>,
): BoardListItem[] {
  const parentIds = new Set(orchestrations.map((o) => o.parent_run_id));
  const items: BoardListItem[] = [];

  for (const o of orchestrations) {
    const stages = o.plan?.stages ?? [];
    const completedStages = stages.filter((s) => s.status === "Completed").length;
    const title =
      promptByRunId[o.parent_run_id] ||
      o.task ||
      o.plan?.workflow_title ||
      "Multi-agent run";
    items.push({
      kind: "orchestration",
      id: `orch:${o.id}`,
      createdAt: o.created_at,
      status: o.status,
      title: truncate(title, 140),
      workflowTitle: o.plan?.workflow_title ?? o.plan?.workflow_id ?? null,
      stageCount: stages.length,
      completedStages,
      orchestration: o,
    });
  }

  for (const r of runs) {
    if (parentIds.has(r.id)) continue;
    items.push({
      kind: "run",
      id: `run:${r.id}`,
      createdAt: r.created_at,
      status: r.status,
      title: truncate(promptByRunId[r.id] || "Single-agent turn", 140),
      run: r,
    });
  }

  items.sort((a, b) => Date.parse(b.createdAt) - Date.parse(a.createdAt));
  return items;
}

export function filterBoardList(
  items: BoardListItem[],
  filter: BoardFilter,
): BoardListItem[] {
  return items.filter((item) => {
    if (filter === "all") return true;
    if (filter === "multi") return item.kind === "orchestration";
    if (filter === "single") return item.kind === "run";
    if (filter === "active") return isActive(item.status);
    if (filter === "failed") {
      return item.status === "Failed" || item.status === "PartialCompleted";
    }
    return true;
  });
}

function mapTask(t: OrchestrationStageTask): BoardTaskView {
  return {
    id: t.id,
    label: t.label,
    status: t.status,
    outputPreview: t.output_summary ? truncate(t.output_summary, 200) : null,
    itemIndex: t.item_index ?? null,
  };
}

function mapStage(s: OrchestrationStage): BoardStageView {
  const tasks = (s.tasks ?? []).map(mapTask);
  const completedTasks = tasks.filter((t) => t.status === "Completed").length;
  const attempt = Math.max(1, ...(s.tasks ?? []).map((t) => t.attempt ?? 1));
  const spec = s.execution_spec;
  return {
    id: s.id,
    kind: s.kind,
    title: s.title,
    status: s.status,
    agentName: s.agent_name,
    taskCount: tasks.length,
    completedTasks,
    tasks,
    outputPreview: s.output_payload ? truncate(s.output_payload, 120) : null,
    errorMessage: s.error_message ?? null,
    currentIteration: s.current_iteration ?? null,
    maxIterations: s.max_iterations ?? null,
    bodyStageIds: s.body_stage_ids ?? [],
    modelTier: spec?.model_tier ?? null,
    thinkingMode: spec?.thinking_mode ?? null,
    allowedTools: spec?.allowed_tools ?? [],
    canRetry:
      (s.status === "Failed" || s.status === "Cancelled") &&
      s.kind !== "loop" &&
      attempt < 3,
    attempt,
  };
}

/** Progressive disclosure detail model for a selected board item. */
export function buildBoardDetail(
  item: BoardListItem,
  flatFallback: BoardTaskView[] = [],
): BoardDetailView {
  if (item.kind === "orchestration") {
    const o = item.orchestration;
    const stages = (o.plan?.stages ?? []).map(mapStage);
    // Legacy multi-role without stages: project subagents as flat steps.
    const flatSteps =
      stages.length > 0
        ? []
        : (o.plan?.subagents ?? []).map((s, i) => ({
            id: s.id,
            label: s.agent_name || `role-${i + 1}`,
            status: s.status,
            outputPreview: s.output_summary ? truncate(s.output_summary, 200) : null,
            itemIndex: i,
          }));
    return {
      kind: "orchestration",
      title: item.title,
      status: o.status,
      workflowTitle: item.workflowTitle,
      stages,
      flatSteps,
      resultSummary: o.result_summary,
      rationale: o.plan?.planning_rationale || null,
    };
  }

  return {
    kind: "run",
    title: item.title,
    status: item.run.status,
    workflowTitle: null,
    stages: [],
    flatSteps: flatFallback,
    resultSummary: null,
    rationale: null,
  };
}

export function boardStats(items: BoardListItem[]): {
  total: number;
  active: number;
  multi: number;
  completed: number;
  failed: number;
} {
  let active = 0;
  let multi = 0;
  let completed = 0;
  let failed = 0;
  for (const item of items) {
    if (item.kind === "orchestration") multi += 1;
    if (isActive(item.status)) active += 1;
    if (item.status === "Completed" || item.status === "PartialCompleted") completed += 1;
    if (item.status === "Failed" || item.status === "PartialCompleted") failed += 1;
  }
  return { total: items.length, active, multi, completed, failed };
}
