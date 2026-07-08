import { describe, expect, it } from "vitest";
import { asWorkspaceId, type Workspace } from "@/lib/schemas";
import { buildProjectOverviewItems } from "./-overview-model";

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

describe("project overview model", () => {
  it("sorts existing projects by most recently updated", () => {
    const items = buildProjectOverviewItems([
      workspace("older", "Older", "2026-07-07T00:00:00.000Z"),
      workspace("newer", "Newer", "2026-07-08T00:00:00.000Z"),
    ]);

    expect(items.map((item) => item.workspace.name)).toEqual(["Newer", "Older"]);
  });

  it("summarizes the project path and permission footprint", () => {
    const project = {
      ...workspace("project", "Project", "2026-07-08T00:00:00.000Z"),
      trusted: true,
      allowed_read_paths: ["/read/a", "/read/b"],
      allowed_write_paths: ["/write"],
    };

    const [item] = buildProjectOverviewItems([project]);

    expect(item).toMatchObject({
      workspace: project,
      readPathCount: 2,
      writePathCount: 1,
      isTrusted: true,
    });
  });
});
