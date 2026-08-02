import { describe, expect, it } from "vitest";
import type { Thread, ThreadId, WorkspaceId } from "@/lib/schemas";
import {
  buildSessionTreeRows,
  expandAncestorsForActive,
} from "./sidebar-session-tree";

function thread(
  id: string,
  title: string,
  opts?: { parent?: string | null; updated?: string },
): Thread {
  return {
    id: id as ThreadId,
    workspace_id: "ws-1" as WorkspaceId,
    title,
    parent_thread_id: (opts?.parent ?? null) as ThreadId | null,
    archived_at: null,
    created_at: "2026-08-01T00:00:00.000Z",
    updated_at: opts?.updated ?? "2026-08-01T12:00:00.000Z",
  };
}

describe("buildSessionTreeRows", () => {
  it("keeps flat roots when no parent links", () => {
    const rows = buildSessionTreeRows([
      thread("a", "A", { updated: "2026-08-02T00:00:00.000Z" }),
      thread("b", "B", { updated: "2026-08-01T00:00:00.000Z" }),
    ]);
    expect(rows.map((r) => r.thread.id)).toEqual(["a", "b"]);
    expect(rows.every((r) => r.depth === 0 && !r.isBranch)).toBe(true);
  });

  it("nests children under parents with progressive depth", () => {
    const rows = buildSessionTreeRows([
      thread("parent", "Ds-pro", { updated: "2026-08-03T00:00:00.000Z" }),
      thread("child", "子会话问题", {
        parent: "parent",
        updated: "2026-08-03T01:00:00.000Z",
      }),
      thread("other", "ds-flash", { updated: "2026-08-02T00:00:00.000Z" }),
    ]);
    expect(rows.map((r) => [r.thread.id, r.depth, r.isBranch])).toEqual([
      ["parent", 0, false],
      ["child", 1, true],
      ["other", 0, false],
    ]);
    expect(rows[0]?.childCount).toBe(1);
  });

  it("treats missing parent as root (orphan branch)", () => {
    const rows = buildSessionTreeRows([
      thread("orphan", "孤立子", { parent: "gone" }),
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.depth).toBe(0);
    expect(rows[0]?.isBranch).toBe(false);
  });

  it("hides children when parent is collapsed", () => {
    const rows = buildSessionTreeRows(
      [
        thread("p", "Parent"),
        thread("c", "Child", { parent: "p" }),
      ],
      new Set(["p"]),
    );
    expect(rows.map((r) => r.thread.id)).toEqual(["p"]);
    expect(rows[0]?.childCount).toBe(1);
  });

  it("orders siblings by updated_at desc", () => {
    const rows = buildSessionTreeRows([
      thread("p", "P"),
      thread("c1", "old", { parent: "p", updated: "2026-08-01T00:00:00.000Z" }),
      thread("c2", "new", { parent: "p", updated: "2026-08-05T00:00:00.000Z" }),
    ]);
    expect(rows.map((r) => r.thread.id)).toEqual(["p", "c2", "c1"]);
  });
});

describe("expandAncestorsForActive", () => {
  it("expands collapsed ancestors of the active child", () => {
    const threads = [
      thread("p", "P"),
      thread("c", "C", { parent: "p" }),
    ];
    const collapsed = new Set(["p"]);
    const next = expandAncestorsForActive(threads, "c" as ThreadId, collapsed);
    expect(next.has("p")).toBe(false);
  });
});
