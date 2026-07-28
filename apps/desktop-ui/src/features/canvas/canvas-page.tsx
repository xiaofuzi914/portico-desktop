import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  useEdgesState,
  useNodesState,
  useReactFlow,
  type Node,
  type OnNodeDrag,
  type Viewport,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  ArrowLeft,
  GitBranch,
  Goal,
  ListTree,
  Loader2,
  MessageSquare,
  Pencil,
  Plus,
  Sparkles,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ErrorAlert } from "@/components/ui/error-alert";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/lib/i18n-react";
import {
  branchThreadFromContext,
  createThread,
  decomposeCanvasGoal,
  deleteCanvasNode,
  extractCanvasInsights,
  getCanvasSnapshot,
  getOrCreateProjectCanvas,
  listRuns,
  markCanvasStageLaunched,
  reconcileCanvasStagesFromRun,
  sendMessage,
  setCanvasNodeStatus,
  startOrchestration,
  updateCanvasViewport,
  upsertCanvasNode,
} from "@/lib/tauri-api";
import {
  asCanvasId,
  asCanvasNodeId,
  asThreadId,
  type CanvasNode,
  type CanvasNodeKind,
  type ThreadId,
  type WorkspaceId,
} from "@/lib/schemas";
import { workspaceKeys } from "@/lib/query-keys";
import {
  buildNodeChatPrompt,
  CanvasNodeInspector,
  parseStagePayload,
} from "./canvas-node-inspector";
import { StructureEdge } from "./edges/structure-edge";
import { CanvasFlowNodeView } from "./nodes/canvas-node";
import {
  linksForNode,
  nextNodePosition,
  parseViewport,
  serializeViewport,
  isBlankPlaceholderNode,
  snapshotToFlow,
  threadIdFromNode,
  type CanvasFlowNode,
  type CanvasLayerFilter,
} from "./canvas-view-model";

const nodeTypes = { canvas: CanvasFlowNodeView };
const edgeTypes = { structure: StructureEdge };

/** How long a successful auto-sync stays "fresh" across tab remounts. */
const AUTO_SYNC_COOLDOWN_MS = 90_000;
/** Hard cap so a stuck Tauri IPC never leaves the map spinning forever. */
const EXTRACT_TIMEOUT_MS = 45_000;

type NodeContextMenuState = Readonly<{
  nodeId: string;
  title: string;
  x: number;
  y: number;
  /** Session cards can jump back to chat. */
  threadId: string | null;
}>;

/**
 * Survives React remounts when switching 对话/脑图 tabs (component unmounts).
 * Without this, every open re-triggers extract and shows "正在更新会话脑图…".
 */
const autoSyncedAtByCanvas = new Map<string, number>();
const inflightAutoSync = new Set<string>();

function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      reject(new Error(`${label} timed out after ${Math.round(ms / 1000)}s`));
    }, ms);
    promise.then(
      (value) => {
        window.clearTimeout(timer);
        resolve(value);
      },
      (err: unknown) => {
        window.clearTimeout(timer);
        reject(err);
      },
    );
  });
}

interface CanvasPageProps {
  workspaceId: WorkspaceId;
  workspaceName?: string;
  /** When set, open thread-scoped canvas instead of project default. */
  threadId?: string;
}

function CanvasWorkspace({ workspaceId, workspaceName, threadId }: CanvasPageProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { fitView } = useReactFlow();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  /** Detail dialog — opened by double-click so the map stays full-width. */
  const [detailOpen, setDetailOpen] = useState(false);
  const [goalDraft, setGoalDraft] = useState("");
  /** Session mind map defaults to conversation (sessions only); project may use all. */
  const [layer, setLayer] = useState<CanvasLayerFilter>(
    threadId ? "conversation" : "all",
  );
  /** Right-click menu on a node (delete / edit / open). */
  const [nodeMenu, setNodeMenu] = useState<NodeContextMenuState | null>(null);
  const nodeMenuRef = useRef<HTMLDivElement | null>(null);
  const viewportTimer = useRef<number | null>(null);
  const shouldFitRef = useRef(false);
  /** Per-mount guard (Strict Mode); module map handles cross-remount. */
  const autoSyncKeyRef = useRef<string | null>(null);

  // Always use the project canvas so the mind map is a session-relationship
  // overview (parent → branch), not a per-thread narrative dump.
  const canvasQuery = useQuery({
    queryKey: ["project-canvas", workspaceId],
    queryFn: () => getOrCreateProjectCanvas(workspaceId),
  });

  const canvasId = canvasQuery.data?.id;
  const snapshotQuery = useQuery({
    queryKey: ["canvas-snapshot", canvasId],
    queryFn: () => getCanvasSnapshot(canvasId!),
    enabled: Boolean(canvasId),
  });

  const snapshot = snapshotQuery.data;
  const initial = useMemo(
    () =>
      snapshot
        ? snapshotToFlow(
            snapshot,
            layer,
            threadId ?? null,
            /* hide empty stage/note stubs on the session map */
            Boolean(threadId),
          )
        : { nodes: [], edges: [] },
    [snapshot, layer, threadId],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState<CanvasFlowNode>(initial.nodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initial.edges);

  useEffect(() => {
    setNodes(initial.nodes);
    setEdges(initial.edges);
    if (shouldFitRef.current && initial.nodes.length > 0) {
      shouldFitRef.current = false;
      requestAnimationFrame(() => {
        void fitView({ padding: 0.18, duration: 280 });
      });
    }
  }, [fitView, initial.edges, initial.nodes, setEdges, setNodes]);

  const defaultViewport = useMemo(
    () => parseViewport(snapshot?.canvas.viewport_json ?? canvasQuery.data?.viewport_json ?? "{}"),
    [canvasQuery.data?.viewport_json, snapshot?.canvas.viewport_json],
  );

  const invalidate = useCallback(async () => {
    if (canvasId) {
      await queryClient.invalidateQueries({ queryKey: ["canvas-snapshot", canvasId] });
    }
  }, [canvasId, queryClient]);

  // One-shot: delete empty placeholder Stage/Note/Goal left from exploratory adds
  // so reopening 脑图 does not show junk next to the session tree.
  const prunedCanvasRef = useRef<string | null>(null);
  useEffect(() => {
    if (!snapshot || !canvasId || !threadId) return;
    if (prunedCanvasRef.current === canvasId) return;
    const blanks = snapshot.nodes.filter(isBlankPlaceholderNode);
    if (blanks.length === 0) {
      prunedCanvasRef.current = canvasId;
      return;
    }
    prunedCanvasRef.current = canvasId;
    void (async () => {
      for (const n of blanks) {
        try {
          await deleteCanvasNode(asCanvasNodeId(n.id));
        } catch {
          /* best-effort */
        }
      }
      await invalidate();
    })();
  }, [canvasId, invalidate, snapshot, threadId]);

  // When reopening the canvas, sync any InProgress stages from their last launched run.
  useEffect(() => {
    if (!snapshot) return;
    const stages = snapshot.nodes.filter(
      (n) =>
        (n.kind === "Stage" || n.kind === "Goal") &&
        n.status === "InProgress",
    );
    if (stages.length === 0) return;
    let cancelled = false;
    void (async () => {
      for (const stage of stages) {
        if (cancelled) return;
        const p = parseStagePayload(stage.payload_json);
        const tid = p.last_thread_id;
        const rid = p.last_run_id;
        if (!tid) continue;
        try {
          const runs = await listRuns(tid as never);
          const run = rid
            ? runs.find((r) => r.id === rid)
            : runs[0];
          if (!run) continue;
          if (run.status === "Completed" || run.status === "Failed") {
            await reconcileCanvasStagesFromRun(
              workspaceId,
              tid as never,
              run.id,
              run.status === "Completed" ? "done" : "blocked",
            );
          }
        } catch {
          /* best-effort */
        }
      }
      if (!cancelled) await invalidate();
    })();
    return () => {
      cancelled = true;
    };
  }, [invalidate, snapshot?.canvas.revision, workspaceId]);

  /** Single entry: add a generic node; role/type is edited on the node dialog. */
  const addNode = useMutation({
    mutationFn: async () => {
      if (!canvasId || !snapshot) throw new Error("Canvas not ready");
      const now = new Date().toISOString();
      const kind: CanvasNodeKind = "Note";
      const pos = nextNodePosition(snapshot.nodes, kind);
      const node: CanvasNode = {
        id: asCanvasNodeId(crypto.randomUUID()),
        canvas_id: canvasId,
        kind,
        title: t("canvas.defaultNodeTitle"),
        summary: "",
        status: "Todo",
        parent_id: null,
        position_x: pos.x,
        position_y: pos.y,
        layout_rank: pos.rank,
        source: "User",
        payload_json: "{}",
        created_at: now,
        updated_at: now,
      };
      await upsertCanvasNode(node);
      return node.id;
    },
    onSuccess: async (id) => {
      // Manual nodes live outside the session-tree layer — show full graph.
      setLayer("all");
      // Avoid pruning the node we just created before the user edits it.
      prunedCanvasRef.current = canvasId ?? prunedCanvasRef.current;
      setSelectedId(id);
      setDetailOpen(true);
      await invalidate();
    },
  });

  const removeNode = useMutation({
    mutationFn: (nodeId: string) => deleteCanvasNode(asCanvasNodeId(nodeId)),
    onSuccess: async () => {
      setSelectedId(null);
      setDetailOpen(false);
      setNodeMenu(null);
      await invalidate();
    },
  });

  const closeNodeMenu = useCallback(() => setNodeMenu(null), []);

  const openNodeContextMenu = useCallback(
    (event: React.MouseEvent | MouseEvent, node: CanvasFlowNode) => {
      event.preventDefault();
      event.stopPropagation();
      const canvasNode = node.data.canvasNode;
      const links = snapshot ? linksForNode(snapshot, node.id) : [];
      const tid =
        canvasNode.kind === "ThreadCluster"
          ? threadIdFromNode(canvasNode, links)
          : null;
      const pad = 8;
      const menuW = 176;
      // Session cards: edit + open + branch + delete
      const menuH = tid ? 140 : 80;
      const x = Math.min(event.clientX, window.innerWidth - menuW - pad);
      const y = Math.min(event.clientY, window.innerHeight - menuH - pad);
      setSelectedId(node.id);
      setNodeMenu({
        nodeId: node.id,
        title: canvasNode.title || node.data.label,
        x: Math.max(pad, x),
        y: Math.max(pad, y),
        threadId: tid,
      });
    },
    [snapshot],
  );

  useEffect(() => {
    if (!nodeMenu) return;
    const onPointerDown = (event: MouseEvent) => {
      const el = nodeMenuRef.current;
      if (
        el &&
        event.target instanceof Element &&
        el.contains(event.target)
      ) {
        return;
      }
      setNodeMenu(null);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setNodeMenu(null);
    };
    // Defer so the opening right-click does not immediately close the menu.
    const timer = window.setTimeout(() => {
      document.addEventListener("mousedown", onPointerDown);
      document.addEventListener("keydown", onKeyDown);
    }, 0);
    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [nodeMenu]);

  const extractInsights = useMutation({
    mutationFn: async () => {
      if (!canvasId) throw new Error("Canvas not ready");
      // Backend returns the rebuilt snapshot — apply it immediately so the UI
      // never depends only on a follow-up invalidate (felt like "no reaction").
      return withTimeout(
        extractCanvasInsights(canvasId),
        EXTRACT_TIMEOUT_MS,
        "extract_canvas_insights",
      );
    },
    onSuccess: async (nextSnapshot) => {
      shouldFitRef.current = true;
      if (canvasId) {
        autoSyncedAtByCanvas.set(canvasId, Date.now());
        queryClient.setQueryData(["canvas-snapshot", canvasId], nextSnapshot);
      }
      await invalidate();
      // Refresh canvas meta (last_extracted_at) for thread canvases.
      if (threadId) {
        await queryClient.invalidateQueries({
          queryKey: ["thread-canvas", workspaceId, threadId],
        });
      }
    },
  });

  const runExtract = useCallback(() => {
    if (!canvasId || extractInsights.isPending) return;
    extractInsights.reset();
    shouldFitRef.current = true;
    // Manual refresh always allowed; clear cooldown so auto-sync can retry later.
    autoSyncedAtByCanvas.delete(canvasId);
    inflightAutoSync.delete(canvasId);
    extractInsights.mutate();
  }, [canvasId, extractInsights]);

  // Mind map: rebuild session cards when the map first needs them.
  // Skip when already fresh this session (tab switch remounts this component).
  useEffect(() => {
    if (!canvasId || !snapshot) return;
    const syncKey = `${canvasId}:sessions`;
    if (autoSyncKeyRef.current === syncKey) return;

    const hasSessionCards = snapshot.nodes.some((n) => n.kind === "ThreadCluster");
    const lastSynced = autoSyncedAtByCanvas.get(canvasId) ?? 0;
    const cool = Date.now() - lastSynced < AUTO_SYNC_COOLDOWN_MS;
    const extractedAt = snapshot.canvas.last_extracted_at
      ? Date.parse(snapshot.canvas.last_extracted_at)
      : NaN;
    const recentlyExtracted =
      Number.isFinite(extractedAt) && Date.now() - extractedAt < AUTO_SYNC_COOLDOWN_MS;

    // Already have a usable map and we refreshed recently — don't block the UI.
    if (hasSessionCards && (cool || recentlyExtracted || inflightAutoSync.has(canvasId))) {
      autoSyncKeyRef.current = syncKey;
      return;
    }
    // Empty map still needs a first build; claim keys before mutate (Strict Mode).
    if (inflightAutoSync.has(canvasId)) {
      autoSyncKeyRef.current = syncKey;
      return;
    }

    autoSyncKeyRef.current = syncKey;
    inflightAutoSync.add(canvasId);
    shouldFitRef.current = true;
    void extractInsights
      .mutateAsync()
      .then((next) => {
        autoSyncedAtByCanvas.set(canvasId, Date.now());
        queryClient.setQueryData(["canvas-snapshot", canvasId], next);
      })
      .catch(() => {
        // Allow retry on next open / manual refresh.
        if (autoSyncKeyRef.current === syncKey) {
          autoSyncKeyRef.current = null;
        }
        autoSyncedAtByCanvas.delete(canvasId);
      })
      .finally(() => {
        inflightAutoSync.delete(canvasId);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mount/open sync
  }, [canvasId, snapshot?.canvas.id, snapshot?.canvas.last_extracted_at]);

  const openSession = useCallback(
    (targetThreadId: string) => {
      void navigate({
        to: "/workspaces/$workspaceId/threads/$threadId",
        params: { workspaceId, threadId: targetThreadId },
        search: { runId: undefined, view: "chat" },
      });
    },
    [navigate, workspaceId],
  );

  /** Right-click a session card → branch (optional focus = card title). */
  const branchFromSession = useMutation({
    mutationFn: async (args: { parentId: string; focusText?: string | null }) => {
      const child = await branchThreadFromContext(
        workspaceId,
        asThreadId(args.parentId),
        null,
        args.focusText ?? null,
      );
      // Refresh mind map so the new edge appears immediately.
      if (canvasId) {
        const next = await extractCanvasInsights(asCanvasId(canvasId));
        queryClient.setQueryData(["canvas-snapshot", canvasId], next);
      }
      return child;
    },
    onSuccess: async (child) => {
      await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      openSession(child.id);
    },
  });

  const decompose = useMutation({
    mutationFn: async (args: { goalText: string; parentNodeId?: string | null }) => {
      if (!canvasId) throw new Error("Canvas not ready");
      return decomposeCanvasGoal(
        canvasId,
        args.goalText,
        args.parentNodeId ? asCanvasNodeId(args.parentNodeId) : null,
      );
    },
    onSuccess: async () => {
      setGoalDraft("");
      shouldFitRef.current = true;
      setLayer("all");
      await invalidate();
    },
  });

  const saveNode = useMutation({
    mutationFn: async (node: CanvasNode) => {
      await upsertCanvasNode(node);
    },
    onSuccess: async () => {
      await invalidate();
    },
  });

  const launchStage = useMutation({
    mutationFn: async (args: {
      node: CanvasNode;
      mode: "single" | "multi-role";
      target: "new-thread" | "current-thread";
    }) => {
      // Persist any inspector edits before launch bookkeeping.
      await upsertCanvasNode(args.node);
      const prompt = buildNodeChatPrompt(args.node);
      let targetThreadId: ThreadId;
      if (args.target === "current-thread" && threadId) {
        targetThreadId = threadId as ThreadId;
      } else {
        const thread = await createThread(
          workspaceId,
          `${args.node.title}`.slice(0, 80) || t("thread.defaultTitle"),
        );
        targetThreadId = thread.id;
      }
      if (args.mode === "multi-role") {
        const session = await startOrchestration(workspaceId, targetThreadId, prompt);
        await markCanvasStageLaunched(
          asCanvasNodeId(args.node.id),
          targetThreadId,
          session.parent_run_id,
        );
        return { threadId: targetThreadId, runId: session.parent_run_id };
      }
      const run = await sendMessage(targetThreadId, prompt, crypto.randomUUID());
      await markCanvasStageLaunched(
        asCanvasNodeId(args.node.id),
        targetThreadId,
        run.id,
      );
      return { threadId: targetThreadId, runId: run.id };
    },
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      await invalidate();
      void navigate({
        to: "/workspaces/$workspaceId/threads/$threadId",
        params: { workspaceId, threadId: result.threadId },
        search: { runId: result.runId, view: "chat" },
      });
    },
  });

  /** Note / Goal / Insight → send content into current or new session. */
  const sendToChat = useMutation({
    mutationFn: async (args: {
      node: CanvasNode;
      target: "new-thread" | "current-thread";
    }) => {
      const prompt = buildNodeChatPrompt(args.node);
      let targetThreadId: ThreadId;
      if (args.target === "current-thread" && threadId) {
        targetThreadId = threadId as ThreadId;
      } else {
        const thread = await createThread(
          workspaceId,
          `${args.node.title}`.slice(0, 80) || t("thread.defaultTitle"),
        );
        targetThreadId = thread.id;
      }
      const run = await sendMessage(targetThreadId, prompt, crypto.randomUUID());
      const payload = parseStagePayload(args.node.payload_json);
      const next: CanvasNode = {
        ...args.node,
        status: args.node.status === "Done" ? "Done" : "InProgress",
        source: args.node.source === "Auto" ? "User" : args.node.source,
        payload_json: JSON.stringify({
          ...payload,
          last_thread_id: targetThreadId,
          last_run_id: run.id,
        }),
        updated_at: new Date().toISOString(),
      };
      await upsertCanvasNode(next);
      return { threadId: targetThreadId, runId: run.id };
    },
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      await invalidate();
      void navigate({
        to: "/workspaces/$workspaceId/threads/$threadId",
        params: { workspaceId, threadId: result.threadId },
        search: { runId: result.runId, view: "chat" },
      });
    },
  });

  const markDone = useMutation({
    mutationFn: (node: CanvasNode) => setCanvasNodeStatus(asCanvasNodeId(node.id), "Done"),
    onSuccess: async () => {
      await invalidate();
    },
  });

  const persistPosition = useMutation({
    mutationFn: async (node: Node) => {
      const data = node.data as CanvasFlowNode["data"];
      const current = data.canvasNode;
      // Keep `source` untouched: dragged auto nodes must stay deletable by the
      // next "sync from conversation" rebuild, or stale cards overlap the new tree.
      const next: CanvasNode = {
        ...current,
        position_x: node.position.x,
        position_y: node.position.y,
        updated_at: new Date().toISOString(),
      };
      await upsertCanvasNode(next);
    },
  });

  const onNodeDragStop: OnNodeDrag<CanvasFlowNode> = useCallback(
    (_event, node) => {
      persistPosition.mutate(node);
    },
    [persistPosition],
  );

  const onMoveEnd = useCallback(
    (_event: unknown, viewport: Viewport) => {
      if (!canvasId) return;
      if (viewportTimer.current) window.clearTimeout(viewportTimer.current);
      viewportTimer.current = window.setTimeout(() => {
        void updateCanvasViewport(asCanvasId(canvasId), serializeViewport(viewport)).catch(
          () => undefined,
        );
      }, 400);
    },
    [canvasId],
  );

  const selectedNode =
    snapshot?.nodes.find((n) => n.id === selectedId) ??
    nodes.find((n) => n.id === selectedId)?.data.canvasNode ??
    null;
  const selectedLinks = snapshot && selectedId ? linksForNode(snapshot, selectedId) : [];

  const loadError = canvasQuery.error ?? snapshotQuery.error;
  const busy = canvasQuery.isLoading || snapshotQuery.isLoading;
  const actionError =
    addNode.error ??
    removeNode.error ??
    persistPosition.error ??
    extractInsights.error ??
    decompose.error ??
    saveNode.error ??
    launchStage.error ??
    sendToChat.error ??
    markDone.error ??
    branchFromSession.error;
  const chatBusy = launchStage.isPending || sendToChat.isPending;

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <header className="bg-surface/80 flex shrink-0 flex-col gap-2 border-b px-4 py-3">
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            {!threadId ? (
              <Link
                to="/workspaces/$workspaceId"
                params={{ workspaceId }}
                className="text-muted-foreground hover:text-foreground inline-flex items-center gap-1 text-sm"
              >
                <ArrowLeft className="h-4 w-4" />
                {t("project.back")}
              </Link>
            ) : null}
            <div className="min-w-0">
              <h1 className="truncate text-base font-semibold">
                {threadId ? t("canvas.threadTitle") : t("canvas.title")}
              </h1>
              <p className="text-muted-foreground truncate text-xs">
                {t("canvas.subtitleOverview")}
                {workspaceName ? ` · ${workspaceName}` : ""}
              </p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <div className="bg-muted/50 flex rounded-md p-0.5">
              {(
                [
                  ["all", t("canvas.layerAll")],
                  ["conversation", t("canvas.layerConversation")],
                  ["goal", t("canvas.layerGoal")],
                ] as const
              ).map(([id, label]) => (
                <Button
                  key={id}
                  type="button"
                  size="sm"
                  variant={layer === id ? "default" : "ghost"}
                  className="h-7 px-2 text-xs"
                  onClick={() => {
                    setLayer(id);
                    shouldFitRef.current = true;
                  }}
                >
                  {label}
                </Button>
              ))}
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={!canvasId || !snapshot || extractInsights.isPending}
              onClick={runExtract}
              title={t("canvas.extractInsights")}
            >
              {extractInsights.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Sparkles className="h-4 w-4" />
              )}
              {extractInsights.isPending
                ? t("canvas.extracting")
                : t("canvas.extractInsights")}
            </Button>

            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={!snapshot || addNode.isPending}
              onClick={() => addNode.mutate()}
            >
              {addNode.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Plus className="h-4 w-4" />
              )}
              {t("canvas.addNode")}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={nodes.length === 0}
              onClick={() => void fitView({ padding: 0.18, duration: 280 })}
            >
              {t("canvas.fitView")}
            </Button>
            <div
              className="bg-muted/40 text-muted-foreground ml-1 hidden items-center gap-3 rounded-md border px-2.5 py-1 text-[10px] leading-4 sm:flex"
              title={t("canvas.edgeLegendTitle")}
            >
              <span className="text-foreground/70">{t("canvas.clickToOpenSession")}</span>
              <span className="bg-border h-3 w-px" aria-hidden />
              <span className="font-medium text-foreground/80">
                {t("canvas.edgeLegendTitle")}
              </span>
              <span className="inline-flex items-center gap-1.5">
                <span
                  className="bg-muted-foreground/70 inline-block h-px w-5"
                  aria-hidden
                />
                {t("canvas.edgeLegendBranch")}
              </span>
            </div>
          </div>
        </div>
        {!threadId ? (
          <form
            className="flex flex-wrap items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              const text = goalDraft.trim();
              if (!text || !canvasId) return;
              decompose.mutate({ goalText: text });
            }}
          >
            <Input
              value={goalDraft}
              onChange={(e) => setGoalDraft(e.target.value)}
              placeholder={t("canvas.goalPlaceholder")}
              className="min-w-[200px] flex-1"
            />
            <Button
              type="submit"
              size="sm"
              disabled={!goalDraft.trim() || !snapshot || decompose.isPending}
            >
              {decompose.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <ListTree className="h-4 w-4" />
              )}
              {decompose.isPending ? t("canvas.decomposing") : t("canvas.decomposeGoal")}
            </Button>
          </form>
        ) : null}
      </header>

      {loadError ? (
        <div className="p-4">
          <ErrorAlert
            title={t("canvas.loadFailed")}
            message={loadError instanceof Error ? loadError.message : String(loadError)}
          />
        </div>
      ) : null}
      {extractInsights.error ? (
        <div className="border-b px-4 py-2">
          <ErrorAlert
            title={t("canvas.extractFailed")}
            message={(() => {
              const raw =
                extractInsights.error instanceof Error
                  ? extractInsights.error.message
                  : String(extractInsights.error);
              return /timed out/i.test(raw) ? t("canvas.extractTimeout") : raw;
            })()}
          />
        </div>
      ) : null}

      <div className="flex min-h-0 flex-1">
        <div className="relative min-h-0 min-w-0 flex-1">
          {busy ? (
            <div className="text-muted-foreground absolute inset-0 z-10 flex items-center justify-center gap-2 bg-background/40 text-sm">
              <Loader2 className="h-4 w-4 animate-spin" />
              {t("canvas.loading")}
            </div>
          ) : null}
          {!busy &&
          !extractInsights.isPending &&
          nodes.length === 0 ? (
            <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center p-8">
              <div className="bg-surface/90 pointer-events-auto max-w-md rounded-xl border p-6 text-center shadow-sm">
                <h2 className="text-base font-semibold">
                  {threadId ? t("canvas.threadEmptyTitle") : t("canvas.emptyTitle")}
                </h2>
                <p className="text-muted-foreground mt-2 text-sm">
                  {threadId ? t("canvas.threadEmptyBody") : t("canvas.emptyBody")}
                </p>
                <div className="mt-4 flex flex-wrap justify-center gap-2">
                  <Button type="button" size="sm" onClick={runExtract}>
                    <Sparkles className="h-4 w-4" />
                    {t("canvas.extractInsights")}
                  </Button>
                  {!threadId ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() => {
                        const text = goalDraft.trim() || t("canvas.defaultGoalTitle");
                        setGoalDraft(text);
                        decompose.mutate({ goalText: text });
                      }}
                    >
                      <Goal className="h-4 w-4" />
                      {t("canvas.decomposeGoal")}
                    </Button>
                  ) : null}
                </div>
              </div>
            </div>
          ) : null}
          {/*
            Syncing UX:
            - Empty map: full-surface progress (nothing to interact with yet).
            - Existing nodes: corner chip only — never cover the graph (was the
              "正在更新会话脑图…" stuck-looking overlay on every tab switch).
          */}
          {!busy && extractInsights.isPending && nodes.length === 0 ? (
            <div className="text-muted-foreground absolute inset-0 z-20 flex items-center justify-center gap-2 bg-background/50 text-sm backdrop-blur-[1px]">
              <Loader2 className="h-4 w-4 animate-spin" />
              {threadId ? t("canvas.syncingChat") : t("canvas.extracting")}
            </div>
          ) : null}
          {!busy && extractInsights.isPending && nodes.length > 0 ? (
            <div
              className="bg-surface/95 text-muted-foreground pointer-events-none absolute top-3 left-1/2 z-20 flex -translate-x-1/2 items-center gap-2 rounded-full border px-3 py-1.5 text-xs shadow-sm"
              role="status"
              aria-live="polite"
            >
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {threadId ? t("canvas.syncingChat") : t("canvas.extracting")}
            </div>
          ) : null}
          <ReactFlow
            nodes={nodes}
            edges={edges}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onNodeDragStop={onNodeDragStop}
            onMoveEnd={onMoveEnd}
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            defaultViewport={defaultViewport}
            fitView={false}
            onNodeClick={(_e, node) => {
              setNodeMenu(null);
              setSelectedId(node.id);
              const canvasNode = node.data.canvasNode;
              const tid = threadIdFromNode(
                canvasNode,
                snapshot ? linksForNode(snapshot, node.id) : [],
              );
              // Session cards: click opens the conversation (primary product action).
              if (tid && canvasNode.kind === "ThreadCluster") {
                if (tid !== threadId) {
                  openSession(tid);
                } else {
                  // Already on this session — switch back to chat view.
                  void navigate({
                    to: "/workspaces/$workspaceId/threads/$threadId",
                    params: { workspaceId, threadId: tid },
                    search: { runId: undefined, view: "chat" },
                  });
                }
              }
            }}
            onNodeDoubleClick={(_e, node) => {
              setNodeMenu(null);
              setSelectedId(node.id);
              setDetailOpen(true);
            }}
            onNodeContextMenu={(event, node) => {
              openNodeContextMenu(event, node as CanvasFlowNode);
            }}
            onPaneClick={() => {
              setSelectedId(null);
              setDetailOpen(false);
              setNodeMenu(null);
            }}
            onPaneContextMenu={(event) => {
              event.preventDefault();
              setNodeMenu(null);
            }}
            proOptions={{ hideAttribution: true }}
            className="bg-background"
          >
            <Background gap={18} size={1} />
            <MiniMap pannable zoomable className="!bg-surface" />
            <Controls showInteractive={false} />
          </ReactFlow>
        </div>
        <CanvasNodeInspector
          open={detailOpen && Boolean(selectedNode)}
          onClose={() => setDetailOpen(false)}
          workspaceId={workspaceId}
          threadId={threadId}
          node={selectedNode}
          links={selectedLinks}
          onDelete={(id) => removeNode.mutate(id)}
          onSave={(node) => saveNode.mutate(node)}
          deleting={removeNode.isPending}
          saving={saveNode.isPending}
          launching={chatBusy}
          statusPending={markDone.isPending}
          onLaunchStage={(node, mode, target) =>
            launchStage.mutate({ node, mode, target })
          }
          onSendToChat={(node, target) => sendToChat.mutate({ node, target })}
          onMarkDone={(node) => markDone.mutate(node)}
          onDecomposeGoal={(node) =>
            decompose.mutate({ goalText: node.title, parentNodeId: node.id })
          }
        />
        {nodeMenu
          ? createPortal(
              <div
                ref={nodeMenuRef}
                role="menu"
                aria-label={t("canvas.nodeContextMenu")}
                className="bg-background text-foreground border-border fixed z-[100] min-w-[11rem] rounded-md border py-1 shadow-lg"
                style={{ left: nodeMenu.x, top: nodeMenu.y }}
              >
                <button
                  type="button"
                  role="menuitem"
                  className="hover:bg-muted flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs"
                  onClick={() => {
                    setSelectedId(nodeMenu.nodeId);
                    setDetailOpen(true);
                    closeNodeMenu();
                  }}
                >
                  <Pencil className="h-3.5 w-3.5" />
                  {t("canvas.editNode")}
                </button>
                {nodeMenu.threadId ? (
                  <button
                    type="button"
                    role="menuitem"
                    className="hover:bg-muted flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs"
                    onClick={() => {
                      const tid = nodeMenu.threadId!;
                      closeNodeMenu();
                      if (tid === threadId) {
                        void navigate({
                          to: "/workspaces/$workspaceId/threads/$threadId",
                          params: { workspaceId, threadId: tid },
                          search: { runId: undefined, view: "chat" },
                        });
                      } else {
                        openSession(tid);
                      }
                    }}
                  >
                    <MessageSquare className="h-3.5 w-3.5" />
                    {t("canvas.openSession")}
                  </button>
                ) : null}
                {nodeMenu.threadId ? (
                  <button
                    type="button"
                    role="menuitem"
                    className="hover:bg-muted flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs"
                    disabled={branchFromSession.isPending}
                    onClick={() => {
                      const tid = nodeMenu.threadId!;
                      const focus = nodeMenu.title?.trim() || null;
                      closeNodeMenu();
                      branchFromSession.mutate({ parentId: tid, focusText: focus });
                    }}
                  >
                    <GitBranch className="h-3.5 w-3.5" />
                    {t("agent.selectionBranch")}
                  </button>
                ) : null}
                <button
                  type="button"
                  role="menuitem"
                  className="text-destructive hover:bg-destructive/10 flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs"
                  disabled={removeNode.isPending}
                  onClick={() => {
                    const id = nodeMenu.nodeId;
                    closeNodeMenu();
                    removeNode.mutate(id);
                  }}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t("canvas.deleteNode")}
                </button>
              </div>,
              document.body,
            )
          : null}
      </div>
      {actionError ? (
        <div className="border-t p-3">
          <ErrorAlert title={t("canvas.actionFailed")} message={String(actionError)} />
        </div>
      ) : null}
    </div>
  );
}

export function ProjectCanvasPage(props: CanvasPageProps) {
  return (
    <ReactFlowProvider>
      <CanvasWorkspace {...props} />
    </ReactFlowProvider>
  );
}

/** Alias for thread-scoped embedding. */
export function ThreadCanvasPage(props: CanvasPageProps & { threadId: string }) {
  return <ProjectCanvasPage {...props} />;
}
