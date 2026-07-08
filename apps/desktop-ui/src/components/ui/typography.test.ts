import { describe, expect, it } from "vitest";
import {
  typography,
  typographyLevelRank,
  typographyLevels,
  type TypographyLevel,
} from "./typography";

describe("typography hierarchy", () => {
  it("keeps page, section, item, and metadata levels in descending visual order", () => {
    const orderedLevels: TypographyLevel[] = [
      "pageTitle",
      "pageDescription",
      "sectionTitle",
      "itemTitle",
      "metadata",
    ];

    expect(orderedLevels.map((level) => typographyLevelRank[level])).toEqual([5, 4, 3, 2, 1]);
  });

  it("keeps reusable card titles below page titles", () => {
    expect(typography.cardTitle).not.toContain("text-2xl");
    expect(typography.cardTitle).toContain("text-base");
    expect(typographyLevelRank.cardTitle).toBeLessThan(typographyLevelRank.pageTitle);
  });

  it("defines every exported typography level", () => {
    expect(Object.keys(typography).sort()).toEqual([...typographyLevels].sort());
  });
});
