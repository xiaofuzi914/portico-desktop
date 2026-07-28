import { describe, expect, it } from "vitest";
import {
  applyStageEdit,
  clampLoopMaxIterations,
  newBlankStage,
  removeStage,
  stageToDraft,
  topLevelStageIds,
  validateStageDag,
  type StageEditDraft,
} from "./workflow-dag-model";
import type { OrchestrationStage } from "@/lib/schemas";

function draft(
  partial: Partial<StageEditDraft> & Pick<StageEditDraft, "id" | "kind">,
): StageEditDraft {
  return {
    id: partial.id,
    kind: partial.kind,
    title: partial.title ?? partial.id,
    agent_name: partial.agent_name ?? "worker",
    prompt_template: partial.prompt_template ?? "{task}",
    depends_on: partial.depends_on ?? [],
    foreach_path: partial.foreach_path ?? null,
    body_stage_ids: partial.body_stage_ids ?? [],
    max_iterations: partial.max_iterations ?? null,
    stop_flag_path: partial.stop_flag_path ?? null,
    output_payload: partial.output_payload ?? null,
  };
}

describe("workflow-dag-model", () => {
  it("clamps loop max iterations to 1–32", () => {
    expect(clampLoopMaxIterations(null)).toBe(3);
    expect(clampLoopMaxIterations(0)).toBe(1);
    expect(clampLoopMaxIterations(100)).toBe(32);
    expect(clampLoopMaxIterations(5)).toBe(5);
  });

  it("validates multi-stage catalog-shaped graphs", () => {
    const multiLens = [
      draft({ id: "triage", kind: "single" }),
      draft({
        id: "lenses",
        kind: "foreach",
        depends_on: ["triage"],
        foreach_path: "items",
      }),
      draft({ id: "synthesize", kind: "reduce", depends_on: ["lenses"] }),
    ];
    expect(validateStageDag(multiLens)).toBeNull();

    const withLoop = [
      draft({ id: "draft", kind: "single" }),
      draft({ id: "critique", kind: "single", depends_on: ["draft"] }),
      draft({
        id: "refine_loop",
        kind: "loop",
        depends_on: ["draft"],
        body_stage_ids: ["critique"],
        max_iterations: 3,
        stop_flag_path: "pass",
      }),
      draft({ id: "polish", kind: "reduce", depends_on: ["refine_loop"] }),
    ];
    expect(validateStageDag(withLoop)).toBeNull();
    expect(topLevelStageIds(withLoop)).toEqual(["draft", "refine_loop", "polish"]);
  });

  it("rejects cycles and bad loop bodies", () => {
    const cycle = [
      draft({ id: "a", kind: "single", depends_on: ["b"] }),
      draft({ id: "b", kind: "single", depends_on: ["a"] }),
    ];
    expect(validateStageDag(cycle)).toMatch(/cycle/i);

    const badLoop = [
      draft({
        id: "loop1",
        kind: "loop",
        body_stage_ids: ["missing"],
        max_iterations: 2,
      }),
    ];
    expect(validateStageDag(badLoop)).toMatch(/missing/i);
  });

  it("applyStageEdit round-trips add/remove and normalizes loop", () => {
    let stages = [
      draft({ id: "plan", kind: "single" }),
      draft({ id: "do", kind: "single", depends_on: ["plan"] }),
    ];
    stages = [...stages, newBlankStage("reduce", 3)];
    stages[2] = { ...stages[2], id: "report", depends_on: ["do"], title: "Report" };

    const applied = applyStageEdit(stages);
    expect(applied.ok).toBe(true);
    if (!applied.ok) return;
    expect(applied.stages).toHaveLength(3);
    expect(applied.stages.map((s) => s.id)).toEqual(["plan", "do", "report"]);

    // Add loop body + loop container
    const editable = applied.stages.map(stageToDraft);
    editable.push(
      draft({
        id: "fix",
        kind: "single",
        depends_on: ["report"],
      }),
    );
    editable.push(
      draft({
        id: "improve",
        kind: "loop",
        depends_on: ["report"],
        body_stage_ids: ["fix"],
        max_iterations: 99,
        stop_flag_path: "pass",
      }),
    );
    const withLoop = applyStageEdit(editable);
    expect(withLoop.ok).toBe(true);
    if (!withLoop.ok) return;
    const loop = withLoop.stages.find((s) => s.kind === "loop");
    expect(loop?.max_iterations).toBe(32);
    expect(loop?.body_stage_ids).toEqual(["fix"]);

    // Remove body stage and scrub references
    const scrubbed = removeStage(
      withLoop.stages.map(stageToDraft),
      "fix",
    );
    expect(scrubbed.find((s) => s.id === "improve")?.body_stage_ids).toEqual([]);
    expect(validateStageDag(scrubbed)).toMatch(/body/i);
  });

  it("draftToStage clears runtime fields but keeps seed output_payload", () => {
    const runtime: OrchestrationStage = {
      id: "x",
      kind: "loop",
      title: "L",
      agent_name: "planner",
      status: "Running",
      prompt_template: "loop",
      depends_on: [],
      foreach_path: null,
      body_stage_ids: ["y"],
      max_iterations: 2,
      stop_flag_path: "pass",
      current_iteration: 1,
      tasks: [{ id: "t", label: "r1", status: "Completed" }],
      output_payload: "{}",
      error_message: "oops",
    };
    const draftStage = stageToDraft(runtime);
    const clean = applyStageEdit([
      draftStage,
      draft({ id: "y", kind: "single" }),
    ]);
    expect(clean.ok).toBe(true);
    if (!clean.ok) return;
    const loop = clean.stages.find((s) => s.id === "x")!;
    expect(loop.status).toBe("Pending");
    expect(loop.tasks).toEqual([]);
    expect(loop.current_iteration).toBeNull();
    expect(loop.error_message).toBeNull();
    expect(loop.output_payload).toBe("{}");
  });

  it("multi-lens triage control payload survives draft↔stage save round-trip", () => {
    // Shape matches bundled multi-lens-review seed used for foreach expansion.
    const triageSeed =
      '{"items":[{"id":"security","label":"Security","agent":"security-reviewer"},{"id":"tests","label":"Tests","agent":"tester"},{"id":"docs","label":"Docs","agent":"reviewer"}]}';
    const multiLens: OrchestrationStage[] = [
      {
        id: "triage",
        kind: "single",
        title: "Triage",
        agent_name: "planner",
        status: "Completed",
        prompt_template: "{task}",
        depends_on: [],
        body_stage_ids: [],
        tasks: [{ id: "t0", label: "done", status: "Completed" }],
        output_payload: triageSeed,
        current_iteration: null,
        error_message: null,
      },
      {
        id: "lenses",
        kind: "foreach",
        title: "Lenses",
        agent_name: "reviewer",
        status: "Pending",
        prompt_template: "{item}",
        depends_on: ["triage"],
        foreach_path: "items",
        body_stage_ids: [],
        tasks: [],
        output_payload: null,
      },
      {
        id: "synthesize",
        kind: "reduce",
        title: "Report",
        agent_name: "doc-writer",
        status: "Pending",
        prompt_template: "{upstream}",
        depends_on: ["lenses"],
        body_stage_ids: [],
        tasks: [],
      },
    ];

    const drafts = multiLens.map(stageToDraft);
    expect(drafts[0].output_payload).toBe(triageSeed);

    const applied = applyStageEdit(drafts);
    expect(applied.ok).toBe(true);
    if (!applied.ok) return;

    const triage = applied.stages.find((s) => s.id === "triage");
    expect(triage?.status).toBe("Pending");
    expect(triage?.tasks).toEqual([]);
    expect(triage?.output_payload).toBe(triageSeed);

    const items = JSON.parse(triage!.output_payload!).items as unknown[];
    expect(items).toHaveLength(3);

    // Second round-trip (re-open saved template) still keeps seeds.
    const again = applyStageEdit(applied.stages.map(stageToDraft));
    expect(again.ok).toBe(true);
    if (!again.ok) return;
    expect(again.stages.find((s) => s.id === "triage")?.output_payload).toBe(triageSeed);
  });
});
