import { describe, expect, it } from "vitest";
import {
  isBlankPlaceholderNode,
  nextNodePosition,
  parseViewport,
  resolveEdgeHandles,
  serializeViewport,
  snapshotToFlow,
} from "./canvas-view-model";
import {
  asCanvasId,
  asCanvasNodeId,
  type CanvasNode,
  type CanvasSnapshot,
} from "@/lib/schemas";

const baseNode = (overrides: Partial<CanvasNode> = {}): CanvasNode => ({
  id: asCanvasNodeId("11111111-1111-4111-8111-111111111111"),
  canvas_id: asCanvasId("22222222-2222-4222-8222-222222222222"),
  kind: "Note",
  title: "Hello",
  summary: "body",
  status: "Todo",
  parent_id: null,
  position_x: 12,
  position_y: 34,
  layout_rank: 0,
  source: "User",
  payload_json: "{}",
  created_at: "2026-07-18T00:00:00.000Z",
  updated_at: "2026-07-18T00:00:00.000Z",
  ...overrides,
});

describe("canvas-view-model", () => {
  it("maps snapshot nodes and edges into flow graph", () => {
    const snapshot = {
      canvas: {
        id: "22222222-2222-4222-8222-222222222222",
        workspace_id: "ws",
        thread_id: null,
        title: "Project canvas",
        kind: "Project",
        viewport_json: '{"x":1,"y":2,"zoom":1.5}',
        revision: 1,
        last_extracted_at: null,
        created_at: "2026-07-18T00:00:00.000Z",
        updated_at: "2026-07-18T00:00:00.000Z",
      },
      nodes: [
        baseNode({ id: asCanvasNodeId("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"), title: "A" }),
        baseNode({
          id: asCanvasNodeId("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
          title: "B",
          position_x: 100,
          position_y: 200,
        }),
      ],
      edges: [
        {
          id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
          canvas_id: asCanvasId("22222222-2222-4222-8222-222222222222"),
          from_id: asCanvasNodeId("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
          to_id: asCanvasNodeId("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
          kind: "Parent",
          label: null,
          created_at: "2026-07-18T00:00:00.000Z",
        },
      ],
      links: [],
    } as unknown as CanvasSnapshot;

    const { nodes, edges } = snapshotToFlow(snapshot);
    expect(nodes).toHaveLength(2);
    expect(nodes[0]?.data.label).toBe("A");
    expect(nodes[1]?.position).toEqual({ x: 100, y: 200 });
    expect(edges).toHaveLength(1);
    expect(edges[0]?.source).toBe("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    // Directional arrow on target end
    expect(edges[0]?.markerEnd).toBeTruthy();
    expect(edges[0]?.sourceHandle).toBeTruthy();
    expect(edges[0]?.targetHandle).toBeTruthy();

    const goalOnly = snapshotToFlow(
      {
        ...snapshot,
        nodes: [
          ...snapshot.nodes,
          baseNode({
            id: asCanvasNodeId("dddddddd-dddd-4ddd-8ddd-dddddddddddd"),
            kind: "Goal",
            title: "G",
          }),
        ],
      },
      "goal",
    );
    expect(goalOnly.nodes.every((n) => n.data.kind === "Goal" || n.data.kind === "Stage")).toBe(
      true,
    );
    expect(goalOnly.nodes).toHaveLength(1);
  });

  it("hides blank placeholder stage/note when requested", () => {
    const session = baseNode({
      id: asCanvasNodeId("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
      kind: "ThreadCluster",
      title: "会话",
      summary: "摘要",
      source: "Auto",
    });
    const blankNote = baseNode({
      id: asCanvasNodeId("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
      kind: "Note",
      title: "新便签",
      summary: "",
      status: "Todo",
      source: "User",
    });
    const blankStage = baseNode({
      id: asCanvasNodeId("cccccccc-cccc-4ccc-8ccc-cccccccccccc"),
      kind: "Stage",
      title: "阶段",
      summary: "",
      status: "Todo",
      source: "User",
    });
    expect(isBlankPlaceholderNode(blankNote)).toBe(true);
    expect(isBlankPlaceholderNode(blankStage)).toBe(true);
    expect(isBlankPlaceholderNode(session)).toBe(false);

    const snapshot = {
      canvas: {
        id: "22222222-2222-4222-8222-222222222222",
        workspace_id: "ws",
        thread_id: null,
        title: "Project canvas",
        kind: "Project",
        viewport_json: "{}",
        revision: 1,
        last_extracted_at: null,
        created_at: "2026-07-18T00:00:00.000Z",
        updated_at: "2026-07-18T00:00:00.000Z",
      },
      nodes: [session, blankNote, blankStage],
      edges: [],
      links: [],
    } as unknown as CanvasSnapshot;

    const { nodes } = snapshotToFlow(snapshot, "all", null, true);
    expect(nodes).toHaveLength(1);
    expect(nodes[0]?.data.kind).toBe("ThreadCluster");
  });

  it("parses and serializes viewport JSON safely", () => {
    expect(parseViewport('{"x":3,"y":4,"zoom":2}')).toEqual({ x: 3, y: 4, zoom: 2 });
    expect(parseViewport("not-json")).toEqual({ x: 0, y: 0, zoom: 1 });
    expect(JSON.parse(serializeViewport({ x: 1, y: 2, zoom: 0.5 }))).toEqual({
      x: 1,
      y: 2,
      zoom: 0.5,
    });
  });

  it("places new nodes without stacking at origin", () => {
    const existing = [baseNode({ kind: "Note" }), baseNode({ kind: "Note" })];
    const pos = nextNodePosition(existing, "Note");
    expect(pos.y).toBeGreaterThan(80);
    expect(pos.rank).toBe(2);
  });

  it("resolves handles for vertical and horizontal structure", () => {
    // Parent → child below: arrow must leave bottom and enter top (points down).
    const down = resolveEdgeHandles({ x: 0, y: 0 }, { x: 0, y: 200 }, "Parent");
    expect(down).toEqual({ sourceHandle: "out-bottom", targetHandle: "in-top" });

    // Child above parent (rare): reverse handles so tip still points at target.
    const up = resolveEdgeHandles({ x: 0, y: 200 }, { x: 0, y: 0 }, "Parent");
    expect(up).toEqual({ sourceHandle: "out-top", targetHandle: "in-bottom" });

    // Same row: still force top/bottom so fan arms never use side arrows.
    const right = resolveEdgeHandles({ x: 0, y: 0 }, { x: 400, y: 0 }, "Parent");
    expect(right).toEqual({ sourceHandle: "out-bottom", targetHandle: "in-top" });

    // Branch (short) above leaf (full): height-aware centers still yield down.
    const branchToLeaf = resolveEdgeHandles(
      { x: 0, y: 0 },
      { x: 0, y: 120 },
      "Parent",
      72,
      104,
    );
    expect(branchToLeaf).toEqual({
      sourceHandle: "out-bottom",
      targetHandle: "in-top",
    });

    // Related edges to a goal on the right prefer a clean side exit.
    const relatedSide = resolveEdgeHandles(
      { x: 100, y: 100 },
      { x: 520, y: 140 },
      "Related",
    );
    expect(relatedSide.sourceHandle).toBe("out-right");
    expect(relatedSide.targetHandle).toBe("in-left");

    const relatedDown = resolveEdgeHandles(
      { x: 100, y: 0 },
      { x: 110, y: 220 },
      "Related",
    );
    expect(relatedDown.sourceHandle).toBe("out-bottom");
    expect(relatedDown.targetHandle).toBe("in-top");
  });

  it("draws Parent edges with markerEnd only (tip on child)", () => {
    const snapshot = {
      canvas: {
        id: "22222222-2222-4222-8222-222222222222",
        workspace_id: "ws",
        thread_id: null,
        title: "Project canvas",
        kind: "Project",
        viewport_json: "{}",
        revision: 1,
        last_extracted_at: null,
        created_at: "2026-07-18T00:00:00.000Z",
        updated_at: "2026-07-18T00:00:00.000Z",
      },
      nodes: [
        baseNode({
          id: asCanvasNodeId("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
          position_x: 0,
          position_y: 0,
        }),
        baseNode({
          id: asCanvasNodeId("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
          position_x: 0,
          position_y: 200,
        }),
      ],
      edges: [
        {
          id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
          canvas_id: asCanvasId("22222222-2222-4222-8222-222222222222"),
          from_id: asCanvasNodeId("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
          to_id: asCanvasNodeId("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
          kind: "Parent",
          label: null,
          created_at: "2026-07-18T00:00:00.000Z",
        },
      ],
      links: [],
    } as unknown as CanvasSnapshot;

    const { edges } = snapshotToFlow(snapshot);
    expect(edges[0]?.source).toBe("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    expect(edges[0]?.target).toBe("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
    expect(edges[0]?.sourceHandle).toBe("out-bottom");
    expect(edges[0]?.targetHandle).toBe("in-top");
    expect(edges[0]?.markerEnd).toBeTruthy();
    expect(edges[0]?.markerStart).toBeFalsy();
    // Parent edges use StructureEdge (forces Bottom→Top geometry).
    expect(edges[0]?.type).toBe("structure");
  });

  it("maps Related edges as quiet dashed links without clutter labels", () => {
    const snapshot = {
      canvas: {
        id: "22222222-2222-4222-8222-222222222222",
        workspace_id: "ws",
        thread_id: null,
        title: "Project canvas",
        kind: "Project",
        viewport_json: "{}",
        revision: 1,
        last_extracted_at: null,
        created_at: "2026-07-18T00:00:00.000Z",
        updated_at: "2026-07-18T00:00:00.000Z",
      },
      nodes: [
        baseNode({
          id: asCanvasNodeId("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
          position_x: 0,
          position_y: 0,
        }),
        baseNode({
          id: asCanvasNodeId("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
          position_x: 0,
          position_y: 200,
        }),
      ],
      edges: [
        {
          id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
          canvas_id: asCanvasId("22222222-2222-4222-8222-222222222222"),
          from_id: asCanvasNodeId("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
          to_id: asCanvasNodeId("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
          kind: "Related",
          label: "支撑",
          created_at: "2026-07-18T00:00:00.000Z",
        },
      ],
      links: [],
    } as unknown as CanvasSnapshot;

    const { edges } = snapshotToFlow(snapshot);
    expect(edges[0]?.animated).toBe(false);
    expect(String(edges[0]?.style?.strokeDasharray ?? "")).toContain("6");
    expect(edges[0]?.markerEnd).toBeTruthy();
    // Soft-link labels stay hidden to reduce visual noise.
    expect(edges[0]?.label).toBeUndefined();
  });
});
