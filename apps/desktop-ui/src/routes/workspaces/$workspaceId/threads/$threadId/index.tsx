import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  approveRequest,
  cancelRun,
  denyRequest,
  listMessages,
  listPendingApprovals,
  listRuns,
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
import { ThreadHeader } from "@/features/agent-client/thread-header";
import { updateWorkspaceRunActivity } from "@/components/app-shell/workspace-activity-store";
import { ErrorAlert } from "@/components/ui/error-alert";
import { Button } from "@/components/ui/button";
import { ApprovalModal } from "@/components/approval/approval-modal";

export const Route = createFileRoute("/workspaces/$workspaceId/threads/$threadId/")({
  component: ThreadPage,
});

function ThreadPage() {
  const { t } = useTranslation();
  const { workspaceId: workspaceIdParam, threadId: threadIdParam } = Route.useParams();
  const workspaceId = asWorkspaceId(workspaceIdParam);
  const threadId = asThreadId(threadIdParam);
  const queryClient = useQueryClient();
  const navigate = useNavigate({ from: Route.fullPath });
  const search = Route.useSearch() as { runId?: string };

  const [activeRunId, setActiveRunId] = useState<AgentRunId | undefined>(
    search.runId ? asAgentRunId(search.runId) : undefined,
  );
  const [activeRunStatus, setActiveRunStatus] = useState<AgentRunStatus | undefined>();
  /** When set, composer restores this text (Retry / failed send recovery). */
  const [restoreDraft, setRestoreDraft] = useState<string | null>(null);

  // Switching sessions must not keep the previous thread's run badge / busy flags.
  useEffect(() => {
    setActiveRunId(search.runId ? asAgentRunId(search.runId) : undefined);
    setActiveRunStatus(undefined);
    setRestoreDraft(null);
    // Only re-seed from URL when the thread changes (not on every search edit).
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional: thread boundary only
  }, [threadId]);

  const runsQuery = useQuery({
    queryKey: ["runs", threadId],
    queryFn: () => listRuns(threadId),
    refetchInterval: activeRunStatus === "Running" || activeRunStatus === "Queued" ? 1_000 : false,
  });

  const approvalsQuery = useQuery({
    queryKey: ["pending-approvals", activeRunId],
    queryFn: () => listPendingApprovals(activeRunId),
    enabled: Boolean(activeRunId),
    refetchInterval: activeRunStatus === "WaitingApproval" ? 1_000 : false,
  });
  const pendingApproval = approvalsQuery.data?.[0];

  // Align header/composer with runs that actually belong to this thread.
  useEffect(() => {
    const runs = runsQuery.data;
    if (runs === undefined) return;

    const preferred =
      (activeRunId ? runs.find((run) => run.id === activeRunId) : undefined) ??
      (search.runId ? runs.find((run) => run.id === search.runId) : undefined) ??
      runs[0];

    if (!preferred) {
      // Empty thread (or stale runId from another session): clear UI status.
      setActiveRunId((prev) => (prev === undefined ? prev : undefined));
      setActiveRunStatus((prev) => (prev === undefined ? prev : undefined));
      return;
    }

    setActiveRunId((prev) => (prev === preferred.id ? prev : preferred.id));
    setActiveRunStatus((prev) => (prev === preferred.status ? prev : preferred.status));
  }, [threadId, runsQuery.data, activeRunId, search.runId]);

  useEffect(() => {
    void navigate({
      search: (prev) => ({ ...prev, runId: activeRunId }),
      replace: true,
    });
  }, [activeRunId, navigate, threadId]);

  const submit = useMutation({
    mutationFn: async (content: string) => {
      const run = await sendMessage(threadId, content, crypto.randomUUID());
      // First user turn defines the session topic (unless already renamed).
      void maybeAutoTitleThread(queryClient, workspaceId, threadId, content);
      return run;
    },
    onSuccess: (run) => {
      setActiveRunId(run.id);
      setActiveRunStatus(run.status);
      setRestoreDraft(null);
      void queryClient.invalidateQueries({ queryKey: ["messages", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
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

  // Drop in-flight mutation UI when leaving a session (prevents "发送中" on empty threads).
  useEffect(() => {
    submit.reset();
    cancel.reset();
    approve.reset();
    deny.reset();
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

  const controlError = submit.error ?? cancel.error ?? approve.error ?? deny.error;
  const runIsActive =
    activeRunStatus === "Queued" ||
    activeRunStatus === "Running" ||
    activeRunStatus === "WaitingApproval" ||
    activeRunStatus === "Paused";

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <ThreadHeader
        workspaceId={workspaceId}
        threadId={threadId}
        runId={activeRunId}
        status={activeRunStatus}
      />

      <ApprovalModal
        open={Boolean(pendingApproval)}
        action={pendingApproval?.action ?? ""}
        resource={pendingApproval?.resource ?? ""}
        onApprove={() => approve.mutate()}
        onDeny={() => deny.mutate()}
      />

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <ConversationTimeline
          threadId={threadId}
          activeRunId={activeRunId}
          activeRunStatus={activeRunStatus}
          retryDisabled={submit.isPending}
          onRetry={(content) => {
            // Put text back in the composer and start a fresh turn.
            setRestoreDraft(content);
            void submit.mutateAsync(content).catch(() => {
              // Draft already restored; mutation error is shown via controlError.
            });
          }}
        />

        <div className="bg-surface/70 shrink-0 border-t px-6 py-4">
          {controlError && (
            <ErrorAlert
              title={t("agent.runControlFailed")}
              message={controlError instanceof Error ? controlError.message : String(controlError)}
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
          {!runIsActive && activeRunStatus === "Failed" && (
            <div className="mb-3 flex justify-end">
              <Button
                type="button"
                variant="outline"
                disabled={submit.isPending}
                onClick={() => {
                  // Restore the failed turn's user text so it can be edited before re-send.
                  void queryClient
                    .fetchQuery({
                      queryKey: ["messages", threadId],
                      queryFn: () => listMessages(threadId),
                    })
                    .then((messages) => {
                      const prompt = [...messages]
                        .reverse()
                        .find(
                          (msg) =>
                            msg.role === "User" &&
                            (!activeRunId || msg.run_id === activeRunId),
                        )?.content;
                      if (prompt) setRestoreDraft(prompt);
                    });
                }}
              >
                {t("agent.retryLast")}
              </Button>
            </div>
          )}
          <ConversationComposer
            // Remount per thread so queue / hold / multi-role pending never leaks across sessions.
            key={threadId}
            workspaceId={workspaceId}
            threadId={threadId}
            onSubmit={async (content) => {
              await submit.mutateAsync(content);
            }}
            // Only block for the in-flight mutation — active runs stay open for queueing.
            isSubmitting={submit.isPending}
            sessionBusy={runIsActive}
            restoreDraft={restoreDraft}
            onRestoreDraftConsumed={() => setRestoreDraft(null)}
          />
        </div>
      </div>
    </div>
  );
}
