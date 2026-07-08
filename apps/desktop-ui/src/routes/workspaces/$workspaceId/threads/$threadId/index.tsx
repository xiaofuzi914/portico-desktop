import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { cancelRun, pauseRun, resumeRun, startRun, submitMessage } from "@/lib/tauri-api";
import { useRuntimeEvents } from "@/lib/tauri-events";
import { asAgentRunId, asThreadId, asWorkspaceId } from "@/lib/schemas";
import type { AgentRunId, AgentRunStatus } from "@/lib/schemas";
import { ConversationComposer } from "@/features/agent-client/conversation-composer";
import { ConversationTimeline } from "@/features/agent-client/conversation-timeline";
import { RunControls } from "@/features/agent-client/run-controls";
import { ThreadHeader } from "@/features/agent-client/thread-header";
import { updateWorkspaceRunActivity } from "@/components/app-shell/workspace-activity-store";

export const Route = createFileRoute("/workspaces/$workspaceId/threads/$threadId/")({
  component: ThreadPage,
});

function ThreadPage() {
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

  useEffect(() => {
    void navigate({ search: (prev) => ({ ...prev, runId: activeRunId }), replace: true });
  }, [activeRunId, navigate]);

  const start = useMutation({
    mutationFn: () => startRun(workspaceId, threadId),
    onSuccess: (run) => {
      setActiveRunId(run.id);
      setActiveRunStatus(run.status);
      void queryClient.invalidateQueries({ queryKey: ["workspaces", workspaceId, "threads"] });
    },
  });

  const submit = useMutation({
    mutationFn: (content: string) =>
      activeRunId
        ? submitMessage(activeRunId, content)
        : Promise.reject(new Error("No active run")),
    onSuccess: () => {
      if (activeRunId) {
        void queryClient.invalidateQueries({
          queryKey: ["workspaces", workspaceId, "threads", threadId, "runs", activeRunId, "events"],
        });
      }
    },
  });

  const cancel = useMutation({
    mutationFn: () =>
      activeRunId ? cancelRun(activeRunId) : Promise.reject(new Error("No active run")),
  });

  const pause = useMutation({
    mutationFn: () =>
      activeRunId ? pauseRun(activeRunId) : Promise.reject(new Error("No active run")),
  });

  const resume = useMutation({
    mutationFn: () =>
      activeRunId ? resumeRun(activeRunId) : Promise.reject(new Error("No active run")),
  });

  const liveEvents = useRuntimeEvents(activeRunId);

  useEffect(() => {
    const lastEvent = liveEvents[liveEvents.length - 1];
    if (lastEvent?.kind === "RunStatusChanged") {
      setActiveRunStatus(lastEvent.data.status);
    }
  }, [liveEvents]);

  useEffect(() => {
    if (!activeRunId || !activeRunStatus) return;
    updateWorkspaceRunActivity(activeRunId, workspaceId, activeRunStatus);
  }, [activeRunId, activeRunStatus, workspaceId]);

  const controlsIsPending =
    start.isPending || cancel.isPending || pause.isPending || resume.isPending;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <ThreadHeader
        workspaceId={workspaceId}
        threadId={threadId}
        runId={activeRunId}
        status={activeRunStatus}
      />

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <ConversationTimeline
          workspaceId={workspaceId}
          threadId={threadId}
          runId={activeRunId}
          onStartRun={() => start.mutate()}
        />

        <div className="bg-surface/70 min-h-[var(--composer-min-height)] shrink-0 border-t px-6 py-4">
          <ConversationComposer
            runId={activeRunId}
            onSubmit={(content) => submit.mutate(content)}
            isSubmitting={submit.isPending}
            controls={
              <RunControls
                runId={activeRunId}
                status={activeRunStatus}
                onStartRun={() => start.mutate()}
                onCancel={() => cancel.mutate()}
                onPause={() => pause.mutate()}
                onResume={() => resume.mutate()}
                isPending={controlsIsPending}
              />
            }
          />
        </div>
      </div>
    </div>
  );
}
