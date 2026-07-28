import type { CSSProperties } from "react";
import { MarkerType, Position, type Edge, type Node } from "@xyflow/react";
import type {
  CanvasEdge,
  CanvasNode,
  CanvasNodeKind,
  CanvasSnapshot,
} from "@/lib/schemas";

/** Approximate card size used when choosing left/right vs top/bottom handles. */
const NODE_W = 280;
const NODE_H = 104;
/** Session relationship cards show title + longer summary. */
const SESSION_H = 148;
/** Branch header cards are shorter (`h-[72px]`). */
const BRANCH_H = 72;

function nodeHeight(node: CanvasNode): number {
  if (node.kind === "ThreadCluster") return SESSION_H;
  try {
    const p = JSON.parse(node.payload_json) as { narrative_role?: string };
    if (p.narrative_role === "branch") return BRANCH_H;
  } catch {
    /* ignore */
  }
  return NODE_H;
}

/** Resolve linked thread id from a canvas node payload or links. */
export function threadIdFromNode(
  node: CanvasNode,
  links: { ref_type: string; ref_id: string }[] = [],
): string | null {
  try {
    const p = JSON.parse(node.payload_json) as { thread_id?: string };
    if (typeof p.thread_id === "string" && p.thread_id.length > 0) {
      return p.thread_id;
    }
  } catch {
    /* ignore */
  }
  const link = links.find((l) => l.ref_type === "Thread");
  return link?.ref_id ?? null;
}

export type CanvasFlowNodeData = {
  canvasNode: CanvasNode;
  label: string;
  summary: string;
  kind: CanvasNodeKind;
  status: CanvasNode["status"];
  /** When set, this card is the currently open session (thread mind-map). */
  highlighted?: boolean;
};

export type CanvasFlowNode = Node<CanvasFlowNodeData, "canvas">;

const KIND_STYLE: Record<
  CanvasNodeKind,
  { border: string; bg: string; badge: string }
> = {
  Insight: {
    border: "border-sky-500/40",
    bg: "bg-sky-500/10",
    badge: "text-sky-700 dark:text-sky-300",
  },
  Goal: {
    border: "border-violet-500/40",
    bg: "bg-violet-500/10",
    badge: "text-violet-700 dark:text-violet-300",
  },
  Stage: {
    border: "border-amber-500/40",
    bg: "bg-amber-500/10",
    badge: "text-amber-800 dark:text-amber-300",
  },
  ThreadCluster: {
    border: "border-emerald-500/40",
    bg: "bg-emerald-500/10",
    badge: "text-emerald-700 dark:text-emerald-300",
  },
  Note: {
    border: "border-border",
    bg: "bg-muted/60",
    badge: "text-muted-foreground",
  },
};

export function kindStyle(kind: CanvasNodeKind) {
  return KIND_STYLE[kind] ?? KIND_STYLE.Note;
}

export type NarrativeRole = "root" | "branch" | "leaf" | "unknown";

const BRANCH_LABEL: Record<string, string> = {
  intent: "意图",
  progress: "推进",
  conclusion: "结论",
};

const BRANCH_CARD: Record<string, { branchStyle: string; branchBadge: string }> = {
  intent: {
    branchStyle: "border-violet-500/45 bg-violet-500/10",
    branchBadge: "text-violet-700 dark:text-violet-300",
  },
  progress: {
    branchStyle: "border-amber-500/45 bg-amber-500/10",
    branchBadge: "text-amber-800 dark:text-amber-300",
  },
  conclusion: {
    branchStyle: "border-sky-500/45 bg-sky-500/10",
    branchBadge: "text-sky-700 dark:text-sky-300",
  },
};

/** Parse narrative payload written by session extract. */
export function narrativeMeta(node: CanvasNode): {
  role: NarrativeRole;
  branch: string | null;
  branchLabel: string;
  branchStyle: string;
  branchBadge: string;
} {
  try {
    const p = JSON.parse(node.payload_json) as {
      narrative_role?: string;
      branch?: string;
    };
    const role =
      p.narrative_role === "root" ||
      p.narrative_role === "branch" ||
      p.narrative_role === "leaf"
        ? p.narrative_role
        : node.kind === "ThreadCluster"
          ? "root"
          : "unknown";
    const branch = typeof p.branch === "string" ? p.branch : null;
    const style = (branch && BRANCH_CARD[branch]) || {
      branchStyle: "border-border bg-muted/50",
      branchBadge: "text-muted-foreground",
    };
    return {
      role,
      branch,
      branchLabel: (branch && BRANCH_LABEL[branch]) || "",
      branchStyle: style.branchStyle,
      branchBadge: style.branchBadge,
    };
  } catch {
    return {
      role: node.kind === "ThreadCluster" ? "root" : "unknown",
      branch: null,
      branchLabel: "",
      branchStyle: "border-border bg-muted/50",
      branchBadge: "text-muted-foreground",
    };
  }
}

export type CanvasLayerFilter = "all" | "conversation" | "goal";

function nodeLayer(node: CanvasNode): "conversation" | "goal" | "other" {
  if (node.kind === "ThreadCluster" || node.kind === "Insight") return "conversation";
  if (node.kind === "Goal" || node.kind === "Stage") return "goal";
  try {
    const p = JSON.parse(node.payload_json) as { layer?: string };
    if (p.layer === "conversation" || p.layer === "goal") return p.layer;
  } catch {
    /* ignore */
  }
  return "other";
}

/**
 * Empty stub cards left from exploratory “add node” / role defaults.
 * Product rule: session mind map starts with session cards only; Note/Stage/Goal
 * appear only after the user intentionally creates real content.
 */
const BLANK_PLACEHOLDER_TITLES = new Set([
  "新便签",
  "New note",
  "阶段",
  "Stage",
  "新节点",
  "New node",
  "项目目标",
  "Project goal",
  "目标",
  "Goal",
]);

export function isBlankPlaceholderNode(node: CanvasNode): boolean {
  if (node.kind !== "Note" && node.kind !== "Stage" && node.kind !== "Goal") {
    return false;
  }
  // Auto-extracted session trees are never placeholders.
  if (node.source === "Auto") return false;
  const title = node.title.trim();
  if (!BLANK_PLACEHOLDER_TITLES.has(title)) return false;
  if (node.summary.trim().length > 0) return false;
  // Unstarted empty cards only.
  return node.status === "Todo";
}

/**
 * Pick source/target handle ids from relative node centers so edges get a clear
 * direction (down / up / left / right) instead of always top↔bottom.
 *
 * Parent (结构) edges are vertical-first: the narrative / forest layouts stack
 * children below parents, so a sideways exit only makes sense when the two
 * nodes sit on the same row (e.g. after a manual drag).
 *
 * `fromH` / `toH` are card heights so branch (72px) vs full (104px) centers are
 * correct — wrong centers flip dy and reverse arrow direction.
 */
export function resolveEdgeHandles(
  from: { x: number; y: number },
  to: { x: number; y: number },
  kind: CanvasEdge["kind"],
  fromH: number = NODE_H,
  toH: number = NODE_H,
): { sourceHandle: string; targetHandle: string } {
  const fromCx = from.x + NODE_W / 2;
  const fromCy = from.y + fromH / 2;
  const toCx = to.x + NODE_W / 2;
  const toCy = to.y + toH / 2;
  const dx = toCx - fromCx;
  const dy = toCy - fromCy;
  const absX = Math.abs(dx);
  const absY = Math.abs(dy);

  if (kind === "Parent") {
    // Always top/bottom for structure — never side handles. Side attachment made
    // root→意图 / root→结论 arrows point left/right along the fan arms.
    // (StructureEdge also forces Bottom→Top path geometry for the same reason.)
    if (dy >= 0) {
      return { sourceHandle: "out-bottom", targetHandle: "in-top" };
    }
    return { sourceHandle: "out-top", targetHandle: "in-bottom" };
  }

  // Related (支撑): goals sit to the right of conversation, so prefer a clean
  // side exit into the goal spine instead of long diagonal smoothsteps.
  if (kind === "Related") {
    if (absX >= absY * 0.55) {
      return dx >= 0
        ? { sourceHandle: "out-right", targetHandle: "in-left" }
        : { sourceHandle: "out-left", targetHandle: "in-right" };
    }
    return dy >= 0
      ? { sourceHandle: "out-bottom", targetHandle: "in-top" }
      : { sourceHandle: "out-top", targetHandle: "in-bottom" };
  }

  const preferVertical = absY >= absX * 0.55;
  if (preferVertical) {
    return dy >= 0
      ? { sourceHandle: "out-bottom", targetHandle: "in-top" }
      : { sourceHandle: "out-top", targetHandle: "in-bottom" };
  }
  return dx >= 0
    ? { sourceHandle: "out-right", targetHandle: "in-left" }
    : { sourceHandle: "out-left", targetHandle: "in-right" };
}

function edgeVisual(kind: CanvasEdge["kind"]): {
  animated: boolean;
  style: CSSProperties;
  zIndex: number;
  markerColor: string;
} {
  switch (kind) {
    case "Related":
      return {
        // Soft links stay quiet: no animation, lighter stroke, sit under structure.
        animated: false,
        style: {
          strokeDasharray: "6 5",
          stroke: "var(--color-muted-foreground, #94a3b8)",
          strokeWidth: 1.1,
          opacity: 0.55,
        },
        zIndex: 0,
        markerColor: "var(--color-muted-foreground, #94a3b8)",
      };
    case "DerivedFrom":
      return {
        animated: true,
        style: {
          stroke: "var(--color-sky-600, #0284c7)",
          strokeWidth: 1.5,
        },
        zIndex: 1,
        markerColor: "var(--color-sky-600, #0284c7)",
      };
    case "Blocks":
      return {
        animated: false,
        style: {
          stroke: "var(--color-destructive, #dc2626)",
          strokeWidth: 1.5,
        },
        zIndex: 2,
        markerColor: "var(--color-destructive, #dc2626)",
      };
    case "Parent":
    default:
      return {
        animated: false,
        style: {
          stroke: "var(--color-slate-400, #94a3b8)",
          strokeWidth: 1.5,
          opacity: 0.9,
        },
        zIndex: 2,
        markerColor: "var(--color-slate-400, #94a3b8)",
      };
  }
}

/** Project snapshot → React Flow graph, optionally filtered by layer. */
export function snapshotToFlow(
  snapshot: CanvasSnapshot,
  layer: CanvasLayerFilter = "all",
  focusThreadId?: string | null,
  /** When true (session 脑图), drop empty Note/Stage/Goal stubs. */
  hideBlankPlaceholders = false,
): {
  nodes: CanvasFlowNode[];
  edges: Edge[];
} {
  const visible = snapshot.nodes.filter((node) => {
    if (hideBlankPlaceholders && isBlankPlaceholderNode(node)) return false;
    if (layer === "all") return true;
    const l = nodeLayer(node);
    // Conversation = session relationship tree only (not notes / stages).
    if (layer === "conversation") return l === "conversation";
    // Goal = goal/stage spine only.
    if (layer === "goal") return l === "goal";
    return true;
  });
  const visibleIds = new Set(visible.map((n) => n.id));
  const nodeById = new Map(visible.map((n) => [n.id, n] as const));
  const posById = new Map(
    visible.map((n) => [n.id, { x: n.position_x, y: n.position_y }] as const),
  );

  const nodes: CanvasFlowNode[] = visible.map((node) => {
    const linkedThread = threadIdFromNode(node);
    return {
      id: node.id,
      type: "canvas" as const,
      position: { x: node.position_x, y: node.position_y },
      // Default connection side when a handle id is missing — top-down structure.
      sourcePosition: Position.Bottom,
      targetPosition: Position.Top,
      data: {
        canvasNode: node,
        label: node.title,
        summary: node.summary,
        kind: node.kind,
        status: node.status,
        highlighted: Boolean(
          focusThreadId && linkedThread && linkedThread === focusThreadId,
        ),
      },
    };
  });

  const edges: Edge[] = snapshot.edges
    .filter((edge) => visibleIds.has(edge.from_id) && visibleIds.has(edge.to_id))
    .map((edge) => {
      const fromNode = nodeById.get(edge.from_id);
      const toNode = nodeById.get(edge.to_id);
      const from = posById.get(edge.from_id) ?? { x: 0, y: 0 };
      const to = posById.get(edge.to_id) ?? { x: 0, y: 0 };
      const fromH = fromNode ? nodeHeight(fromNode) : NODE_H;
      const toH = toNode ? nodeHeight(toNode) : NODE_H;
      const handles = resolveEdgeHandles(from, to, edge.kind, fromH, toH);
      const visual = edgeVisual(edge.kind);
      const isParent = edge.kind === "Parent";
      const isRelated = edge.kind === "Related";
      return {
        id: edge.id,
        source: edge.from_id,
        target: edge.to_id,
        // Parent always anchors bottom→top so StructureEdge can route downward.
        sourceHandle: isParent ? "out-bottom" : handles.sourceHandle,
        targetHandle: isParent
          ? to.y + toH / 2 >= from.y + fromH / 2
            ? "in-top"
            : "in-bottom"
          : handles.targetHandle,
        label: undefined,
        // Custom structure edge forces Bottom→Top geometry (fixes sideways arrows
        // on root fan-out to 意图 / 结论). Other kinds keep smoothstep.
        type: isParent ? "structure" : "smoothstep",
        pathOptions: isParent
          ? undefined
          : {
              borderRadius: 16,
              offset: isRelated ? 12 : 0,
            },
        animated: visual.animated,
        style: visual.style,
        zIndex: visual.zIndex,
        markerEnd: {
          type: MarkerType.ArrowClosed,
          width: isRelated ? 12 : 16,
          height: isRelated ? 12 : 16,
          color: visual.markerColor,
        },
        markerStart: undefined,
      };
    });

  return { nodes, edges };
}

export function parseViewport(viewportJson: string): {
  x: number;
  y: number;
  zoom: number;
} {
  try {
    const raw = JSON.parse(viewportJson) as { x?: number; y?: number; zoom?: number };
    return {
      x: typeof raw.x === "number" ? raw.x : 0,
      y: typeof raw.y === "number" ? raw.y : 0,
      zoom: typeof raw.zoom === "number" && raw.zoom > 0 ? raw.zoom : 1,
    };
  } catch {
    return { x: 0, y: 0, zoom: 1 };
  }
}

export function serializeViewport(vp: { x: number; y: number; zoom: number }): string {
  return JSON.stringify({ x: vp.x, y: vp.y, zoom: vp.zoom });
}

/** Layout helper for a new node so it does not stack on the origin forever. */
export function nextNodePosition(existing: CanvasNode[], kind: CanvasNodeKind): {
  x: number;
  y: number;
  rank: number;
} {
  const sameKind = existing.filter((n) => n.kind === kind);
  const rank = existing.length;
  const col = kind === "Goal" || kind === "Stage" ? 1 : 0;
  const row = sameKind.length;
  return {
    x: 80 + col * 320,
    y: 80 + row * 140,
    rank,
  };
}

export function linksForNode(snapshot: CanvasSnapshot, nodeId: string) {
  return snapshot.links.filter((link) => link.node_id === nodeId);
}

/** Product-facing edge kind: solid structure vs dashed support. */
export function edgeKindLabel(edge: CanvasEdge): string {
  switch (edge.kind) {
    case "Parent":
      return "structure";
    case "Related":
      return "support";
    case "DerivedFrom":
      return "derived";
    case "Blocks":
      return "blocks";
    default:
      return edge.kind;
  }
}

/** True when the edge is drawn dashed (Related soft links). */
export function isDashedEdge(edge: CanvasEdge): boolean {
  return edge.kind === "Related";
}

/** True when the edge path is animated. */
export function isAnimatedEdge(edge: CanvasEdge): boolean {
  return edge.kind === "DerivedFrom";
}
