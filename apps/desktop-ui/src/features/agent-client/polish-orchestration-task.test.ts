import { describe, expect, it } from "vitest";
import { polishOrchestrationTask, shouldSuggestMultiRole } from "./polish-orchestration-task";

describe("polishOrchestrationTask", () => {
  it("keeps empty input empty", () => {
    expect(polishOrchestrationTask("   ").polished).toBe("");
  });

  it("expands a casual Chinese security question", () => {
    const result = polishOrchestrationTask("帮我看看安不安全");
    expect(result.polished).toContain("【任务】");
    expect(result.polished).toContain("帮我看看安不安全");
    expect(result.suggestedRoles).toContain("security-reviewer");
    expect(result.suggestedRoles.length).toBeLessThanOrEqual(2);
    expect(result.adjustments.length).toBeGreaterThan(0);
  });

  it("uses a single explorer for project architecture questions", () => {
    const result = polishOrchestrationTask("详细看下这个项目，技术架构是什么");
    expect(result.suggestedRoles).toEqual(["explorer"]);
    expect(result.polished).toContain("【任务】");
  });

  it("uses a single default agent for vague questions", () => {
    const result = polishOrchestrationTask("你好，随便聊聊");
    expect(result.suggestedRoles).toEqual(["default"]);
  });

  it("prefers habit roles when patterns exist (capped at 2)", () => {
    const result = polishOrchestrationTask("随便看看", [
      {
        id: "550e8400-e29b-41d4-a716-446655440000" as never,
        name: "explore-first",
        summary: "always explore",
        preferred_roles: ["explorer", "reviewer", "tester"],
        collaboration_style: "concise",
        strength: 2,
        score: 3,
      },
    ]);
    expect(result.suggestedRoles[0]).toBe("explorer");
    expect(result.suggestedRoles.length).toBeLessThanOrEqual(2);
    expect(result.adjustments.some((a) => a.includes("explore-first"))).toBe(true);
  });

  it("suggests multi-role for specialist or multi-role plans only", () => {
    expect(shouldSuggestMultiRole(polishOrchestrationTask("帮我看看安不安全"))).toBe(true);
    expect(shouldSuggestMultiRole(polishOrchestrationTask("详细看下这个项目架构"))).toBe(true);
    expect(shouldSuggestMultiRole(polishOrchestrationTask("你好，随便聊聊"))).toBe(false);
  });
});
