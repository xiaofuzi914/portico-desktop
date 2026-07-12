import { describe, expect, it } from "vitest";
import { renderToString } from "react-dom/server";
import { MarkdownBody } from "./markdown-body";

describe("MarkdownBody", () => {
  it("renders GFM tables and task lists", () => {
    const markdown = "# Hello\n\n- [x] Done\n\n| Name | Value |\n| --- | --- |\n| Portico | 1 |";
    const html = renderToString(<MarkdownBody content={markdown} />);
    expect(html).toContain("<h1>Hello</h1>");
    expect(html).toContain('type="checkbox"');
    expect(html).toContain("<table>");
  });

  it("renders KaTeX math from $ delimiters", () => {
    const html = renderToString(<MarkdownBody content={"Euler: $E = mc^2$"} />);
    expect(html).toContain("katex");
    expect(html).toMatch(/E|mc/);
  });

  it("renders mermaid fences as a client surface (SSR shows source)", () => {
    const markdown = '```mermaid\npie title Revenue\n"A" : 40\n"B" : 60\n```';
    const html = renderToString(<MarkdownBody content={markdown} />);
    expect(html).toContain("markdown-mermaid");
    expect(html).toContain("pie title Revenue");
  });

  it("does not execute raw HTML", () => {
    const html = renderToString(
      <MarkdownBody content={'<script>alert("xss")</script>\n\n**ok**'} />,
    );
    expect(html).not.toContain("<script>");
    expect(html).toContain("<strong>ok</strong>");
  });

  it("preserves ASCII architecture diagrams without highlight spans", () => {
    const markdown = [
      "```",
      "  ┌─────────┐       ┌───────────┐",
      "  │ 💬 聊天  │  ──▶  │ 🧠 思考   │",
      "  └─────────┘       └───────────┘",
      "```",
    ].join("\n");
    const html = renderToString(<MarkdownBody content={markdown} />);
    expect(html).toContain("markdown-ascii-diagram");
    expect(html).toContain("┌─────────┐");
    expect(html).toContain("💬 聊天");
    // highlight.js spans would break column alignment
    expect(html).not.toContain("hljs-");
  });

  it("still highlights explicit language fences", () => {
    const markdown = "```javascript\nconst x = 1;\n```";
    const html = renderToString(<MarkdownBody content={markdown} />);
    expect(html).toContain("hljs");
    expect(html).toContain("const");
  });
});
