import { describe, expect, it } from "vitest";
import { classifyTaskMode } from "./classify-task-mode";

describe("classifyTaskMode", () => {
  it("defaults short chat to single agent", () => {
    const r = classifyTaskMode("你好");
    expect(r.mode.kind).toBe("single");
    expect(r.confidence).toBe("high");
  });

  it("picks multi-lens for review language", () => {
    const r = classifyTaskMode("请做一次代码审查，多角度检查 auth 模块风险");
    expect(r.mode.kind).toBe("multi");
    if (r.mode.kind !== "multi") return;
    expect(r.mode.workflowId).toBe("multi-lens-review");
  });

  it("picks deep-research for architecture dig", () => {
    const r = classifyTaskMode("请深入摸清这个项目的架构与来龙去脉，找出证据与缺口");
    expect(r.mode.kind).toBe("multi");
    if (r.mode.kind !== "multi") return;
    expect(r.mode.workflowId).toBe("deep-research");
  });

  it("picks iterative-refine for polish language", () => {
    const r = classifyTaskMode("把这份方案反复打磨到能交差");
    expect(r.mode.kind).toBe("multi");
    if (r.mode.kind !== "multi") return;
    expect(r.mode.workflowId).toBe("iterative-refine");
  });

  it("stays single when multi-agent is not ready", () => {
    const r = classifyTaskMode("请做一次全面代码审查和安全审计", [], false);
    expect(r.mode.kind).toBe("single");
  });

  it("uses adaptive for clear multi-step delivery with roles", () => {
    const r = classifyTaskMode(
      "请先梳理项目结构，并且再做一次安全审计，给出可执行改动清单",
    );
    // Either adaptive multi or a specific recipe — never single for this length.
    expect(r.mode.kind).toBe("multi");
  });
});
