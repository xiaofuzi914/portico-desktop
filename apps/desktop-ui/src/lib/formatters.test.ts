import { describe, expect, it } from "vitest";
import { formatDateTime, formatRelativeTime, truncate } from "./formatters";

describe("formatters", () => {
  it("formats a date", () => {
    const result = formatDateTime("2026-01-15T10:30:00.000Z");
    expect(result).toBeTruthy();
  });

  it("formats relative time", () => {
    const now = new Date();
    const result = formatRelativeTime(now);
    expect(result).toContain("now");
  });

  it("truncates long text", () => {
    expect(truncate("hello world", 5)).toBe("hello…");
  });
});
