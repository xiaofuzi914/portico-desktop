import { describe, expect, it } from "vitest";
import { hasProjectContext } from "./app-shell-context";

describe("app shell project context", () => {
  it("hides project-scoped surfaces on global routes", () => {
    expect(hasProjectContext({})).toBe(false);
    expect(hasProjectContext({ workspaceId: "" })).toBe(false);
  });

  it("shows project-scoped surfaces inside a project route", () => {
    expect(hasProjectContext({ workspaceId: "550e8400-e29b-41d4-a716-446655440000" })).toBe(true);
  });
});
