import { describe, expect, it } from "vitest";
import {
  boardStats,
  buildBoardDetail,
  buildBoardList,
  filterBoardList,
} from "./execution-board-model";
import type { AgentRun, Orchestration } from "@/lib/schemas";
import { asAgentRunId, asThreadId, asWorkspaceId } from "@/lib/schemas";

const ws = asWorkspaceId("550e8400-e29b-41d4-a716-446655440000");
const th = asThreadId("550e8400-e29b-41d4-a716-446655440001");
const parent = asAgentRunId("550e8400-e29b-41d4-a716-446655440002");

function orch(partial: Partial<Orchestration> & Pick<Orchestration, "id" | "status">): Orchestration {
  return {
    id: partial.id as never,
    parent_run_id: parent,
    workspace_id: ws,
    thread_id: th,
    task: partial.task ?? "review auth",
    status: partial.status,
    plan: partial.plan ?? {
      parent_run_id: parent,
      subagents: [],
      pattern_ids: [],
      planning_rationale: "test graph",
      stages: [
        {
          id: "triage",
          kind: "single",
          title: "Triage",
          agent_name: "planner",
          status: "Completed",
          prompt_template: "{task}",
          depends_on: [],
          body_stage_ids: [],
          tasks: [
            {
              id: "triage-main",
              label: "Triage",
              status: "Completed",
              output_summary: "lenses ready",
            },
          ],
          output_payload: "{\"items\":[]}",
        },
        {
          id: "lenses",
          kind: "foreach",
          title: "Lenses",
          agent_name: "reviewer",
          status: "Running",
          prompt_template: "{item}",
          depends_on: ["triage"],
          foreach_path: "items",
          body_stage_ids: [],
          tasks: [
            {
              id: "lenses-task-0",
              item_index: 0,
              label: "Security",
              status: "Completed",
              output_summary: "no critical issues",
            },
            {
              id: "lenses-task-1",
              item_index: 1,
              label: "Tests",
              status: "Running",
              output_summary: null,
            },
          ],
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
      ],
      workflow_id: "multi-lens-review",
      workflow_title: "Multi-lens review",
    },
    pattern_ids: [],
    result_summary: partial.result_summary ?? null,
    created_at: partial.created_at ?? "2026-07-19T12:00:00.000Z",
    updated_at: partial.updated_at ?? "2026-07-19T12:00:00.000Z",
    completed_at: partial.completed_at ?? null,
  };
}

function run(id: string, status: AgentRun["status"], created: string): AgentRun {
  return {
    id: asAgentRunId(id),
    thread_id: th,
    workspace_id: ws,
    status,
    created_at: created,
    started_at: created,
    completed_at: status === "Completed" ? created : null,
  };
}

describe("execution-board-model", () => {
  it("builds hierarchy with workflow title and stage progress", () => {
    const list = buildBoardList(
      [orch({ id: "550e8400-e29b-41d4-a716-446655440003" as never, status: "Running" })],
      [],
      { [parent]: "Please review the auth module" },
    );
    expect(list).toHaveLength(1);
    expect(list[0].kind).toBe("orchestration");
    if (list[0].kind !== "orchestration") return;
    expect(list[0].title).toContain("auth");
    expect(list[0].workflowTitle).toBe("Multi-lens review");
    expect(list[0].stageCount).toBe(3);
    expect(list[0].completedStages).toBe(1);

    const detail = buildBoardDetail(list[0]);
    expect(detail.stages).toHaveLength(3);
    expect(detail.stages[0].kind).toBe("single");
    expect(detail.stages[1].kind).toBe("foreach");
    expect(detail.stages[1].tasks).toHaveLength(2);
    expect(detail.stages[1].tasks[0].outputPreview).toContain("critical");
    expect(detail.stages[2].kind).toBe("reduce");
    expect(detail.flatSteps).toHaveLength(0);
  });

  it("maps loop rounds onto board stage view", () => {
    const loopOrch = orch({
      id: "550e8400-e29b-41d4-a716-446655440099" as never,
      status: "Running",
      plan: {
        parent_run_id: parent,
        subagents: [],
        pattern_ids: [],
        planning_rationale: "iterative refine",
        stages: [
          {
            id: "draft",
            kind: "single",
            title: "Draft",
            agent_name: "worker",
            status: "Completed",
            prompt_template: "{task}",
            depends_on: [],
            body_stage_ids: [],
            tasks: [],
          },
          {
            id: "critique",
            kind: "single",
            title: "Critique",
            agent_name: "reviewer",
            status: "Completed",
            prompt_template: "{upstream}",
            depends_on: ["draft"],
            body_stage_ids: [],
            tasks: [],
          },
          {
            id: "refine_loop",
            kind: "loop",
            title: "Bounded refine loop",
            agent_name: "planner",
            status: "Running",
            prompt_template: "Loop",
            depends_on: ["draft"],
            body_stage_ids: ["critique"],
            max_iterations: 3,
            stop_flag_path: "pass",
            current_iteration: 2,
            tasks: [
              {
                id: "refine_loop-round-1",
                item_index: 1,
                label: "round 1/3",
                status: "Completed",
                output_summary: "continue",
              },
              {
                id: "refine_loop-round-2",
                item_index: 2,
                label: "round 2/3",
                status: "Running",
                output_summary: "still fixing",
              },
            ],
          },
        ],
        workflow_id: "iterative-refine",
        workflow_title: "Iterative refine",
      },
    });
    const list = buildBoardList([loopOrch], [], {});
    const detail = buildBoardDetail(list[0]);
    const loopStage = detail.stages.find((s) => s.kind === "loop");
    expect(loopStage).toBeTruthy();
    expect(loopStage?.currentIteration).toBe(2);
    expect(loopStage?.maxIterations).toBe(3);
    expect(loopStage?.bodyStageIds).toEqual(["critique"]);
    expect(loopStage?.tasks).toHaveLength(2);
    expect(loopStage?.tasks[0].label).toContain("round 1");
  });

  it("filters active and multi-role items", () => {
    const list = buildBoardList(
      [
        orch({
          id: "550e8400-e29b-41d4-a716-446655440003" as never,
          status: "Running",
          created_at: "2026-07-19T12:00:00.000Z",
        }),
        orch({
          id: "550e8400-e29b-41d4-a716-446655440004" as never,
          status: "Failed",
          created_at: "2026-07-19T11:00:00.000Z",
        }),
      ],
      [
        run("550e8400-e29b-41d4-a716-446655440005", "Completed", "2026-07-19T10:00:00.000Z"),
        run("550e8400-e29b-41d4-a716-446655440006", "Running", "2026-07-19T09:00:00.000Z"),
      ],
      {},
    );
    expect(filterBoardList(list, "multi")).toHaveLength(2);
    expect(filterBoardList(list, "single")).toHaveLength(2);
    expect(filterBoardList(list, "active").every((i) => i.status === "Running")).toBe(true);
    expect(filterBoardList(list, "failed")).toHaveLength(1);
    const stats = boardStats(list);
    expect(stats.total).toBe(4);
    expect(stats.multi).toBe(2);
    expect(stats.active).toBe(2);
    expect(stats.failed).toBe(1);
  });

  it("projects legacy subagents when stages are empty", () => {
    const o = orch({
      id: "550e8400-e29b-41d4-a716-446655440003" as never,
      status: "Completed",
    });
    o.plan = {
      parent_run_id: parent,
      subagents: [
        {
          id: asAgentRunId("550e8400-e29b-41d4-a716-446655440010"),
          parent_run_id: parent,
          agent_name: "explorer",
          status: "Completed",
          task_description: "scan",
          output_summary: "found modules",
          created_at: "2026-07-19T12:00:00.000Z",
          completed_at: "2026-07-19T12:01:00.000Z",
        },
      ],
      pattern_ids: [],
      planning_rationale: "legacy",
      stages: [],
    };
    const list = buildBoardList([o], [], {});
    const detail = buildBoardDetail(list[0]);
    expect(detail.stages).toHaveLength(0);
    expect(detail.flatSteps).toHaveLength(1);
    expect(detail.flatSteps[0].label).toBe("explorer");
    expect(detail.flatSteps[0].outputPreview).toContain("modules");
  });
});
