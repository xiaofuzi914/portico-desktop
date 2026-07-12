import { describe, expect, it } from "vitest";
import {
  auditKeys,
  automationKeys,
  backgroundTaskKeys,
  browserWindowKeys,
  desktopKeys,
  gitKeys,
  migrationKeys,
  modelKeys,
  mcpKeys,
  notificationKeys,
  pluginKeys,
  providerKeys,
  runKeys,
  skillKeys,
  terminalKeys,
  usageKeys,
  workspaceKeys,
} from "./query-keys";
import type { AgentRunId, ProviderId, TerminalId, ThreadId, WorkspaceId } from "./schemas";

const workspaceId = "ws-1" as WorkspaceId;
const threadId = "th-1" as ThreadId;
const runId = "run-1" as AgentRunId;
const terminalId = "term-1" as TerminalId;
const providerId = "prov-1" as ProviderId;

describe("query key factories", () => {
  it("workspace keys", () => {
    expect(workspaceKeys.list()).toEqual(["workspaces"]);
    expect(workspaceKeys.threads(workspaceId)).toEqual(["workspaces", workspaceId, "threads"]);
    expect(workspaceKeys.worktrees(workspaceId)).toEqual(["workspaces", workspaceId, "worktrees"]);
    expect(workspaceKeys.memories(workspaceId)).toEqual(["workspaces", workspaceId, "memories"]);
  });

  it("git keys use a single shape", () => {
    expect(gitKeys.status(workspaceId, "repo")).toEqual([
      "workspaces",
      workspaceId,
      "git",
      "status",
      "repo",
    ]);
    expect(gitKeys.diff(workspaceId, "repo")).toEqual([
      "workspaces",
      workspaceId,
      "git",
      "diff",
      "repo",
    ]);
    expect(gitKeys.status(workspaceId)).toEqual(["workspaces", workspaceId, "git", "status", ""]);
  });

  it("run keys include thread id and normalize missing run id", () => {
    expect(runKeys.events(workspaceId, threadId, runId)).toEqual([
      "workspaces",
      workspaceId,
      "threads",
      threadId,
      "runs",
      runId,
      "events",
    ]);
    expect(runKeys.events(workspaceId, threadId)).toEqual([
      "workspaces",
      workspaceId,
      "threads",
      threadId,
      "runs",
      "none",
      "events",
    ]);
    expect(runKeys.context(workspaceId, threadId, runId, "query")).toEqual([
      "workspaces",
      workspaceId,
      "threads",
      threadId,
      "runs",
      runId,
      "context",
      "query",
    ]);
  });

  it("terminal keys", () => {
    expect(terminalKeys.history(terminalId)).toEqual(["terminals", terminalId, "history"]);
    expect(terminalKeys.history(undefined)).toEqual(["terminals", undefined, "history"]);
  });

  it("browser window and desktop keys", () => {
    expect(browserWindowKeys.list()).toEqual(["browser-windows"]);
    expect(desktopKeys.capture(workspaceId)).toEqual(["desktop-capture", workspaceId]);
  });

  it("notification and background task keys", () => {
    expect(notificationKeys.list(null, true)).toEqual(["notifications", null, true]);
    expect(notificationKeys.list(workspaceId, false)).toEqual([
      "notifications",
      workspaceId,
      false,
    ]);
    expect(backgroundTaskKeys.list(workspaceId)).toEqual(["background-tasks", workspaceId]);
  });

  it("automation keys", () => {
    expect(automationKeys.list(workspaceId)).toEqual(["automations", workspaceId]);
    expect(automationKeys.list(null)).toEqual(["automations", null]);
  });

  it("capability keys", () => {
    expect(pluginKeys.list()).toEqual(["plugins"]);
    expect(mcpKeys.list()).toEqual(["mcp-servers"]);
    expect(skillKeys.list()).toEqual(["skills"]);
    expect(providerKeys.list()).toEqual(["providers"]);
    expect(modelKeys.list(providerId)).toEqual(["models", providerId]);
    expect(modelKeys.list(null)).toEqual(["models"]);
    expect(modelKeys.list()).toEqual(["models"]);
    expect(usageKeys.summary()).toEqual(["usage-summary"]);
  });

  it("migration keys", () => {
    expect(migrationKeys.list()).toEqual(["migrations"]);
  });

  it("audit keys", () => {
    expect(auditKeys.log(workspaceId, threadId, runId)).toEqual([
      "audit-log",
      workspaceId,
      threadId,
      runId,
    ]);
    expect(auditKeys.log(null, null, null)).toEqual(["audit-log", null, null, null]);
  });
});
