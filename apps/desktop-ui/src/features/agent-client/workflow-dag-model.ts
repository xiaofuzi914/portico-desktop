/**
 * Pure helpers for editable multi-stage workflow DAGs (template library + loop).
 * Unit-tested without React; UI maps these into the DAG editor surface.
 */

import type { OrchestrationStage, OrchestrationStageKind } from "@/lib/schemas";

export type StageEditDraft = {
  id: string;
  kind: OrchestrationStageKind;
  title: string;
  agent_name: string;
  prompt_template: string;
  depends_on: string[];
  foreach_path: string | null;
  body_stage_ids: string[];
  max_iterations: number | null;
  stop_flag_path: string | null;
  /**
   * Seeded control JSON for foreach handoff (e.g. multi-lens triage `items`).
   * Must survive save → start-by-UUID so fan-out is not empty.
   */
  output_payload: string | null;
};

export function stageToDraft(s: OrchestrationStage): StageEditDraft {
  return {
    id: s.id,
    kind: s.kind,
    title: s.title,
    agent_name: s.agent_name,
    prompt_template: s.prompt_template,
    depends_on: [...(s.depends_on ?? [])],
    foreach_path: s.foreach_path ?? null,
    body_stage_ids: [...(s.body_stage_ids ?? [])],
    max_iterations: s.max_iterations ?? null,
    stop_flag_path: s.stop_flag_path ?? null,
    // Preserve seeded control payloads (foreach items) across edit/save.
    output_payload: s.output_payload ?? null,
  };
}

export function draftToStage(d: StageEditDraft): OrchestrationStage {
  return {
    id: d.id.trim(),
    kind: d.kind,
    title: d.title.trim() || d.id,
    agent_name: d.agent_name.trim() || "worker",
    status: "Pending",
    prompt_template: d.prompt_template,
    depends_on: d.depends_on.map((x) => x.trim()).filter(Boolean),
    foreach_path: d.kind === "foreach" ? d.foreach_path : null,
    body_stage_ids: d.kind === "loop" ? d.body_stage_ids.map((x) => x.trim()).filter(Boolean) : [],
    max_iterations: d.kind === "loop" ? d.max_iterations : null,
    stop_flag_path: d.kind === "loop" ? d.stop_flag_path : null,
    current_iteration: null,
    tasks: [],
    // Keep seed control JSON; runtime tasks/status are cleared on edit.
    output_payload: d.output_payload ?? null,
    error_message: null,
  };
}

/** Clamp loop max iterations to engine bounds (1–32). */
export function clampLoopMaxIterations(value: number | null | undefined): number {
  if (value == null || !Number.isFinite(value)) return 3;
  return Math.min(32, Math.max(1, Math.floor(value)));
}

/**
 * Validate an editable stage DAG (mirrors backend validate_stage_dag essentials).
 * Returns null when valid, otherwise a human-readable error.
 */
export function validateStageDag(stages: StageEditDraft[]): string | null {
  if (stages.length === 0) return "workflow needs at least one stage";
  const ids = new Set<string>();
  for (const s of stages) {
    const id = s.id.trim();
    if (!id) return "stage id must not be empty";
    if (ids.has(id)) return `duplicate stage id: ${id}`;
    ids.add(id);
  }

  const bodyMembers = new Set<string>();
  for (const s of stages) {
    if (s.kind !== "loop") continue;
    if (s.body_stage_ids.length === 0) {
      return `loop stage "${s.id}" needs body_stage_ids`;
    }
    for (const bid of s.body_stage_ids) {
      if (!ids.has(bid.trim())) {
        return `loop "${s.id}" body stage missing: ${bid}`;
      }
      bodyMembers.add(bid.trim());
    }
    const max = clampLoopMaxIterations(s.max_iterations);
    if (max < 1 || max > 32) {
      return `loop "${s.id}" max_iterations out of range`;
    }
  }

  for (const s of stages) {
    for (const dep of s.depends_on) {
      if (!ids.has(dep.trim())) {
        return `stage "${s.id}" depends on unknown id: ${dep}`;
      }
    }
  }

  // Cycle detection on non-body edges (same spirit as backend).
  const adj = new Map<string, string[]>();
  for (const s of stages) {
    if (bodyMembers.has(s.id)) continue;
    for (const dep of s.depends_on) {
      const d = dep.trim();
      if (!ids.has(d) || bodyMembers.has(d)) continue;
      const list = adj.get(d) ?? [];
      list.push(s.id);
      adj.set(d, list);
    }
  }
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const dfs = (n: string): boolean => {
    if (visited.has(n)) return false;
    if (visiting.has(n)) return true;
    visiting.add(n);
    for (const x of adj.get(n) ?? []) {
      if (dfs(x)) return true;
    }
    visiting.delete(n);
    visited.add(n);
    return false;
  };
  for (const s of stages) {
    if (bodyMembers.has(s.id)) continue;
    if (dfs(s.id)) return "stage graph has a cycle";
  }
  return null;
}

/** Apply a DAG edit: validate + normalize stages for save/start. */
export function applyStageEdit(stages: StageEditDraft[]): {
  ok: true;
  stages: OrchestrationStage[];
} | {
  ok: false;
  error: string;
} {
  const err = validateStageDag(stages);
  if (err) return { ok: false, error: err };
  return {
    ok: true,
    stages: stages.map((d) => {
      const stage = draftToStage(d);
      if (stage.kind === "loop") {
        stage.max_iterations = clampLoopMaxIterations(d.max_iterations);
        stage.stop_flag_path = (d.stop_flag_path ?? "pass").trim() || "pass";
      }
      return stage;
    }),
  };
}

export function newBlankStage(kind: OrchestrationStageKind = "single", index = 1): StageEditDraft {
  const id = kind === "loop" ? `loop_${index}` : `stage_${index}`;
  return {
    id,
    kind,
    title: kind === "loop" ? "Bounded loop" : `Stage ${index}`,
    agent_name: kind === "loop" ? "planner" : "worker",
    prompt_template:
      kind === "loop"
        ? "Loop container"
        : "Task:\n{task}\n\nUpstream:\n{upstream}\n\nDo the work.",
    depends_on: [],
    foreach_path: kind === "foreach" ? "items" : null,
    body_stage_ids: [],
    max_iterations: kind === "loop" ? 3 : null,
    stop_flag_path: kind === "loop" ? "pass" : null,
    output_payload: null,
  };
}

/** Remove a stage and scrub references from depends_on / body_stage_ids. */
export function removeStage(stages: StageEditDraft[], id: string): StageEditDraft[] {
  return stages
    .filter((s) => s.id !== id)
    .map((s) => ({
      ...s,
      depends_on: s.depends_on.filter((d) => d !== id),
      body_stage_ids: s.body_stage_ids.filter((b) => b !== id),
    }));
}

/** Top-level stages for display (exclude pure loop body members when possible). */
export function topLevelStageIds(stages: StageEditDraft[]): string[] {
  const body = new Set(
    stages.filter((s) => s.kind === "loop").flatMap((s) => s.body_stage_ids),
  );
  return stages.filter((s) => !body.has(s.id)).map((s) => s.id);
}
