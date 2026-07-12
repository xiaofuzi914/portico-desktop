import { describe, expect, it } from "vitest";
import { asWorkspaceId, type Workspace } from "@/lib/schemas";
import { buildSidebarProjectItems } from "./sidebar-projects-model";

function workspace(id: string, name: string, updatedAt: string): Workspace {
  return {
    id: asWorkspaceId(id),
    name,
    root_path: `/tmp/${name}`,
    trusted: false,
    allowed_read_paths: [],
    allowed_write_paths: [],
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: updatedAt,
  };
}

describe("sidebar project model", () => {
  it("shows all projects when the list fits in the sidebar", () => {
    const items = buildSidebarProjectItems([
      workspace("a", "Alpha", "2026-07-07T00:00:00.000Z"),
      workspace("b", "Beta", "2026-07-08T00:00:00.000Z"),
    ]);

    expect(items.map((item) => item.kind)).toEqual(["workspace", "workspace"]);
    expect(items.map((item) => (item.kind === "workspace" ? item.workspace.name : ""))).toEqual([
      "Beta",
      "Alpha",
    ]);
  });

  it("collapses overflow into an all-projects entry plus prioritized projects", () => {
    const items = buildSidebarProjectItems(
      [
        workspace("a", "Alpha", "2026-07-01T00:00:00.000Z"),
        workspace("b", "Beta", "2026-07-06T00:00:00.000Z"),
        workspace("c", "Current", "2026-07-02T00:00:00.000Z"),
        workspace("d", "Delta", "2026-07-05T00:00:00.000Z"),
        workspace("e", "Echo", "2026-07-04T00:00:00.000Z"),
        workspace("f", "Foxtrot", "2026-07-03T00:00:00.000Z"),
      ],
      { runningWorkspaceIds: new Set([asWorkspaceId("c")]) },
    );

    expect(items).toHaveLength(5);
    expect(items[0]?.kind).toBe("overview");
    expect(
      items.slice(1).map((item) => (item.kind === "workspace" ? item.workspace.name : "")),
    ).toEqual(["Current", "Beta", "Delta", "Echo"]);
  });

  it("prefers explicit last-used timestamps before workspace update times", () => {
    const items = buildSidebarProjectItems(
      [
        workspace("a", "Alpha", "2026-07-08T00:00:00.000Z"),
        workspace("b", "Beta", "2026-07-07T00:00:00.000Z"),
      ],
      {
        lastUsedAtByWorkspaceId: new Map([["b", "2026-07-09T00:00:00.000Z"]]),
      },
    );

    expect(items.map((item) => (item.kind === "workspace" ? item.workspace.name : ""))).toEqual([
      "Beta",
      "Alpha",
    ]);
  });
});
