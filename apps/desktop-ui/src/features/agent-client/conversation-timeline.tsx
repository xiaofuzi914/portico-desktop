import { useQuery } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import { listMessages } from "@/lib/tauri-api";
import type { AgentRunId, AgentRunStatus, ThreadId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { useRuntimeEvents } from "@/lib/tauri-events";
import { mapMessageToBlock, type ConversationBlock } from "./event-view-models";
import { ConversationEventBlock } from "./conversation-event-block";

interface ConversationTimelineProps {
  threadId: ThreadId;
  /** Currently active turn — messages for this run get a running pulse. */
  activeRunId?: AgentRunId;
  activeRunStatus?: AgentRunStatus;
  onRetry?: (content: string) => void;
  retryDisabled?: boolean;
}

/** How close to the bottom (px) counts as "following" the live conversation. */
const NEAR_BOTTOM_THRESHOLD_PX = 96;

function accumulateStreamingText(
  events: ReturnType<typeof useRuntimeEvents>,
): { text: string; completed: boolean } {
  let text = "";
  let completed = false;
  for (const event of events) {
    if (event.kind === "MessageDelta") {
      text += event.data.content;
      completed = false;
    } else if (event.kind === "MessageCompleted") {
      text = event.data.content;
      completed = true;
    }
  }
  return { text, completed };
}

export function ConversationTimeline({
  threadId,
  activeRunId,
  activeRunStatus,
  onRetry,
  retryDisabled = false,
}: ConversationTimelineProps) {
  const { t } = useTranslation();
  const liveEvents = useRuntimeEvents(activeRunId);

  const { data: messages, isLoading } = useQuery({
    queryKey: ["messages", threadId],
    queryFn: () => listMessages(threadId),
    // Keep timeline fresh while a turn is running (tools / final persist).
    refetchInterval:
      activeRunStatus === "Running" ||
      activeRunStatus === "Queued" ||
      activeRunStatus === "WaitingApproval"
        ? 1_200
        : false,
  });

  const runIsLive =
    activeRunStatus === "Queued" ||
    activeRunStatus === "Running" ||
    activeRunStatus === "WaitingApproval" ||
    activeRunStatus === "Paused";

  const durableBlocks = useMemo(() => {
    return (messages ?? []).map(mapMessageToBlock);
  }, [messages]);

  /** Map run_id → latest user prompt for that turn (for error Retry + context). */
  const userPromptByRunId = useMemo(() => {
    const map = new Map<string, string>();
    for (const message of messages ?? []) {
      if (message.role !== "User" || !message.run_id) continue;
      map.set(message.run_id, message.content);
    }
    return map;
  }, [messages]);

  // Live assistant tokens for the active run (not yet in durable messages).
  const streamingBlock = useMemo((): ConversationBlock | null => {
    if (!activeRunId || !runIsLive) return null;
    const hasDurableAssistant = (messages ?? []).some(
      (message) => message.run_id === activeRunId && message.role === "Assistant",
    );
    if (hasDurableAssistant) return null;

    const { text } = accumulateStreamingText(liveEvents);
    if (!text.trim()) {
      // Show a placeholder so the user sees the turn started.
      return {
        id: `stream-${activeRunId}`,
        sequence: Date.now(),
        kind: "message",
        title: "Assistant",
        body: t("agent.streamingPlaceholder"),
        tone: "muted",
        createdAt: new Date().toISOString(),
        raw: {
          id: -1,
          run_id: activeRunId,
          thread_id: threadId,
          sequence: Date.now(),
          event_type: "MessageDelta",
          payload: { role: "Assistant", content: "" },
          created_at: new Date().toISOString(),
        },
      };
    }

    return {
      id: `stream-${activeRunId}`,
      sequence: Date.now(),
      kind: "message",
      title: "Assistant",
      body: text,
      tone: "muted",
      createdAt: new Date().toISOString(),
      raw: {
        id: -1,
        run_id: activeRunId,
        thread_id: threadId,
        sequence: Date.now(),
        event_type: "MessageDelta",
        payload: { role: "Assistant", content: text },
        created_at: new Date().toISOString(),
      },
    };
  }, [activeRunId, runIsLive, messages, liveEvents, threadId, t]);

  const blocks = useMemo(() => {
    if (!streamingBlock) return durableBlocks;
    return [...durableBlocks, streamingBlock];
  }, [durableBlocks, streamingBlock]);

  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);
  const previousThreadIdRef = useRef(threadId);
  const previousBlockCountRef = useRef(0);
  const previousStreamLenRef = useRef(0);

  // Reset follow mode when switching sessions.
  useEffect(() => {
    if (previousThreadIdRef.current !== threadId) {
      previousThreadIdRef.current = threadId;
      stickToBottomRef.current = true;
      previousBlockCountRef.current = 0;
      previousStreamLenRef.current = 0;
    }
  }, [threadId]);

  useLayoutEffect(() => {
    const container = scrollRef.current;
    if (!container || isLoading) return;

    const blockCount = blocks.length;
    const streamLen = streamingBlock?.body.length ?? 0;
    const grew =
      blockCount > previousBlockCountRef.current || streamLen > previousStreamLenRef.current;
    const switchedThread = previousBlockCountRef.current === 0 && blockCount > 0;
    previousBlockCountRef.current = blockCount;
    previousStreamLenRef.current = streamLen;

    if (!stickToBottomRef.current && !switchedThread) return;
    if (!grew && !switchedThread && blockCount === 0) return;

    bottomRef.current?.scrollIntoView({
      block: "end",
      behavior: streamLen > 0 && grew ? "auto" : grew ? "smooth" : "auto",
    });
  }, [blocks, streamingBlock, isLoading, threadId]);

  function handleScroll() {
    const container = scrollRef.current;
    if (!container) return;
    const distanceFromBottom =
      container.scrollHeight - container.scrollTop - container.clientHeight;
    stickToBottomRef.current = distanceFromBottom <= NEAR_BOTTOM_THRESHOLD_PX;
  }

  return (
    <section className="bg-background flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="bg-surface/70 flex h-10 shrink-0 items-center justify-between border-b px-6">
        <h2 className="text-sm font-semibold">{t("agent.conversation")}</h2>
        <span className="text-muted-foreground text-xs">
          {blocks.length} {t("agent.events")}
        </span>
      </div>
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="min-h-0 flex-1 overflow-y-auto px-6 pt-6 pb-8"
      >
        {isLoading ? (
          <div className="mx-auto max-w-4xl">
            <p className="text-muted-foreground text-sm">{t("agent.loadingConversation")}</p>
          </div>
        ) : blocks.length ? (
          <div className="mx-auto flex max-w-4xl flex-col gap-3">
            {blocks.map((block) => {
              const runId = block.raw.run_id as AgentRunId | undefined;
              const userPrompt =
                runId && runId !== ("unknown" as AgentRunId)
                  ? (userPromptByRunId.get(runId) ?? null)
                  : null;
              const isStreamingBubble = block.id.startsWith("stream-");
              const isRunningTurn =
                Boolean(runIsLive && activeRunId && runId && runId === activeRunId) ||
                isStreamingBubble;
              return (
                <ConversationEventBlock
                  key={block.id}
                  block={block}
                  userPrompt={userPrompt}
                  onRetry={onRetry}
                  retryDisabled={retryDisabled}
                  isRunning={isRunningTurn}
                />
              );
            })}
            <div ref={bottomRef} className="h-3 w-full shrink-0" aria-hidden />
          </div>
        ) : (
          <div className="mx-auto max-w-4xl rounded-lg border border-dashed p-6">
            <p className="text-muted-foreground text-sm">{t("agent.startThreadRunBody")}</p>
          </div>
        )}
      </div>
    </section>
  );
}
