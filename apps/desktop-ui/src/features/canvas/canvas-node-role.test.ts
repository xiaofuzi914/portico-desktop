import { describe, expect, it } from "vitest";
import { asCanvasId, asCanvasNodeId, type CanvasNode } from "@/lib/schemas";
import {
  applyRoleToNode,
  kindFromRole,
  roleFromNode,
} from "./canvas-node-role";

function node(partial: Partial<CanvasNode> = {}): CanvasNode {
  const now = new Date().toISOString();
  return {
    id: asCanvasNodeId("550e8400-e29b-41d4-a716-446655440001"),
    canvas_id: asCanvasId("550e8400-e29b-41d4-a716-446655440000"),
    kind: "Note",
    title: "t",
    summary: "",
    status: "Todo",
    parent_id: null,
    position_x: 0,
    position_y: 0,
    layout_rank: 0,
    source: "User",
    payload_json: "{}",
    created_at: now,
    updated_at: now,
    ...partial,
  };
}

describe("canvas-node-role", () => {
  it("maps note/goal/stage kinds to roles", () => {
    expect(roleFromNode(node({ kind: "Note" }))).toBe("note");
    expect(roleFromNode(node({ kind: "Goal" }))).toBe("goal");
    expect(roleFromNode(node({ kind: "Stage" }))).toBe("stage");
  });

  it("maps insight branch payload to intent/progress/conclusion", () => {
    expect(
      roleFromNode(
        node({
          kind: "Insight",
          payload_json: JSON.stringify({
            narrative_role: "leaf",
            branch: "intent",
          }),
        }),
      ),
    ).toBe("intent");
    expect(
      roleFromNode(
        node({
          kind: "Insight",
          payload_json: JSON.stringify({ branch: "progress" }),
        }),
      ),
    ).toBe("progress");
  });

  it("applies role back to kind + payload", () => {
    const base = node({ kind: "Note", payload_json: "{}" });
    const intent = applyRoleToNode(base, "intent");
    expect(intent.kind).toBe("Insight");
    expect(JSON.parse(intent.payload_json).branch).toBe("intent");
    expect(kindFromRole("stage")).toBe("Stage");
  });
});
