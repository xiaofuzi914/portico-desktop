import { afterEach, describe, expect, it, vi } from "vitest";
import { formatDateTime, formatRelativeTime, truncate } from "./formatters";

describe("formatters", () => {
  afterEach(() => vi.useRealTimers());
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

  it.each([
    [30_000, "second"],
    [30 * 60_000, "minute"],
    [3 * 60 * 60_000, "hour"],
    [3 * 24 * 60 * 60_000, "day"],
  ])("formats relative time at %sms using %s units", (ageMs, unit) => {
    const now = new Date("2026-07-10T12:00:00.000Z");
    vi.useFakeTimers();
    vi.setSystemTime(now);

    expect(formatRelativeTime(new Date(now.getTime() - ageMs))).toContain(unit);
  });

  it("keeps short strings unchanged", () => {
    expect(truncate("short", 10)).toBe("short");
  });
});
