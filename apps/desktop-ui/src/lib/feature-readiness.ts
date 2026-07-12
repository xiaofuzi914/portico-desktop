/**
 * Product surface readiness flags.
 *
 * Prefer `getFeatureCapabilities()` from the backend for authoritative state.
 * These static flags mirror the current closed-loop product surface so the UI
 * can render before the first probe resolves, and for unit tests.
 *
 * Default path: single agent + tools (Codex / Claude-style).
 * Multi-agent orchestration is a production secondary path for real tasks.
 */
export const featureReadiness = {
  skillInvocation: {
    ready: false,
    reason: "Skill invocation is not exposed by the Tauri backend yet. Skill listing is available.",
  },
  coreAgentWorkflow: {
    ready: true,
    reason:
      "Primary path: durable conversation, model tool loop (list/read/search/edit/write, git read), approvals, context injection, and recovery.",
  },
  contextInjection: {
    ready: true,
    reason: "Instructions, non-sensitive memory, and RAG excerpts are assembled into the agent prompt.",
  },
  workspaceIndexer: {
    ready: true,
    reason: "Rebuild index scans the project tree from disk into the RAG store.",
  },
  advancedFileTools: {
    ready: true,
    reason: "fs_search and fs_edit are on the durable safe-tool allowlist.",
  },
  nativeTools: {
    ready: false,
    reason:
      "Native automation is disabled until workspace isolation, approval continuation, and platform permission checks are complete.",
  },
  multiAgentOrchestration: {
    ready: true,
    reason:
      "Production multi-role collaboration with durable orchestration sessions (composer secondary action; default send is single-agent).",
  },
  terminal: {
    ready: false,
    reason: "Terminal remains closed until PTY isolation and reconciliation land.",
  },
  gitMutation: {
    ready: false,
    reason: "Git write operations remain closed; only status/diff are enabled.",
  },
  automations: {
    ready: false,
    reason: "Automations are not productized; scheduler ticker is stopped.",
  },
  mcpAgentTools: {
    ready: false,
    reason: "MCP servers can be listed; tools are not attached to the agent loop yet.",
  },
} as const;
