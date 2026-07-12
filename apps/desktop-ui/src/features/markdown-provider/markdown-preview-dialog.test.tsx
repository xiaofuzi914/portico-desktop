import { renderToString } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { MarkdownPreviewDialog } from "./markdown-preview-dialog";
import type { ArtifactPreview } from "@/lib/schemas";

vi.mock("./markdown-workspace-preview", () => ({
  MarkdownWorkspacePreview: () => <div data-testid="markdown-preview" />,
}));

describe("MarkdownPreviewDialog", () => {
  it("renders an accessible large preview surface", () => {
    const preview: ArtifactPreview = {
      path: "/tmp/guide.md",
      mime_type: "text/markdown",
      content_base64: "IyBHdWlkZQ==",
      size_bytes: 7,
    };
    const html = renderToString(<MarkdownPreviewDialog preview={preview} onClose={() => {}} />);
    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain("guide.md");
    expect(html).toContain("92vw");
  });
});
