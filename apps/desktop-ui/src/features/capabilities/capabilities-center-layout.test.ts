import { describe, expect, it } from "vitest";
import { CAPABILITIES_CENTER_CLASS } from "./capabilities-center";

describe("capabilities center layout", () => {
  it("owns a full-height vertical scroll container inside the app shell", () => {
    expect(CAPABILITIES_CENTER_CLASS).toContain("h-full");
    expect(CAPABILITIES_CENTER_CLASS).toContain("overflow-y-auto");
  });
});
