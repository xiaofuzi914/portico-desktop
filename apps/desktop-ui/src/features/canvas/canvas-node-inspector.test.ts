import { describe, expect, it } from "vitest";
import { asCanvasId, asCanvasNodeId, type CanvasNode } from "@/lib/schemas";
import { buildNodeChatPrompt, parseStagePayload } from "./canvas-node-inspector";

function node(partial: Partial<CanvasNode> & Pick<CanvasNode, "kind" | "title">): CanvasNode {
  const now = new Date().toISOString();
  return {
    id: asCanvasNodeId("550e8400-e29b-41d4-a716-446655440001"),
    canvas_id: asCanvasId("550e8400-e29b-41d4-a716-446655440000"),
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

describe("buildNodeChatPrompt", () => {
  it("uses note title and summary", () => {
    const prompt = buildNodeChatPrompt(
      node({ kind: "Note", title: "登录改版", summary: "先做手机端" }),
    );
    expect(prompt).toContain("登录改版");
    expect(prompt).toContain("先做手机端");
  });

  it("prefers stage suggested_prompt", () => {
    const prompt = buildNodeChatPrompt(
      node({
        kind: "Stage",
        title: "阶段 A",
        summary: "说明",
        payload_json: JSON.stringify({ suggested_prompt: "直接执行这个" }),
      }),
    );
    expect(prompt).toBe("直接执行这个");
  });

  it("falls back to structured stage prompt", () => {
    const prompt = buildNodeChatPrompt(
      node({ kind: "Stage", title: "阶段 B", summary: "细节" }),
    );
    expect(prompt).toContain("【任务】阶段 B");
    expect(prompt).toContain("【说明】细节");
  });
});

describe("parseStagePayload", () => {
  it("returns empty object on invalid json", () => {
    expect(parseStagePayload("not-json")).toEqual({});
  });
});
