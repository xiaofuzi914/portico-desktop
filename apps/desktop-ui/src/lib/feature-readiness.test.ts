import { describe, expect, it } from "vitest";
import { featureReadiness } from "./feature-readiness";

describe("feature readiness", () => {
  it("marks core agent workflows as ready for manual testing", () => {
    expect(featureReadiness.coreAgentWorkflow.ready).toBe(true);
  });

  it("does not expose skill invocation as ready before the backend command exists", () => {
    expect(featureReadiness.skillInvocation.ready).toBe(false);
    expect(featureReadiness.skillInvocation.reason).toContain("not exposed");
  });
});
