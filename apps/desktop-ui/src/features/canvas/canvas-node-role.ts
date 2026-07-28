import type { CanvasNode, CanvasNodeKind } from "@/lib/schemas";

/**
 * Unified "role" shown and edited on a node. Maps to backend `kind` + payload
 * (e.g. narrative branch). Keeps the product model as one node with attributes
 * rather than one toolbar button per type.
 */
export type CanvasNodeRole =
  | "note"
  | "goal"
  | "stage"
  | "session"
  | "intent"
  | "progress"
  | "conclusion"
  | "insight";

/** Roles a user can assign when creating/editing (session root is extract-only). */
export const USER_EDITABLE_ROLES: CanvasNodeRole[] = [
  "note",
  "goal",
  "stage",
  "intent",
  "progress",
  "conclusion",
  "insight",
];

export function roleFromNode(node: CanvasNode): CanvasNodeRole {
  if (node.kind === "Note") return "note";
  if (node.kind === "Goal") return "goal";
  if (node.kind === "Stage") return "stage";
  if (node.kind === "ThreadCluster") return "session";
  // Insight — prefer narrative branch
  try {
    const p = JSON.parse(node.payload_json) as {
      narrative_role?: string;
      branch?: string;
    };
    if (p.narrative_role === "branch" || p.narrative_role === "leaf") {
      if (p.branch === "intent") return "intent";
      if (p.branch === "progress") return "progress";
      if (p.branch === "conclusion") return "conclusion";
    }
    if (p.branch === "intent") return "intent";
    if (p.branch === "progress") return "progress";
    if (p.branch === "conclusion") return "conclusion";
  } catch {
    /* ignore */
  }
  return "insight";
}

export function kindFromRole(role: CanvasNodeRole): CanvasNodeKind {
  switch (role) {
    case "note":
      return "Note";
    case "goal":
      return "Goal";
    case "stage":
      return "Stage";
    case "session":
      return "ThreadCluster";
    case "intent":
    case "progress":
    case "conclusion":
    case "insight":
      return "Insight";
  }
}

/** Merge role into kind + payload_json (preserves unrelated payload fields). */
export function applyRoleToNode(
  node: CanvasNode,
  role: CanvasNodeRole,
): Pick<CanvasNode, "kind" | "payload_json"> {
  const kind = kindFromRole(role);
  let payload: Record<string, unknown> = {};
  try {
    const parsed = JSON.parse(node.payload_json) as unknown;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      payload = { ...(parsed as Record<string, unknown>) };
    }
  } catch {
    payload = {};
  }

  // Clear narrative fields unless this is a narrative insight role.
  if (role === "intent" || role === "progress" || role === "conclusion") {
    payload.branch = role;
    payload.narrative_role = "leaf";
    payload.layer = "conversation";
  } else if (role === "insight") {
    delete payload.branch;
    payload.narrative_role = "leaf";
    payload.layer = "conversation";
  } else if (role === "session") {
    payload.narrative_role = "root";
    delete payload.branch;
  } else if (role === "goal" || role === "stage") {
    payload.layer = "goal";
    delete payload.branch;
    delete payload.narrative_role;
  } else {
    // note
    delete payload.branch;
    delete payload.narrative_role;
    delete payload.layer;
  }

  return {
    kind,
    payload_json: JSON.stringify(payload),
  };
}

/** Default title when creating a node of this role (i18n key suffix). */
export function defaultTitleKey(role: CanvasNodeRole): string {
  switch (role) {
    case "goal":
      return "canvas.defaultGoalTitle";
    case "stage":
      return "canvas.defaultStageTitle";
    case "intent":
      return "canvas.defaultIntentTitle";
    case "progress":
      return "canvas.defaultProgressTitle";
    case "conclusion":
      return "canvas.defaultConclusionTitle";
    case "insight":
      return "canvas.defaultInsightTitle";
    case "session":
      return "canvas.defaultSessionTitle";
    case "note":
    default:
      return "canvas.defaultNoteTitle";
  }
}

/** i18n key for role label. */
export function roleLabelKey(role: CanvasNodeRole): string {
  return `canvas.role.${role}`;
}
