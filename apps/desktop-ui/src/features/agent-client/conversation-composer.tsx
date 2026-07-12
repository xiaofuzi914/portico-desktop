import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { GitBranch, Loader2, SendHorizontal, Users, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ErrorAlert } from "@/components/ui/error-alert";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/lib/i18n-react";
import { featureReadiness } from "@/lib/feature-readiness";
import type { PatternHint, ThreadId, WorkspaceId } from "@/lib/schemas";
import { workspaceKeys } from "@/lib/query-keys";
import {
  cancelOrchestration,
  listThreadOrchestrations,
  recallWorkflowPatterns,
  startOrchestration,
} from "@/lib/tauri-api";
import { maybeAutoTitleThread } from "@/lib/maybe-auto-title-thread";
import {
  polishOrchestrationTask,
  shouldSuggestMultiRole,
} from "./polish-orchestration-task";
import { cn } from "@/lib/utils";

const EMPTY_PATTERNS: PatternHint[] = [];

type QueueMode = "send" | "multi-role";

type QueuedTask = Readonly<{
  id: string;
  content: string;
  mode: QueueMode;
}>;

interface ConversationComposerProps {
  /** Default path: single-agent chat. Prefer Promise so draft clears only on success. */
  onSubmit: (content: string) => void | Promise<void>;
  /** True only while the current send HTTP/mutation is in flight (not while a run is active). */
  isSubmitting: boolean;
  /**
   * True when a turn is already Running/Queued/etc. Composer stays open and
   * further sends are queued for sequential execution.
   */
  sessionBusy?: boolean;
  controls?: ReactNode;
  disabled?: boolean;
  placeholder?: string;
  workspaceId?: WorkspaceId;
  threadId?: ThreadId;
  /** Optional external draft (e.g. Retry restores the last user message). */
  restoreDraft?: string | null;
  onRestoreDraftConsumed?: () => void;
}

/**
 * Product composer (see docs/AGENT-PRODUCT-PATH.md):
 * - **Send** → single agent + tools (default)
 * - **Multi-role** → orchestration (opt-in)
 * - While a session turn is busy, Send / Multi-role **queue** the next task
 *   instead of disabling the input (backend allows one active turn at a time).
 */
export function ConversationComposer({
  onSubmit,
  isSubmitting,
  sessionBusy = false,
  controls,
  disabled = false,
  placeholder,
  workspaceId,
  threadId,
  restoreDraft = null,
  onRestoreDraftConsumed,
}: ConversationComposerProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [content, setContent] = useState("");
  const [queue, setQueue] = useState<QueuedTask[]>([]);
  /** Covers the gap between dispatch resolve and parent/orchestration status flip. */
  const [dispatchHold, setDispatchHold] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const drainingRef = useRef(false);

  useEffect(() => {
    if (restoreDraft == null || restoreDraft === "") return;
    setContent(restoreDraft);
    onRestoreDraftConsumed?.();
    requestAnimationFrame(() => {
      const el = textareaRef.current;
      if (!el) return;
      el.focus();
      const end = restoreDraft.length;
      el.setSelectionRange(end, end);
    });
  }, [restoreDraft, onRestoreDraftConsumed]);

  const multiRoleReady =
    Boolean(workspaceId && threadId) && featureReadiness.multiAgentOrchestration.ready;

  const sessionsQuery = useQuery({
    queryKey: ["orchestrations", threadId],
    queryFn: () => listThreadOrchestrations(threadId!),
    enabled: multiRoleReady && Boolean(threadId),
    refetchInterval: (query) => {
      const latest = query.state.data?.[0];
      return latest?.status === "Running" || latest?.status === "Planning" ? 2_000 : false;
    },
  });

  const patternsQuery = useQuery({
    queryKey: ["workflow-patterns-recall", workspaceId, content],
    queryFn: () => recallWorkflowPatterns(content, workspaceId),
    enabled: multiRoleReady && content.trim().length > 2,
    staleTime: 8_000,
  });

  const patterns = patternsQuery.data ?? EMPTY_PATTERNS;
  const planHint = useMemo(() => polishOrchestrationTask(content, patterns), [content, patterns]);
  const showMultiRoleHint =
    multiRoleReady && content.trim().length > 2 && shouldSuggestMultiRole(planHint);

  const orchestrate = useMutation({
    mutationFn: async (task: string) => {
      const result = await startOrchestration(workspaceId!, threadId!, task);
      if (workspaceId && threadId) {
        void maybeAutoTitleThread(queryClient, workspaceId, threadId, task);
      }
      return result;
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["orchestrations", threadId] });
      await queryClient.invalidateQueries({ queryKey: ["messages", threadId] });
      await queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
      if (workspaceId) {
        await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      }
      textareaRef.current?.focus();
    },
  });

  const cancelOrchestrationMut = useMutation({
    mutationFn: (id: string) => cancelOrchestration(id as never),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["orchestrations", threadId] });
    },
  });

  const latest = sessionsQuery.data?.[0];
  // Only non-terminal orchestration counts as busy (Completed/Failed must not lock the composer).
  const multiRoleBusy = latest?.status === "Running" || latest?.status === "Planning";
  const dispatchBusy = isSubmitting || orchestrate.isPending;
  const channelBusy = sessionBusy || multiRoleBusy || dispatchBusy || dispatchHold;

  // Hold bridges the gap between mutate resolve and parent run status flip.
  // Clear when real busy takes over, or quickly if the turn never became active
  // (e.g. already Completed) so we don't show a stuck "任务运行中" banner.
  useEffect(() => {
    if (!dispatchHold) return;
    if (sessionBusy || multiRoleBusy) {
      setDispatchHold(false);
      return;
    }
    if (!isSubmitting && !orchestrate.isPending) {
      const timer = window.setTimeout(() => setDispatchHold(false), 600);
      return () => window.clearTimeout(timer);
    }
  }, [dispatchHold, sessionBusy, multiRoleBusy, isSubmitting, orchestrate.isPending]);

  // Hard reset local busy UI when switching sessions (parent also remounts via key).
  useEffect(() => {
    setDispatchHold(false);
    setQueue([]);
    drainingRef.current = false;
  }, [threadId]);

  // Input stays editable during session runs — only hard-disabled by parent policy.
  const inputDisabled = disabled;
  const hasText = content.trim().length > 0;
  const canCompose = !disabled && hasText && !dispatchBusy;

  const enqueue = useCallback((mode: QueueMode, text: string) => {
    const payload = text.trim();
    if (!payload) return;
    setQueue((prev) => [
      ...prev,
      { id: crypto.randomUUID(), content: payload, mode },
    ]);
    setContent("");
    textareaRef.current?.focus();
  }, []);

  const removeQueued = useCallback((id: string) => {
    setQueue((prev) => prev.filter((item) => item.id !== id));
  }, []);

  const dispatchTask = useCallback(
    async (mode: QueueMode, text: string) => {
      const payload = text.trim();
      if (!payload) return;
      if (mode === "multi-role") {
        if (!multiRoleReady) return;
        const task = polishOrchestrationTask(payload, patterns).polished || payload;
        await orchestrate.mutateAsync(task.trim());
        return;
      }
      await onSubmit(payload);
    },
    [multiRoleReady, onSubmit, orchestrate, patterns],
  );

  // Drain queue when channel is free (backend allows one active turn at a time).
  useEffect(() => {
    if (channelBusy || drainingRef.current || queue.length === 0) return;
    const next = queue[0];
    if (!next) return;
    drainingRef.current = true;
    setDispatchHold(true);
    setQueue((prev) => prev.slice(1));
    void (async () => {
      try {
        await dispatchTask(next.mode, next.content);
      } catch {
        setDispatchHold(false);
        // Put failed item back at the front so the user can remove/retry.
        setQueue((prev) => [next, ...prev]);
      } finally {
        drainingRef.current = false;
        textareaRef.current?.focus();
      }
    })();
  }, [channelBusy, queue, dispatchTask]);

  const handleSend = async () => {
    if (!canCompose) return;
    const payload = content.trim();
    if (!payload) return;

    // Session already working → queue; user can keep stacking tasks.
    if (sessionBusy || multiRoleBusy || dispatchHold) {
      enqueue("send", payload);
      return;
    }

    try {
      setContent("");
      setDispatchHold(true);
      await onSubmit(payload);
      textareaRef.current?.focus();
    } catch {
      setContent(payload);
      textareaRef.current?.focus();
    } finally {
      // sessionBusy covers active runs; don't leave hold stuck after terminal status.
      if (!sessionBusy && !multiRoleBusy) {
        window.setTimeout(() => setDispatchHold(false), 400);
      }
    }
  };

  const handleMultiRole = () => {
    if (!canCompose || !multiRoleReady) return;
    const task = (planHint.polished || content).trim();
    if (!task) return;

    if (sessionBusy || multiRoleBusy || dispatchHold) {
      enqueue("multi-role", task);
      return;
    }

    setContent("");
    setDispatchHold(true);
    orchestrate.mutate(task, {
      onError: () => {
        setDispatchHold(false);
        setContent(task);
        textareaRef.current?.focus();
      },
      onSettled: () => {
        if (!sessionBusy && !multiRoleBusy) {
          window.setTimeout(() => setDispatchHold(false), 400);
        }
      },
    });
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      void handleSend();
    }
  };

  const actionError = orchestrate.error ?? cancelOrchestrationMut.error;
  const willQueue = (sessionBusy || multiRoleBusy) && hasText;
  const sendLabel = dispatchBusy
    ? t("agent.sending")
    : willQueue
      ? t("agent.queueSend")
      : t("agent.send");
  const multiLabel = orchestrate.isPending
    ? t("orchestration.running")
    : willQueue
      ? t("orchestration.queueMultiRole")
      : t("orchestration.multiRole");

  return (
    <div
      className={cn(
        "bg-background mx-auto flex max-w-4xl flex-col gap-3 rounded-lg border p-3 shadow-xs",
        channelBusy && "conversation-composer-active",
      )}
    >
      {channelBusy && (
        <div className="conversation-running-banner flex items-center gap-2 rounded-md px-3 py-1.5 text-[11px] font-medium">
          <span className="conversation-running-dot" aria-hidden />
          <span>{t("agent.sessionRunning")}</span>
          {queue.length > 0 ? (
            <span className="text-muted-foreground font-normal">
              · {t("agent.queueCount").replace("{n}", String(queue.length))}
            </span>
          ) : null}
        </div>
      )}

      <Textarea
        ref={textareaRef}
        value={content}
        onChange={(event) => setContent(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={
          channelBusy
            ? t("agent.sendWhileRunningPlaceholder")
            : (placeholder ?? t("agent.sendPlaceholder"))
        }
        disabled={inputDisabled}
        className="h-20 max-h-20 min-h-20 resize-none border-0 px-1 py-1 text-sm leading-6 shadow-none focus-visible:ring-0 focus-visible:ring-offset-0"
      />

      {queue.length > 0 && (
        <div className="flex flex-col gap-1.5 border-t pt-2">
          <p className="text-muted-foreground text-[10px] font-medium tracking-wide uppercase">
            {t("agent.queueTitle")}
          </p>
          <ul className="flex flex-col gap-1">
            {queue.map((item, index) => (
              <li
                key={item.id}
                className="bg-muted/50 flex items-start gap-2 rounded-md border px-2 py-1.5 text-xs"
              >
                <span className="text-muted-foreground mt-0.5 shrink-0 tabular-nums">
                  #{index + 1}
                </span>
                <span className="text-muted-foreground shrink-0 rounded border px-1 py-0.5 text-[10px] uppercase">
                  {item.mode === "multi-role"
                    ? t("orchestration.multiRole")
                    : t("agent.send")}
                </span>
                <span className="min-w-0 flex-1 truncate leading-5">{item.content}</span>
                <button
                  type="button"
                  className="text-muted-foreground hover:text-foreground shrink-0 rounded p-0.5"
                  aria-label={t("agent.queueRemove")}
                  onClick={() => removeQueued(item.id)}
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {showMultiRoleHint && (
        <div className="bg-muted/40 text-muted-foreground flex flex-wrap items-center gap-2 rounded-md border px-3 py-2 text-[11px] leading-5">
          <Users className="h-3.5 w-3.5 shrink-0" />
          <span>
            {t("orchestration.suggestHint").replace(
              "{roles}",
              planHint.suggestedRoles.join(" → ") || "—",
            )}
          </span>
        </div>
      )}

      {multiRoleReady && latest && (multiRoleBusy || latest.status === "Failed") && (
        <div
          className={cn(
            "text-muted-foreground flex flex-wrap items-center justify-between gap-2 border-t pt-2 text-xs",
            multiRoleBusy && "conversation-running-inline rounded-md px-2 py-1.5",
          )}
        >
          <span className="min-w-0 flex-1 truncate">
            {multiRoleBusy ? (
              <span className="text-foreground mr-1.5 inline-flex items-center gap-1.5 font-medium">
                <span className="conversation-running-dot" aria-hidden />
                {t("orchestration.running")}
              </span>
            ) : (
              <span className="text-foreground font-medium">{t("orchestration.latest")}</span>
            )}
            {": "}
            {latest.status}
            {latest.plan.subagents.length > 0
              ? ` · ${latest.plan.subagents.map((n) => `${n.agent_name}(${n.status})`).join(" · ")}`
              : ""}
          </span>
          {multiRoleBusy ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={cancelOrchestrationMut.isPending}
              onClick={() => cancelOrchestrationMut.mutate(latest.id)}
            >
              {t("orchestration.cancel")}
            </Button>
          ) : null}
        </div>
      )}

      {actionError && (
        <ErrorAlert
          title={t("orchestration.failed")}
          message={actionError instanceof Error ? actionError.message : String(actionError)}
        />
      )}

      <div className="flex items-center justify-between gap-3 border-t pt-3">
        <div className="min-w-0 flex-1">{controls}</div>
        <div className="flex shrink-0 items-center gap-2">
          {multiRoleReady && (
            <Button
              type="button"
              variant="outline"
              disabled={!canCompose}
              onClick={handleMultiRole}
              title={t("orchestration.multiRoleHint")}
              className="gap-1.5"
            >
              {orchestrate.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <GitBranch className="h-4 w-4" />
              )}
              {multiLabel}
            </Button>
          )}
          <Button
            type="button"
            disabled={!canCompose}
            onClick={() => void handleSend()}
            className="gap-1.5"
          >
            {dispatchBusy ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <SendHorizontal className="h-4 w-4" />
            )}
            {sendLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
