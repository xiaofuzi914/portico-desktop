import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  approveRequest,
  branchThreadFromContext,
  cancelOrchestration,
  cancelRun,
  denyRequest,
  extractCanvasInsights,
  getOrCreateProjectCanvas,
  listMessages,
  listPendingApprovals,
  listRuns,
  listThreadOrchestrations,
  listWorkspaces,
  reconcileCanvasStagesFromRun,
  sendMessage,
} from "@/lib/tauri-api";
import { useRuntimeEvents } from "@/lib/tauri-events";
import { asAgentRunId, asThreadId, asWorkspaceId } from "@/lib/schemas";
import type { AgentRunId, AgentRunStatus } from "@/lib/schemas";
import { workspaceKeys } from "@/lib/query-keys";
import { maybeAutoTitleThread } from "@/lib/maybe-auto-title-thread";
import { useTranslation } from "@/lib/i18n-react";
import { ConversationComposer } from "@/features/agent-client/conversation-composer";
import { ConversationTimeline } from "@/features/agent-client/conversation-timeline";
import {
  sendOptionsFromControl,
  type ThinkingControlState,
} from "@/features/agent-client/model-thinking-prefs";
import {
  ExecutionBoard,
  hasActiveExecution,
} from "@/features/agent-client/execution-board";
import { ThreadHeader } from "@/features/agent-client/thread-header";
import { ThreadCanvasPage } from "@/features/canvas/canvas-page";
import { updateWorkspaceRunActivity } from "@/components/app-shell/workspace-activity-store";
import { ErrorAlert } from "@/components/ui/error-alert";
import { Button } from "@/components/ui/button";
import { ApprovalModal } from "@/components/approval/approval-modal";
import { cn } from "@/lib/utils";

type ThreadView = "chat" | "mindmap" | "execution";

function parseThreadView(value: unknown): ThreadView | undefined {
  if (value === "mindmap" || value === "chat" || value === "execution") return value;
  return undefined;
}

export const Route = createFileRoute("/workspaces/$workspaceId/threads/$threadId/")({
  component: ThreadPage,
  validateSearch: (search: Record<string, unknown>): {
    runId?: string;
    view?: ThreadView;
    messageId?: string;
  } => ({
    runId: typeof search.runId === "string" ? search.runId : undefined,
    // Omit default "chat" so Links without search remain valid.
    view: parseThreadView(search.view),
    messageId: typeof search.messageId === "string" ? search.messageId : undefined,
  }),
});

function ThreadPage() {
  const { t } = useTranslation();
  const { workspaceId: workspaceIdParam, threadId: threadIdParam } = Route.useParams();
  const workspaceId = asWorkspaceId(workspaceIdParam);
  const threadId = asThreadId(threadIdParam);
  const queryClient = useQueryClient();
  const navigate = useNavigate({ from: Route.fullPath });
  const search = Route.useSearch();
  const view = search.view ?? "chat";
  const highlightMessageId = search.messageId ?? null;
  const reconciledRunRef = useRef<string | null>(null);

  const [activeRunId, setActiveRunId] = useState<AgentRunId | undefined>(
    search.runId ? asAgentRunId(search.runId) : undefined,
  );
  const [activeRunStatus, setActiveRunStatus] = useState<AgentRunStatus | undefined>();
  /** When set, composer restores this text (Retry / failed send recovery). */
  const [restoreDraft, setRestoreDraft] = useState<{
    text: string;
    nonce: number;
  } | null>(null);
  const restoreNonceRef = useRef(0);


  const { data: workspaces } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });
  const workspaceName = workspaces?.find((w) => w.id === workspaceId)?.name;

  // Switching sessions must not keep the previous thread's run badge / busy flags.
  useEffect(() => {
    setActiveRunId(search.runId ? asAgentRunId(search.runId) : undefined);
    setActiveRunStatus(undefined);
    setRestoreDraft(null);
    // Only re-seed from URL when the thread changes (not on every search edit).
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional: thread boundary only
  }, [threadId]);

  const orchestrationsQuery = useQuery({
    queryKey: ["orchestrations", threadId],
    queryFn: () => listThreadOrchestrations(threadId),
    // Always poll lightly so multi-agent background work is visible even when
    // the header run was already Completed from a prior turn.
    refetchInterval: (q) => {
      const list = q.state.data;
      const orchActive = list?.some(
        (o) => o.status === "Running" || o.status === "Planning",
      );
      if (orchActive) return 800;
      if (
        activeRunStatus === "Running" ||
        activeRunStatus === "Queued" ||
        activeRunStatus === "WaitingApproval"
      ) {
        return 800;
      }
      return false;
    },
  });

  const runsQuery = useQuery({
    queryKey: ["runs", threadId],
    queryFn: () => listRuns(threadId),
    refetchInterval: (q) => {
      const orchActive = orchestrationsQuery.data?.some(
        (o) => o.status === "Running" || o.status === "Planning",
      );
      if (orchActive) return 600;
      if (activeRunStatus === "Running" || activeRunStatus === "Queued") {
        return 600;
      }
      const runs = q.state.data;
      if (runs?.some((r) => r.status === "Running" || r.status === "Queued")) return 600;
      return false;
    },
  });

  const executionActive = hasActiveExecution(orchestrationsQuery.data, runsQuery.data);

  const approvalsQuery = useQuery({
    queryKey: ["pending-approvals", activeRunId],
    queryFn: () => listPendingApprovals(activeRunId),
    enabled: Boolean(activeRunId),
    refetchInterval: activeRunStatus === "WaitingApproval" ? 1_000 : false,
  });
  const pendingApproval = approvalsQuery.data?.[0];

  // Align header/composer with runs that actually belong to this thread.
  // Prefer the parent run of an in-flight multi-agent orchestration so the
  // header is not stuck on a previous Completed single-agent turn.
  //
  // Critical: never clobber a freshly submitted run with a stale runs cache
  // that does not yet include that id (that delayed chat "思考中" by seconds).
  useEffect(() => {
    const runs = runsQuery.data;
    if (runs === undefined) return;

    const activeOrch = orchestrationsQuery.data?.find(
      (o) => o.status === "Running" || o.status === "Planning",
    );
    const fromOrch = activeOrch
      ? runs.find((run) => run.id === activeOrch.parent_run_id)
      : undefined;
    const fromActive = activeRunId
      ? runs.find((run) => run.id === activeRunId)
      : undefined;
    const fromSearch = search.runId
      ? runs.find((run) => run.id === search.runId)
      : undefined;

    // Optimistic local run not in cache yet — keep local id/status until refetch.
    // Prevents chat lag: stale runs list used to snap back to an older Failed turn.
    if (activeRunId && !fromActive) {
      const localIsLive =
        activeRunStatus === "Queued" ||
        activeRunStatus === "Running" ||
        activeRunStatus === "WaitingApproval" ||
        activeRunStatus === "Paused";
      if (localIsLive) {
        return;
      }
    }

    const preferred = fromOrch ?? fromActive ?? fromSearch ?? runs[0];

    if (!preferred) {
      // Empty thread (or stale runId from another session): clear UI status.
      setActiveRunId((prev) => (prev === undefined ? prev : undefined));
      setActiveRunStatus((prev) => (prev === undefined ? prev : undefined));
      return;
    }

    // Do not demote a live local turn to a terminal status from a different run
    // while the live run is still missing from a partially refreshed list.
    if (
      activeRunId &&
      preferred.id !== activeRunId &&
      (activeRunStatus === "Queued" ||
        activeRunStatus === "Running" ||
        activeRunStatus === "WaitingApproval" ||
        activeRunStatus === "Paused")
    ) {
      return;
    }

    setActiveRunId((prev) => (prev === preferred.id ? prev : preferred.id));
    setActiveRunStatus((prev) => (prev === preferred.status ? prev : preferred.status));
  }, [
    threadId,
    runsQuery.data,
    orchestrationsQuery.data,
    activeRunId,
    activeRunStatus,
    search.runId,
  ]);

  useEffect(() => {
    void navigate({
      search: (prev) => ({ ...prev, runId: activeRunId, view }),
      replace: true,
    });
  }, [activeRunId, navigate, threadId, view]);

  /** Cancel stuck turns so Retry / re-send is not a silent no-op (RUN_ALREADY_ACTIVE). */
  const clearBlockingTurns = useCallback(async () => {
    try {
      const [runs, orchestrations] = await Promise.all([
        listRuns(threadId),
        listThreadOrchestrations(threadId).catch(() => []),
      ]);
      const activeStatuses = new Set([
        "Queued",
        "Running",
        "WaitingApproval",
        "Paused",
      ]);
      await Promise.all(
        runs
          .filter((r) => activeStatuses.has(r.status))
          .map((r) => cancelRun(r.id).catch(() => undefined)),
      );
      await Promise.all(
        orchestrations
          .filter((o) => o.status === "Running" || o.status === "Planning")
          .map((o) => cancelOrchestration(o.id).catch(() => undefined)),
      );
    } catch {
      /* best-effort */
    }
  }, [threadId]);

  const pushRestoreDraft = useCallback((text: string) => {
    restoreNonceRef.current += 1;
    setRestoreDraft({ text, nonce: restoreNonceRef.current });
  }, []);

  const thinkingRef = useRef<ThinkingControlState | null>(null);
  const onThinkingChange = useCallback((state: ThinkingControlState) => {
    thinkingRef.current = state;
  }, []);

  const submit = useMutation({
    mutationFn: async (content: string) => {
      const text = content.trim();
      if (!text) throw new Error("Message is empty");
      const thinkingOpts = thinkingRef.current
        ? sendOptionsFromControl(thinkingRef.current)
        : {};
      const sendOpts = {
        clientRequestId: crypto.randomUUID(),
        thinkingMode: thinkingOpts.thinkingMode,
        reasoningEffort: thinkingOpts.reasoningEffort,
      };
      try {
        const run = await sendMessage(threadId, text, sendOpts);
        void maybeAutoTitleThread(queryClient, workspaceId, threadId, text);
        return run;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        // Stuck Queued/Running turn blocks all sends — clear and retry once.
        if (msg.includes("RUN_ALREADY_ACTIVE")) {
          await clearBlockingTurns();
          const run = await sendMessage(threadId, text, {
            ...sendOpts,
            clientRequestId: crypto.randomUUID(),
          });
          void maybeAutoTitleThread(queryClient, workspaceId, threadId, text);
          return run;
        }
        throw err;
      }
    },
    onMutate: () => {
      // Immediate chat feedback while the HTTP round-trip is in flight.
      setActiveRunStatus((prev) =>
        prev === "Running" || prev === "Queued" ? prev : "Queued",
      );
    },
    onSuccess: (run) => {
      setActiveRunId(run.id);
      setActiveRunStatus(run.status === "Queued" ? "Running" : run.status);
      setRestoreDraft(null);
      // Patch runs cache so the align-effect cannot snap back to an older Failed run.
      queryClient.setQueryData(
        ["runs", threadId],
        (prev: Awaited<ReturnType<typeof listRuns>> | undefined) => {
          if (!prev) return [run];
          if (prev.some((r) => r.id === run.id)) {
            return prev.map((r) => (r.id === run.id ? run : r));
          }
          return [run, ...prev];
        },
      );
      void queryClient.invalidateQueries({ queryKey: ["messages", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["orchestrations", threadId] });
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
    },
    onError: () => {
      // Leave Failed/Interrupted badges to the next runs refetch.
      void queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
    },
  });

  const liveEvents = useRuntimeEvents(activeRunId);

  const cancel = useMutation({
    mutationFn: async () => {
      if (!activeRunId) throw new Error("No active run to cancel");
      await cancelRun(activeRunId);
    },
    onSuccess: () => {
      setActiveRunStatus("Cancelled");
      void queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
    },
  });

  const approve = useMutation({
    mutationFn: () => {
      if (!pendingApproval) throw new Error("No pending approval");
      return approveRequest(pendingApproval.id);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["pending-approvals", activeRunId] });
      void queryClient.invalidateQueries({ queryKey: ["messages", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
    },
  });
  const deny = useMutation({
    mutationFn: () => {
      if (!pendingApproval) throw new Error("No pending approval");
      return denyRequest(pendingApproval.id, "Denied by user");
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["pending-approvals", activeRunId] });
      void queryClient.invalidateQueries({ queryKey: ["messages", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
    },
  });

  /**
   * 划词发散：child session + mind-map edge + first user question.
   * focusText seeds context/title; question is the first User message on the child.
   */
  const branchInFlightRef = useRef(false);
  const branchSession = useMutation({
    mutationFn: async (args: { focusText: string; question: string }) => {
      const focus = args.focusText.trim();
      const question = args.question.trim();
      if (!focus || !question) throw new Error("focus and question required");
      if (branchInFlightRef.current) {
        throw new Error("BRANCH_ALREADY_IN_FLIGHT");
      }
      branchInFlightRef.current = true;
      try {
        const child = await branchThreadFromContext(
          workspaceId,
          threadId,
          null,
          focus,
        );
        // First question on the child so the branch is immediately active.
        // Backend already inherited the parent's active model onto the child thread.
        const run = await sendMessage(child.id, question, crypto.randomUUID());
        void maybeAutoTitleThread(queryClient, workspaceId, child.id, question);

        // Rebuild session cards so the parent → child edge appears immediately.
        try {
          const canvas = await getOrCreateProjectCanvas(workspaceId);
          const snap = await extractCanvasInsights(canvas.id);
          queryClient.setQueryData(["canvas-snapshot", canvas.id], snap);
        } catch {
          /* best-effort; next mind-map open will auto-sync */
        }
        return { child, run };
      } finally {
        branchInFlightRef.current = false;
      }
    },
    onSuccess: async ({ child, run }) => {
      await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      await queryClient.invalidateQueries({ queryKey: ["project-canvas", workspaceId] });
      // Prefetch child messages so the first paint is not empty + stale.
      void queryClient.invalidateQueries({ queryKey: ["messages", child.id] });
      void queryClient.invalidateQueries({ queryKey: ["runs", child.id] });
      void navigate({
        to: "/workspaces/$workspaceId/threads/$threadId",
        params: { workspaceId, threadId: child.id },
        search: { runId: run.id, view: "chat" },
      });
    },
  });

  // Drop in-flight mutation UI when leaving a session (prevents "发送中" on empty threads).
  useEffect(() => {
    submit.reset();
    cancel.reset();
    approve.reset();
    deny.reset();
    branchSession.reset();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only at thread boundary
  }, [threadId]);

  useEffect(() => {
    const lastEvent = liveEvents[liveEvents.length - 1];
    if (!lastEvent) return;
    if (lastEvent.kind === "RunStatusChanged") {
      setActiveRunStatus(lastEvent.data.status);
    } else if (lastEvent.kind === "RunCompleted") {
      setActiveRunStatus("Completed");
    } else if (lastEvent.kind === "RunFailed") {
      setActiveRunStatus("Failed");
    }
    if (
      lastEvent.kind === "MessageCompleted" ||
      lastEvent.kind === "RunCompleted" ||
      lastEvent.kind === "RunFailed"
    ) {
      void queryClient.invalidateQueries({ queryKey: ["messages", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["orchestrations", threadId] });
    }
    if (
      lastEvent.kind === "ToolApprovalRequired" ||
      (lastEvent.kind === "RunStatusChanged" && lastEvent.data.status === "WaitingApproval")
    ) {
      void queryClient.invalidateQueries({ queryKey: ["pending-approvals", activeRunId] });
    }
  }, [activeRunId, liveEvents, queryClient, threadId]);

  useEffect(() => {
    if (!activeRunId || !activeRunStatus) return;
    updateWorkspaceRunActivity(activeRunId, workspaceId, activeRunStatus);
  }, [activeRunId, activeRunStatus, workspaceId]);

  // Full product loop: write run outcome back to canvas Stage/Goal nodes.
  useEffect(() => {
    if (!activeRunId) return;
    if (activeRunStatus !== "Completed" && activeRunStatus !== "Failed") return;
    const key = `${activeRunId}:${activeRunStatus}`;
    if (reconciledRunRef.current === key) return;
    reconciledRunRef.current = key;
    const outcome = activeRunStatus === "Completed" ? "done" : "blocked";
    void reconcileCanvasStagesFromRun(workspaceId, threadId, activeRunId, outcome)
      .then(() => {
        void queryClient.invalidateQueries({ queryKey: ["project-canvas", workspaceId] });
        void queryClient.invalidateQueries({ queryKey: ["canvas-snapshot"] });
      })
      .catch(() => {
        // Best-effort; canvas may not have launched this run.
      });
  }, [activeRunId, activeRunStatus, queryClient, threadId, workspaceId]);

  const controlError = submit.error ?? cancel.error ?? approve.error ?? deny.error;
  const multiAgentActive = Boolean(
    orchestrationsQuery.data?.some(
      (o) => o.status === "Running" || o.status === "Planning",
    ),
  );
  const runIsActive =
    activeRunStatus === "Queued" ||
    activeRunStatus === "Running" ||
    activeRunStatus === "WaitingApproval" ||
    activeRunStatus === "Paused" ||
    multiAgentActive;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <ThreadHeader
        workspaceId={workspaceId}
        threadId={threadId}
        runId={activeRunId}
        status={activeRunStatus}
      />

      <div className="bg-surface/70 flex shrink-0 items-center gap-1 border-b px-4 py-1.5">
        <Button
          type="button"
          size="sm"
          variant={view === "chat" ? "default" : "outline"}
          className={cn("h-7")}
          onClick={() =>
            void navigate({
              search: (prev) => ({ ...prev, view: "chat" }),
              replace: true,
            })
          }
        >
          {t("canvas.viewChat")}
        </Button>
        <Button
          type="button"
          size="sm"
          variant={view === "execution" ? "default" : "outline"}
          className={cn("h-7 gap-1.5")}
          onClick={() =>
            void navigate({
              search: (prev) => ({ ...prev, view: "execution" }),
              replace: true,
            })
          }
        >
          {t("execution.viewTab")}
          {executionActive ? (
            <span
              className="bg-sky-500 h-1.5 w-1.5 shrink-0 animate-pulse rounded-full"
              aria-label={t("execution.activeDot")}
            />
          ) : null}
        </Button>
        <Button
          type="button"
          size="sm"
          variant={view === "mindmap" ? "default" : "outline"}
          className={cn("h-7")}
          onClick={() =>
            void navigate({
              search: (prev) => ({ ...prev, view: "mindmap" }),
              replace: true,
            })
          }
        >
          {t("canvas.viewMindmap")}
        </Button>
      </div>

      <ApprovalModal
        open={Boolean(pendingApproval)}
        action={pendingApproval?.action ?? ""}
        resource={pendingApproval?.resource ?? ""}
        onApprove={() => approve.mutate()}
        onDeny={() => deny.mutate()}
      />


      {view === "mindmap" ? (
        <div className="min-h-0 flex-1 overflow-hidden">
          <ThreadCanvasPage
            workspaceId={workspaceId}
            workspaceName={workspaceName}
            threadId={threadId}
          />
        </div>
      ) : view === "execution" ? (
        <div className="min-h-0 flex-1 overflow-hidden">
          <ExecutionBoard
            threadId={threadId}
            activeRunId={activeRunId}
            onSelectRun={(runId) => {
              setActiveRunId(runId);
              const run = runsQuery.data?.find((r) => r.id === runId);
              if (run) setActiveRunStatus(run.status);
            }}
            onOpenChat={() =>
              void navigate({
                search: (prev) => ({ ...prev, view: "chat" }),
                replace: true,
              })
            }
          />
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <ConversationTimeline
            threadId={threadId}
            workspaceId={workspaceId}
            activeRunId={activeRunId}
            activeRunStatus={activeRunStatus}
            isSubmitting={submit.isPending}
            highlightMessageId={highlightMessageId}
            retryDisabled={submit.isPending || cancel.isPending}
            branchPending={branchSession.isPending}
            onBranchConfirm={({ focusText, question }) => {
              if (branchSession.isPending) return;
              branchSession.mutate({ focusText, question });
            }}
            onRetry={(content) => {
              const text = content.trim();
              if (!text) return;
              // Always put text back in the composer so the user sees immediate feedback.
              pushRestoreDraft(text);
              submit.reset();
              // Optimistic: show Running in header / 思考中 without waiting for cancels.
              setActiveRunStatus("Queued");
              void (async () => {
                try {
                  // Interrupted / Failed header can still leave a Queued/Running row
                  // that makes send a silent backend rejection — clear first.
                  await clearBlockingTurns();
                  await submit.mutateAsync(text);
                } catch {
                  // Draft already restored; controlError surfaces the failure.
                }
              })();
            }}
          />

          <div className="bg-surface/70 shrink-0 border-t px-3 py-4 sm:px-5 md:px-6 lg:px-8">
            <div className="conversation-column mx-auto w-full">
            {controlError && (
              <ErrorAlert
                title={t("agent.runControlFailed")}
                message={
                  controlError instanceof Error
                    ? controlError.message.includes("RUN_ALREADY_ACTIVE")
                      ? t("agent.runAlreadyActive")
                      : controlError.message
                    : String(controlError)
                }
                className="mb-3"
              />
            )}
            {runIsActive && activeRunId && (
              <div className="mb-3 flex justify-end">
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => cancel.mutate()}
                  disabled={cancel.isPending}
                >
                  {t("agent.stop")}
                </Button>
              </div>
            )}
            {!runIsActive &&
              (activeRunStatus === "Failed" ||
                activeRunStatus === "Interrupted" ||
                activeRunStatus === "Cancelled") && (
              <div className="mb-3 flex justify-end gap-2">
                <Button
                  type="button"
                  variant="outline"
                  disabled={submit.isPending || cancel.isPending}
                  onClick={() => {
                    void queryClient
                      .fetchQuery({
                        queryKey: ["messages", threadId],
                        queryFn: () => listMessages(threadId),
                      })
                      .then((messages) => {
                        const newestFirst = [...messages].reverse();
                        const prompt =
                          newestFirst.find(
                            (msg) =>
                              msg.role === "User" &&
                              activeRunId &&
                              msg.run_id === activeRunId,
                          )?.content ??
                          newestFirst.find((msg) => msg.role === "User")?.content;
                        if (!prompt?.trim()) return;
                        pushRestoreDraft(prompt);
                        submit.reset();
                        setActiveRunStatus("Queued");
                        void (async () => {
                          try {
                            await clearBlockingTurns();
                            await submit.mutateAsync(prompt);
                          } catch {
                            /* controlError */
                          }
                        })();
                      });
                  }}
                >
                  {submit.isPending ? t("agent.retrying") : t("agent.retryLast")}
                </Button>
              </div>
            )}
            <ConversationComposer
              key={threadId}
              workspaceId={workspaceId}
              threadId={threadId}
              onSubmit={async (content) => {
                await submit.mutateAsync(content);
              }}
              onThinkingChange={onThinkingChange}
              isSubmitting={submit.isPending}
              sessionBusy={runIsActive}
              restoreDraft={restoreDraft}
              onRestoreDraftConsumed={() => setRestoreDraft(null)}
            />
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
