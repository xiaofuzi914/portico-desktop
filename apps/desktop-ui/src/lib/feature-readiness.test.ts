import { describe, expect, it } from "vitest";
import { featureReadiness } from "./feature-readiness";

describe("feature readiness", () => {
  it("advertises the core workflow after the safe golden path closed", () => {
    expect(featureReadiness.coreAgentWorkflow.ready).toBe(true);
  });

  it("does not advertise native automation while its safety boundary is disabled", () => {
    expect(featureReadiness.nativeTools.ready).toBe(false);
    expect(featureReadiness.nativeTools.reason).toContain("disabled");
  });

  it("does not expose skill invocation as ready before the backend command exists", () => {
    expect(featureReadiness.skillInvocation.ready).toBe(false);
    expect(featureReadiness.skillInvocation.reason).toContain("not exposed");
  });

  it("exposes multi-agent as production secondary path (default remains single-agent)", () => {
    expect(featureReadiness.multiAgentOrchestration.ready).toBe(true);
    expect(featureReadiness.multiAgentOrchestration.reason.toLowerCase()).toContain("production");
    expect(featureReadiness.coreAgentWorkflow.ready).toBe(true);
  });

  it("marks context injection, indexer, and advanced file tools ready after closed-loop work", () => {
    expect(featureReadiness.contextInjection.ready).toBe(true);
    expect(featureReadiness.workspaceIndexer.ready).toBe(true);
    expect(featureReadiness.advancedFileTools.ready).toBe(true);
    expect(featureReadiness.automations.ready).toBe(false);
    expect(featureReadiness.terminal.ready).toBe(false);
  });
});
