import { describe, expect, it } from "vitest";
import { asWorkspaceId, type Workspace } from "@/lib/schemas";
import { buildAllowedPathsSummary } from "./allowed-paths-summary-model";

function workspace(id: string, name: string): Workspace {
  return {
    id: asWorkspaceId(id),
    name,
    root_path: `/tmp/${name}`,
    trusted: false,
    allowed_read_paths: [],
    allowed_write_paths: [],
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-07-08T00:00:00.000Z",
  };
}

describe("allowed paths summary model", () => {
  it("returns only projects with recorded allowed paths", () => {
    const empty = workspace("empty", "Empty");
    const configured = {
      ...workspace("configured", "Configured"),
      allowed_read_paths: ["/read"],
      allowed_write_paths: ["/write/a", "/write/b"],
    };

    const items = buildAllowedPathsSummary([empty, configured]);

    expect(items).toEqual([
      {
        workspace: configured,
        readPaths: ["/read"],
        writePaths: ["/write/a", "/write/b"],
        totalCount: 3,
      },
    ]);
  });

  it("filters the summary to one workspace when a workspace id is provided", () => {
    const first = {
      ...workspace("first", "First"),
      allowed_read_paths: ["/first"],
    };
    const second = {
      ...workspace("second", "Second"),
      allowed_write_paths: ["/second"],
    };

    const items = buildAllowedPathsSummary([first, second], asWorkspaceId("second"));

    expect(items.map((item) => item.workspace.name)).toEqual(["Second"]);
    expect(items[0]?.writePaths).toEqual(["/second"]);
  });
});
