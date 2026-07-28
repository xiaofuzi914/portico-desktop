import { describe, expect, it } from "vitest";
import { expandOffsets } from "./selection-expand";

describe("expandOffsets", () => {
  const sample =
    "前言。BaseAgent 作为抽象基类，通过 LangGraph StateGraph 构建。\n第二段内容。";

  it("expands to word (latin / cjk run)", () => {
    const idx = sample.indexOf("LangGraph");
    const r = expandOffsets(sample, idx + 2, idx + 4, "word");
    expect(sample.slice(r.start, r.end)).toBe("LangGraph");
  });

  it("expands to sentence", () => {
    const idx = sample.indexOf("LangGraph");
    const r = expandOffsets(sample, idx, idx + 3, "sentence");
    const sentence = sample.slice(r.start, r.end);
    expect(sentence).toContain("LangGraph StateGraph");
    expect(sentence.startsWith("BaseAgent") || sentence.includes("BaseAgent")).toBe(true);
  });

  it("expands to full paragraph text", () => {
    const r = expandOffsets(sample, 5, 10, "paragraph");
    expect(r.start).toBe(0);
    expect(r.end).toBe(sample.length);
  });
});
