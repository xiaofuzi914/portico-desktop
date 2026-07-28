import { describe, expect, it } from "vitest";
import { asWorkspaceId, type Workspace } from "@/lib/schemas";
import {
  buildAllowedPathsSummary,
  buildFolderAllowlist,
  flattenReadPaths,
  flattenWritePaths,
} from "./allowed-paths-summary-model";

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
  it("merges same path read+write into one folder with both tags", () => {
    const ws = {
      ...workspace("yuxi", "Yuxi"),
      root_path: "/Users/owen/Program/Yuxi",
      allowed_read_paths: ["/Users/owen/Program/Yuxi"],
      allowed_write_paths: ["/Users/owen/Program/Yuxi"],
    };

    const folders = buildFolderAllowlist(ws);
    expect(folders).toHaveLength(1);
    expect(folders[0]).toMatchObject({
      name: "Yuxi",
      path: "/Users/owen/Program/Yuxi",
      isProjectRoot: true,
      canRead: true,
      canWrite: true,
    });
  });

  it("lists distinct folders with independent permissions", () => {
    const ws = {
      ...workspace("configured", "Configured"),
      root_path: "/tmp/Configured",
      allowed_read_paths: ["/tmp/Configured", "/tmp/Configured/docs"],
      allowed_write_paths: ["/tmp/Configured"],
    };

    const folders = buildFolderAllowlist(ws);
    expect(folders.map((f) => f.name)).toEqual(["Configured", "docs"]);
    const root = folders.find((f) => f.isProjectRoot)!;
    const docs = folders.find((f) => f.name === "docs")!;
    expect(root.canRead && root.canWrite).toBe(true);
    expect(docs.canRead).toBe(true);
    expect(docs.canWrite).toBe(false);
  });

  it("returns only projects with recorded folders and filters by workspace id", () => {
    const empty = workspace("empty", "Empty");
    const configured = {
      ...workspace("configured", "Configured"),
      allowed_read_paths: ["/read"],
      allowed_write_paths: ["/write/a", "/write/b"],
    };

    const items = buildAllowedPathsSummary([empty, configured]);
    expect(items).toHaveLength(1);
    expect(items[0]?.folders.length).toBeGreaterThanOrEqual(2);
    expect(flattenReadPaths(items[0]!)).toContain("/read");
    expect(flattenWritePaths(items[0]!)).toEqual(
      expect.arrayContaining(["/write/a", "/write/b"]),
    );

    const first = {
      ...workspace("first", "First"),
      allowed_read_paths: ["/first"],
    };
    const second = {
      ...workspace("second", "Second"),
      allowed_write_paths: ["/second"],
    };
    const filtered = buildAllowedPathsSummary([first, second], asWorkspaceId("second"));
    expect(filtered.map((item) => item.workspace.name)).toEqual(["Second"]);
    expect(filtered[0]?.folders.some((f) => f.path === "/second" && f.canWrite)).toBe(
      true,
    );
  });
});
