export const featureReadiness = {
  skillInvocation: {
    ready: false,
    reason:
      "Skill invocation is not exposed by the Tauri backend yet. Skill listing is available.",
  },
  coreAgentWorkflow: {
    ready: true,
    reason: "Projects, threads, runs, messages, and run events are bound to Tauri commands.",
  },
  nativeTools: {
    ready: true,
    reason: "Browser and desktop controls are bound to Tauri commands and permission checks.",
  },
} as const;
