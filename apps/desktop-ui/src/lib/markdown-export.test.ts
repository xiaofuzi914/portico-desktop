import { describe, expect, it } from "vitest";
import { renderedMarkdownToDocxChildren } from "./markdown-docx";
import { buildStandaloneMarkdownHtml } from "./markdown-export";

describe("markdown export", () => {
  it("builds a self-contained UTF-8 HTML document", () => {
    const html = buildStandaloneMarkdownHtml("A & B", "<h1>你好</h1>");
    expect(html).toContain('<meta charset="utf-8">');
    expect(html).toContain("<title>A &amp; B</title>");
    expect(html).toContain("<main><h1>你好</h1></main>");
    expect(html).toContain("<style>");
  });

  it("converts common rendered Markdown blocks to DOCX children", () => {
    const root = document.createElement("article");
    root.innerHTML =
      "<h1>标题</h1><p>Hello <strong>world</strong></p><ul><li>one</li><li>two</li></ul><table><tbody><tr><td>A</td><td>B</td></tr></tbody></table>";
    expect(renderedMarkdownToDocxChildren(root)).toHaveLength(5);
  });
});
