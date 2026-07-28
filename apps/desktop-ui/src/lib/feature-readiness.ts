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
      "Primary path: durable conversation, model tool loop (fs_*, git, shell_exec, web_*, MCP when connected), approvals, context injection, and recovery.",
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
  projectCanvas: {
    ready: true,
    reason:
      "Session relationship mind map: one card per session with ≤300-char summary, parent→branch edges, click to open chat, branch-from-context.",
  },
  terminal: {
    ready: true,
    reason:
      "shell_exec is available to the agent after user approval; full interactive PTY UI remains optional.",
  },
  gitMutation: {
    ready: true,
    reason: "git add/commit available via the git tool with user approval; force-push stays blocked.",
  },
  automations: {
    ready: false,
    reason: "Automations are not productized; scheduler ticker is stopped.",
  },
  mcpAgentTools: {
    ready: true,
    reason:
      "After connecting MCP servers and refreshing tools in 能力中心, tools are registered into the agent loop (writes require approval).",
  },
} as const;
