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

  it("renders markdown as formatted GFM", () => {
    const markdown = "# Hello\n\n- [x] Done\n\n| Name | Value |\n| --- | --- |\n| Portico | 1 |";
    const preview = makePreview("text/markdown", Buffer.from(markdown, "utf8").toString("base64"));
    const html = renderToString(<ArtifactPreview preview={preview} />);
    expect(html).toContain("<h1>Hello</h1>");
    expect(html).toContain('type="checkbox"');
    expect(html).toContain("<table>");
    expect(html).not.toContain("# Hello");
  });

  it("decodes UTF-8 markdown", () => {
    const preview = makePreview(
      "text/markdown",
      Buffer.from("# 中文预览", "utf8").toString("base64"),
    );
    const html = renderToString(<ArtifactPreview preview={preview} />);
    expect(html).toContain("中文预览");
    expect(html).not.toContain("ä¸­æ");
  });

  it("does not render raw HTML from markdown", () => {
    const preview = makePreview(
      "text/markdown",
      Buffer.from('<script>alert("xss")</script>', "utf8").toString("base64"),
    );
    const html = renderToString(<ArtifactPreview preview={preview} />);
    expect(html).not.toContain("<script>");
  });

  it("does not navigate links or load remote images", () => {
    const markdown = "[Docs](https://example.com) ![tracking](https://example.com/pixel.png)";
    const preview = makePreview("text/markdown", Buffer.from(markdown, "utf8").toString("base64"));
    const html = renderToString(<ArtifactPreview preview={preview} />);
    expect(html).not.toContain("<a href");
    expect(html).toContain('title="https://example.com"');
    expect(html).not.toContain("<img");
    expect(html).toContain("tracking");
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
