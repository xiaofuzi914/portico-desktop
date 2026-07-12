import { describe, expect, it } from "vitest";
import {
  looksLikeAsciiDiagram,
  shouldRenderAsPlainCode,
  languageFromClassName,
} from "./markdown-code";

describe("markdown-code", () => {
  const asciiSample = `
                              AI 进化三阶段

   阶段一              阶段二               阶段三
  ┌─────────┐       ┌───────────┐       ┌──────────────┐
  │ 💬 聊天  │  ──▶  │ 🧠 思考   │  ──▶  │ 🏃 自主执行   │
  │ 问答机器人│       │ 理解+拆解  │       │ Agent 干活   │
  └─────────┘       └───────────┘       └──────────────┘
`.trim();

  it("detects box-drawing architecture diagrams", () => {
    expect(looksLikeAsciiDiagram(asciiSample)).toBe(true);
  });

  it("does not treat normal prose as diagrams", () => {
    expect(looksLikeAsciiDiagram("hello\nworld\nfoo")).toBe(false);
  });

  it("renders unlabeled fences as plain (protects ASCII art)", () => {
    expect(shouldRenderAsPlainCode(asciiSample, null)).toBe(true);
    expect(shouldRenderAsPlainCode("const x = 1;\n", null)).toBe(true);
  });

  it("renders explicit text/ascii fences as plain", () => {
    expect(shouldRenderAsPlainCode("x", "text")).toBe(true);
    expect(shouldRenderAsPlainCode("x", "ascii")).toBe(true);
    expect(shouldRenderAsPlainCode("x", "diagram")).toBe(true);
  });

  it("allows known languages to highlight when not diagram-like", () => {
    expect(shouldRenderAsPlainCode("const x = 1;\nconsole.log(x);\n", "javascript")).toBe(false);
  });

  it("forces plain when box art is mislabeled as a code language", () => {
    expect(shouldRenderAsPlainCode(asciiSample, "javascript")).toBe(true);
  });

  it("parses language class names", () => {
    expect(languageFromClassName("language-mermaid")).toBe("mermaid");
    expect(languageFromClassName("language-JS language-foo")).toBe("js");
    expect(languageFromClassName(undefined)).toBeNull();
  });
});
