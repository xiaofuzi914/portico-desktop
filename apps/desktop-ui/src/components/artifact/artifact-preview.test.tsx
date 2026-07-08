import { describe, expect, it } from "vitest";
import { renderToString } from "react-dom/server";
import { ArtifactPreview } from "./artifact-preview";
import type { ArtifactPreview as ArtifactPreviewType } from "@/lib/schemas";

function makePreview(
  mimeType: string,
  contentBase64: string,
  path = "/tmp/file",
): ArtifactPreviewType {
  return {
    path,
    mime_type: mimeType,
    content_base64: contentBase64,
    size_bytes: contentBase64.length,
  };
}

describe("ArtifactPreview", () => {
  it("renders an image", () => {
    const preview = makePreview("image/png", "iVBORw0KGgo=");
    const html = renderToString(<ArtifactPreview preview={preview} />);
    expect(html).toContain("<img");
    expect(html).toContain("data:image/png;base64,iVBORw0KGgo=");
  });

  it("renders markdown as plain text", () => {
    const preview = makePreview("text/markdown", "IyBIZWxsbw==");
    const html = renderToString(<ArtifactPreview preview={preview} />);
    expect(html).toContain("# Hello");
    expect(html).not.toContain("<img");
  });

  it("renders a CSV as a table", () => {
    const preview = makePreview("text/csv", "YSxiCmMsZA==");
    const html = renderToString(<ArtifactPreview preview={preview} />);
    expect(html).toContain("<table");
    expect(html).toContain("a");
    expect(html).toContain("b");
    expect(html).toContain("c");
    expect(html).toContain("d");
  });

  it("renders a PDF with an object tag", () => {
    const preview = makePreview("application/pdf", "JVBERi0xLg==");
    const html = renderToString(<ArtifactPreview preview={preview} />);
    expect(html).toContain("<object");
    expect(html).toContain('type="application/pdf"');
    expect(html).toContain('data="data:application/pdf;base64,JVBERi0xLg=="');
  });

  it("renders plain text in a pre block", () => {
    const preview = makePreview("text/plain", "SGVsbG8gV29ybGQ=");
    const html = renderToString(<ArtifactPreview preview={preview} />);
    expect(html).toContain("<pre");
    expect(html).toContain("Hello World");
  });
});
