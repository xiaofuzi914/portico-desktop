import { useQuery } from "@tanstack/react-query";
import { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import { listMessages } from "@/lib/tauri-api";
import type { AgentRunId, AgentRunStatus, ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { useRuntimeEvents } from "@/lib/tauri-events";
import {
  mapMessageToBlock,
  sortConversationBlocks,
  type ConversationBlock,
} from "./event-view-models";
import { ConversationEventBlock } from "./conversation-event-block";
import {
  SelectionBranchToolbar,
  type SelectionBranchPayload,
} from "./selection-branch-toolbar";

interface ConversationTimelineProps {
  threadId: ThreadId;
  workspaceId?: WorkspaceId;
  /** Currently active turn — messages for this run get a running pulse. */
  activeRunId?: AgentRunId;
  activeRunStatus?: AgentRunStatus;
  /** True while send/retry HTTP is in flight — show 思考中 immediately. */
  isSubmitting?: boolean;
  onRetry?: (content: string) => void;
  retryDisabled?: boolean;
  /** Scroll to and highlight this durable message id (canvas deep-link). */
  highlightMessageId?: string | null;
  /**
   * 划词发散：editable focus + question in one surface, then create child + send.
   */
  onBranchConfirm?: (payload: SelectionBranchPayload) => void;
  branchPending?: boolean;
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
  workspaceId,
  activeRunId,
  activeRunStatus,
  isSubmitting = false,
  onRetry,
  retryDisabled = false,
  highlightMessageId = null,
  onBranchConfirm,
  branchPending = false,
}: ConversationTimelineProps) {
  const { t } = useTranslation();
  const liveEvents = useRuntimeEvents(activeRunId);
  const selectionRootRef = useRef<HTMLDivElement>(null);

  const { data: messages, isLoading } = useQuery({
    queryKey: ["messages", threadId],
    queryFn: () => listMessages(threadId),
    // Keep timeline fresh while a turn is running (tools / final persist).
    refetchInterval:
      activeRunStatus === "Running" ||
      activeRunStatus === "Queued" ||
      activeRunStatus === "WaitingApproval" ||
      isSubmitting
        ? 700
        : false,
  });

  const runIsLive =
    activeRunStatus === "Queued" ||
    activeRunStatus === "Running" ||
    activeRunStatus === "WaitingApproval" ||
    activeRunStatus === "Paused" ||
    isSubmitting;

  const durableBlocks = useMemo(() => {
    // Preserve API order (created_at ASC, id ASC). Do NOT re-sort by Date.parse —
    // same-second user+assistant used to invert (assistant bubble above "你好").
    const mapped = (messages ?? []).map((message, index) =>
      mapMessageToBlock(message, index),
    );
    const seen = new Set<string>();
    return mapped.filter((block) => {
      if (seen.has(block.id)) return false;
      seen.add(block.id);
      return true;
    });
  }, [messages]);

  /** Map run_id → latest user prompt for that turn (for error Retry + context). */
  const userPromptByRunId = useMemo(() => {
    const map = new Map<string, string>();
    let latestUser: string | null = null;
    for (const message of messages ?? []) {
      if (message.role !== "User") continue;
      latestUser = message.content;
      if (!message.run_id) continue;
      map.set(message.run_id, message.content);
    }
    // Multi-agent: child runs have no User row; map active child → latest user text
    // so Retry on a child failure still restores the original ask.
    if (activeRunId && latestUser && !map.has(activeRunId)) {
      map.set(activeRunId, latestUser);
    }
    return map;
  }, [messages, activeRunId]);

  /** For each timeline block id, nearest preceding user text (fallback for Retry). */
  const userPromptBeforeBlockId = useMemo(() => {
    const map = new Map<string, string>();
    let lastUser: string | null = null;
    for (const block of durableBlocks) {
      if (block.role === "user" && block.body.trim()) {
        lastUser = block.body;
      } else if (
        (block.kind === "error" || block.tone === "danger") &&
        lastUser
      ) {
        map.set(block.id, lastUser);
      }
    }
    return map;
  }, [durableBlocks]);

  // Live assistant tokens for the active run (not yet in durable messages).
  // Also show a pending placeholder while send/retry is in flight (before run id).
  const streamingBlock = useMemo((): ConversationBlock | null => {
    if (!runIsLive) return null;
    if (activeRunId) {
      const hasDurableAssistant = (messages ?? []).some(
        (message) => message.run_id === activeRunId && message.role === "Assistant",
      );
      if (hasDurableAssistant) return null;
    } else if (!isSubmitting) {
      return null;
    }

    // Place stream after all durable messages so it never jumps above the user bubble.
    const sequence = durableBlocks.length * 10 + 9;
    const streamId = activeRunId ? `stream-${activeRunId}` : "stream-pending";
    const { text } = activeRunId
      ? accumulateStreamingText(liveEvents)
      : { text: "" };
    if (!text.trim()) {
      return {
        id: streamId,
        sequence,
        kind: "message",
        title: "Assistant",
        body: t("agent.streamingPlaceholder"),
        tone: "muted",
        role: "assistant",
        createdAt: new Date().toISOString(),
        raw: {
          id: -1,
          run_id: activeRunId ?? ("unknown" as AgentRunId),
          thread_id: threadId,
          sequence,
          event_type: "MessageDelta",
          payload: { role: "Assistant", content: "" },
          created_at: new Date().toISOString(),
        },
      };
    }

    return {
      id: streamId,
      sequence,
      kind: "message",
      title: "Assistant",
      body: text,
      tone: "muted",
      role: "assistant",
      createdAt: new Date().toISOString(),
      raw: {
        id: -1,
        run_id: activeRunId ?? ("unknown" as AgentRunId),
        thread_id: threadId,
        sequence,
        event_type: "MessageDelta",
        payload: { role: "Assistant", content: text },
        created_at: new Date().toISOString(),
      },
    };
  }, [
    activeRunId,
    runIsLive,
    isSubmitting,
    messages,
    liveEvents,
    threadId,
    t,
    durableBlocks.length,
  ]);

  const blocks = useMemo(() => {
    if (!streamingBlock) return durableBlocks;
    return sortConversationBlocks([...durableBlocks, streamingBlock]);
  }, [durableBlocks, streamingBlock]);

  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);
  const previousThreadIdRef = useRef(threadId);
  const previousBlockCountRef = useRef(0);
  const previousStreamLenRef = useRef(0);

  // Canvas deep-link: scroll message into view once messages are loaded.
  useEffect(() => {
    if (!highlightMessageId || isLoading) return;
    const el = document.getElementById(`msg-${highlightMessageId}`);
    if (!el) return;
    stickToBottomRef.current = false;
    el.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [highlightMessageId, isLoading, blocks.length]);

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
        className="min-h-0 flex-1 overflow-y-auto px-3 pt-4 pb-8 sm:px-5 md:px-6 lg:px-8"
      >
        {isLoading ? (
          <div className="conversation-column mx-auto w-full">
            <p className="text-muted-foreground text-sm">{t("agent.loadingConversation")}</p>
          </div>
        ) : blocks.length ? (
          <div
            ref={selectionRootRef}
            className="conversation-column mx-auto flex w-full flex-col gap-2.5"
          >
            {blocks.map((block) => {
              const runId = block.raw.run_id as AgentRunId | undefined;
              const userPrompt =
                (runId &&
                  runId !== ("unknown" as AgentRunId) &&
                  userPromptByRunId.get(runId)) ||
                userPromptBeforeBlockId.get(block.id) ||
                null;
              const isStreamingBubble = block.id.startsWith("stream-");
              // Only pulse the active run's assistant stream / incomplete assistant —
              // never paint "运行中" on older turns (looks like the last reply jumped up).
              const isRunningTurn =
                isStreamingBubble ||
                Boolean(
                  runIsLive &&
                    activeRunId &&
                    runId &&
                    runId === activeRunId &&
                    block.role === "assistant" &&
                    block.kind === "message",
                );
              const msgId = block.id.startsWith("message-")
                ? block.id.slice("message-".length)
                : null;
              const isHighlighted = Boolean(
                highlightMessageId && msgId && msgId === highlightMessageId,
              );
              return (
                <ConversationEventBlock
                  key={block.id}
                  block={block}
                  workspaceId={workspaceId}
                  userPrompt={userPrompt}
                  onRetry={onRetry}
                  retryDisabled={retryDisabled}
                  isRunning={isRunningTurn}
                  isHighlighted={isHighlighted}
                />
              );
            })}
            <div ref={bottomRef} className="h-3 w-full shrink-0" aria-hidden />
          </div>
        ) : (
          <div className="conversation-column mx-auto w-full rounded-lg border border-dashed p-6">
            <p className="text-muted-foreground text-sm">{t("agent.startThreadRunBody")}</p>
          </div>
        )}
      </div>
      {onBranchConfirm ? (
        <SelectionBranchToolbar
          containerRef={selectionRootRef}
          disabled={isSubmitting || retryDisabled}
          pending={branchPending}
          onConfirm={onBranchConfirm}
        />
      ) : null}
    </section>
  );
}
