import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { MessageSquarePlus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { listRunEvents } from "@/lib/tauri-api";
import { useRuntimeEvents } from "@/lib/tauri-events";
import type { AgentRunId, ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { runKeys } from "@/lib/query-keys";
import { mapRunEventToBlock, mergeRunEvents } from "./event-view-models";
import { ConversationEventBlock } from "./conversation-event-block";

interface ConversationTimelineProps {
  workspaceId: WorkspaceId;
  threadId: ThreadId;
  runId?: AgentRunId;
  onStartRun: () => void;
}

function EmptyState({ onStartRun }: { onStartRun: () => void }) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full min-h-[320px] flex-col items-center justify-center gap-4 px-6 text-center">
      <div className="flex h-12 w-12 items-center justify-center rounded-lg border bg-muted">
        <MessageSquarePlus className="h-5 w-5" />
      </div>
      <div className="space-y-1">
        <h2 className="text-base font-semibold">{t("agent.startThreadRunTitle")}</h2>
        <p className="text-muted-foreground max-w-sm text-sm leading-6">
          {t("agent.startThreadRunBody")}
        </p>
      </div>
      <Button onClick={onStartRun}>{t("agent.startRun")}</Button>
    </div>
  );
}

export function ConversationTimeline({
  workspaceId,
  threadId,
  runId,
  onStartRun,
}: ConversationTimelineProps) {
  const { t } = useTranslation();
  const { data: persistedEvents, isLoading } = useQuery({
    queryKey: runKeys.events(workspaceId, threadId, runId),
    queryFn: () => (runId ? listRunEvents(runId) : Promise.resolve([])),
    enabled: !!runId,
  });

  const liveEvents = useRuntimeEvents(runId);

  const blocks = useMemo(() => {
    const merged = mergeRunEvents(persistedEvents ?? [], liveEvents);
    return merged.map(mapRunEventToBlock);
  }, [persistedEvents, liveEvents]);

  return (
    <section className="flex min-h-0 flex-1 flex-col overflow-hidden bg-background">
      <div className="flex h-10 shrink-0 items-center justify-between border-b bg-surface/70 px-6">
        <h2 className="text-sm font-semibold">{t("agent.conversation")}</h2>
        <span className="text-muted-foreground text-xs">
          {blocks.length} {t("agent.events")}
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-6">
        {!runId ? (
          <EmptyState onStartRun={onStartRun} />
        ) : isLoading ? (
          <div className="mx-auto max-w-4xl">
            <p className="text-muted-foreground text-sm">{t("agent.loadingConversation")}</p>
          </div>
        ) : blocks.length ? (
          <div className="mx-auto flex max-w-4xl flex-col gap-3">
            {blocks.map((block) => (
              <ConversationEventBlock key={block.id} block={block} />
            ))}
          </div>
        ) : (
          <div className="mx-auto max-w-4xl rounded-lg border border-dashed p-6">
            <p className="text-muted-foreground text-sm">{t("agent.noEvents")}</p>
          </div>
        )}
      </div>
    </section>
  );
}
