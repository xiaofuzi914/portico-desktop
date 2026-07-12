import { afterEach, describe, expect, it } from "vitest";
import {
  DEFAULT_INSPECTOR_WIDTH,
  MAX_INSPECTOR_WIDTH,
  MIN_INSPECTOR_WIDTH,
  clampInspectorWidth,
  readInspectorWidth,
  readSidebarCollapsed,
  writeInspectorWidth,
  writeSidebarCollapsed,
} from "./shell-layout-state";

describe("shell layout state", () => {
  afterEach(() => {
    window.localStorage.clear();
  });

  it("clamps inspector width to product bounds", () => {
    expect(clampInspectorWidth(100, 2000)).toBe(MIN_INSPECTOR_WIDTH);
    expect(clampInspectorWidth(900, 2000)).toBe(MAX_INSPECTOR_WIDTH);
    expect(clampInspectorWidth(500, 2000)).toBe(500);
  });

  it("keeps a usable main pane when the container is narrow", () => {
    // container 800, min main 360 → max inspector 440
    expect(clampInspectorWidth(600, 800)).toBe(440);
    expect(clampInspectorWidth(320, 800)).toBe(320);
  });

  it("persists sidebar collapsed preference", () => {
    writeSidebarCollapsed(true);
    expect(readSidebarCollapsed()).toBe(true);
    writeSidebarCollapsed(false);
    expect(readSidebarCollapsed()).toBe(false);
  });

  it("persists inspector width and rejects invalid storage values", () => {
    writeInspectorWidth(512);
    expect(readInspectorWidth()).toBe(512);

    window.localStorage.setItem("portico.shell.inspector.width", "not-a-number");
    expect(readInspectorWidth()).toBe(DEFAULT_INSPECTOR_WIDTH);

    writeInspectorWidth(50);
    expect(readInspectorWidth()).toBe(MIN_INSPECTOR_WIDTH);
  });
});
