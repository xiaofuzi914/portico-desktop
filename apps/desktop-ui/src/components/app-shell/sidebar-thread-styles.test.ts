import { describe, expect, it } from "vitest";
import { SIDEBAR_THREAD_ACTION_CLASS, SIDEBAR_THREAD_LINK_CLASS } from "./sidebar-thread-styles";

describe("sidebar thread density", () => {
  it("keeps thread actions and rows more compact than project rows", () => {
    expect(SIDEBAR_THREAD_ACTION_CLASS).toContain("h-7");
    expect(SIDEBAR_THREAD_ACTION_CLASS).toContain("text-xs");
    expect(SIDEBAR_THREAD_LINK_CLASS).toContain("h-7");
    expect(SIDEBAR_THREAD_LINK_CLASS).toContain("text-xs");
  });
});
