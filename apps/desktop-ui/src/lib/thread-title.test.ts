import { describe, expect, it } from "vitest";
import { deriveThreadTitle, isDefaultThreadTitle } from "./thread-title";

describe("isDefaultThreadTitle", () => {
  it("treats placeholders as default", () => {
    expect(isDefaultThreadTitle("新会话")).toBe(true);
    expect(isDefaultThreadTitle("New session")).toBe(true);
    expect(isDefaultThreadTitle("新会话 2")).toBe(true);
    expect(isDefaultThreadTitle("")).toBe(true);
    expect(isDefaultThreadTitle(null)).toBe(true);
  });

  it("treats user topics as non-default", () => {
    expect(isDefaultThreadTitle("画功能全景图")).toBe(false);
    expect(isDefaultThreadTitle("Yuxi architecture")).toBe(false);
  });
});

describe("deriveThreadTitle", () => {
  it("uses first line and strips task wrappers when short enough", () => {
    expect(deriveThreadTitle("【任务】画架构图\n细节…")).toBe("画架构图");
  });

  it("truncates to at most 10 characters including ellipsis", () => {
    const long = "给我一个完整的功能图，生成一份完整的md";
    const title = deriveThreadTitle(long);
    expect([...title].length).toBeLessThanOrEqual(10);
    expect(title.endsWith("…")).toBe(true);
  });

  it("returns empty for blank input", () => {
    expect(deriveThreadTitle("   \n  ")).toBe("");
  });
});
