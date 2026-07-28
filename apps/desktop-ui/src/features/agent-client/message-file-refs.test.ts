import { describe, expect, it } from "vitest";
import {
  collectFileRefsFromRunEvents,
  extractPathsFromText,
  mergeFileRefs,
} from "./message-file-refs";
import type { RunEvent } from "@/lib/schemas";
import { asAgentRunId, asThreadId } from "@/lib/schemas";

describe("extractPathsFromText", () => {
  it("finds backtick and bare markdown paths", () => {
    const text =
      "已产出 `ARCHITECTURE_DEEP.md`，另见 docs/guide.md 与 plain ARCHITECTURE.md 文件。";
    const paths = extractPathsFromText(text).map((p) => p.path);
    expect(paths).toContain("ARCHITECTURE_DEEP.md");
    expect(paths).toContain("docs/guide.md");
    expect(paths).toContain("ARCHITECTURE.md");
  });

  it("finds source files", () => {
    const paths = extractPathsFromText("Updated `src/app.ts` and crates/foo/bar.rs").map(
      (p) => p.path,
    );
    expect(paths).toContain("src/app.ts");
    expect(paths).toContain("crates/foo/bar.rs");
  });
});

describe("collectFileRefsFromRunEvents", () => {
  it("reads path from ToolRequested fs_write arguments", () => {
    const events: RunEvent[] = [
      {
        id: 1,
        run_id: asAgentRunId("11111111-1111-4111-8111-111111111111"),
        thread_id: asThreadId("22222222-2222-4222-8222-222222222222"),
        sequence: 1,
        event_type: "ToolRequested",
        payload: {
          kind: "ToolRequested",
          data: {
            run_id: "11111111-1111-4111-8111-111111111111",
            tool_name: "fs_write",
            arguments: { path: "ARCHITECTURE_DEEP.md", content: "..." },
          },
        },
        created_at: "2026-07-21T00:00:00.000Z",
      },
    ];
    const refs = collectFileRefsFromRunEvents(events);
    expect(refs).toEqual([{ path: "ARCHITECTURE_DEEP.md", kind: "written" }]);
  });
});

describe("mergeFileRefs", () => {
  it("prefers written over mentioned", () => {
    const merged = mergeFileRefs(
      [{ path: "a.md", kind: "written" }],
      [{ path: "a.md", kind: "mentioned" }, { path: "b.md", kind: "mentioned" }],
    );
    expect(merged.find((r) => r.path === "a.md")?.kind).toBe("written");
    expect(merged.map((r) => r.path).sort()).toEqual(["a.md", "b.md"]);
  });
});
