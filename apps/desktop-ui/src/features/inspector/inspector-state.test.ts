import { describe, expect, it } from "vitest";
import { isInspectorTab, readInspectorCollapsed, writeInspectorCollapsed } from "./inspector-state";

describe("inspector state", () => {
  it("accepts only registered tabs", () => {
    expect(isInspectorTab("timeline")).toBe(true);
    expect(isInspectorTab("audit")).toBe(true);
    expect(isInspectorTab("files")).toBe(true);
    expect(isInspectorTab("terminal")).toBe(false);
  });

  it("persists the collapsed inspector preference", () => {
    writeInspectorCollapsed(true);
    expect(readInspectorCollapsed()).toBe(true);
    writeInspectorCollapsed(false);
    expect(readInspectorCollapsed()).toBe(false);
  });
});
