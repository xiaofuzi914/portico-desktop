import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Loader2, Pencil, SendHorizontal, Sparkles, X } from "lucide-react";
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
import { polishOrchestrationTask } from "./polish-orchestration-task";
import { classifyTaskMode } from "./classify-task-mode";
import { WorkflowDagEditor } from "./workflow-dag-editor";
import {
  type ComposerQueuedTask,
  queueMultiRoleTask,
  queueSendTask,
  resolveQueuedOrchestrationStart,
} from "./orchestration-queue";
import { cn } from "@/lib/utils";

const EMPTY_PATTERNS: PatternHint[] = [];

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
  /**
   * Optional external draft (Retry / failed send recovery).
   * Use `{ text, nonce }` so re-applying the same text still triggers a restore.
   */
  restoreDraft?: { text: string; nonce: number } | null;
  onRestoreDraftConsumed?: () => void;
}

/**
 * Product composer:
 * - **One primary Send** — AI/rules auto-pick single vs multi-agent recipe
 * - Advanced: optional “customize steps” for power users only
 * - Busy channel → queue next message (preserves auto-chosen mode)
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
  const [queue, setQueue] = useState<ComposerQueuedTask[]>([]);
  const [dispatchHold, setDispatchHold] = useState(false);
  const [dagEditorOpen, setDagEditorOpen] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const drainingRef = useRef(false);

  useEffect(() => {
    if (restoreDraft == null) return;
    const text = restoreDraft.text;
    if (!text) return;
    setContent(text);
    onRestoreDraftConsumed?.();
    requestAnimationFrame(() => {
      const el = textareaRef.current;
      if (!el) return;
      el.focus();
      const end = text.length;
      el.setSelectionRange(end, end);
      el.scrollIntoView({ block: "nearest", behavior: "smooth" });
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

  const classified = useMemo(
    () => classifyTaskMode(content, patterns, multiRoleReady),
    [content, patterns, multiRoleReady],
  );

  const autoModeLabel = useMemo(() => {
    if (!content.trim()) return null;
    if (classified.mode.kind === "single") {
      return t(classified.mode.labelKey);
    }
    return t(classified.mode.labelKey);
  }, [classified, content, t]);

  const orchestrate = useMutation({
    mutationFn: async (args: { task: string; workflowId?: string | null }) => {
      const result = await startOrchestration(
        workspaceId!,
        threadId!,
        args.task,
        args.workflowId ?? null,
      );
      if (workspaceId && threadId) {
        void maybeAutoTitleThread(queryClient, workspaceId, threadId, args.task);
      }
      return result;
    },
    onSuccess: async () => {
      setDispatchHold(false);
      await queryClient.invalidateQueries({ queryKey: ["orchestrations", threadId] });
      await queryClient.invalidateQueries({ queryKey: ["messages", threadId] });
      await queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
      if (workspaceId) {
        await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      }
      textareaRef.current?.focus();
    },
    onError: () => {
      setDispatchHold(false);
    },
  });

  const cancelOrchestrationMut = useMutation({
    mutationFn: (id: string) => cancelOrchestration(id as never),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["orchestrations", threadId] });
    },
  });

  const latest = sessionsQuery.data?.[0];
  const multiRoleBusy = latest?.status === "Running" || latest?.status === "Planning";
  const dispatchBusy = isSubmitting || orchestrate.isPending;
  const channelBusy = sessionBusy || multiRoleBusy || dispatchBusy || dispatchHold;

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

  useEffect(() => {
    setDispatchHold(false);
    setQueue([]);
    drainingRef.current = false;
  }, [threadId]);

  const inputDisabled = disabled;
  const hasText = content.trim().length > 0;
  const canCompose = !disabled && hasText && !dispatchBusy;

  const removeQueued = useCallback((id: string) => {
    setQueue((prev) => prev.filter((item) => item.id !== id));
  }, []);

  const dispatchQueued = useCallback(
    async (item: ComposerQueuedTask) => {
      if (item.mode === "multi-role") {
        if (!multiRoleReady) return;
        const start = resolveQueuedOrchestrationStart(item);
        if (!start) return;
        const task =
          polishOrchestrationTask(start.task, patterns).polished || start.task;
        await orchestrate.mutateAsync({
          task: task.trim(),
          workflowId: start.workflowId,
        });
        return;
      }
      const payload = item.content.trim();
      if (!payload) return;
      await onSubmit(payload);
    },
    [multiRoleReady, onSubmit, orchestrate, patterns],
  );

  useEffect(() => {
    if (channelBusy || drainingRef.current || queue.length === 0) return;
    const next = queue[0];
    if (!next) return;
    drainingRef.current = true;
    setDispatchHold(true);
    setQueue((prev) => prev.slice(1));
    void (async () => {
      try {
        await dispatchQueued(next);
      } catch {
        setDispatchHold(false);
        setQueue((prev) => [next, ...prev]);
      } finally {
        drainingRef.current = false;
        textareaRef.current?.focus();
      }
    })();
  }, [channelBusy, queue, dispatchQueued]);

  const runClassified = useCallback(
    async (raw: string) => {
      const decision = classifyTaskMode(raw, patterns, multiRoleReady);
      if (decision.mode.kind === "multi" && multiRoleReady) {
        await orchestrate.mutateAsync({
          task: decision.taskText.trim() || raw,
          workflowId: decision.mode.workflowId,
        });
        return;
      }
      await onSubmit(raw);
    },
    [multiRoleReady, onSubmit, orchestrate, patterns],
  );

  const handleSend = async () => {
    if (!canCompose) return;
    const payload = content.trim();
    if (!payload) return;

    const decision = classifyTaskMode(payload, patterns, multiRoleReady);

    // Busy → queue with the mode that would have been used now.
    if (sessionBusy || multiRoleBusy || dispatchHold) {
      if (decision.mode.kind === "multi" && multiRoleReady) {
        const wf = decision.mode.workflowId;
        setQueue((prev) => [
          ...prev,
          queueMultiRoleTask(decision.taskText || payload, wf, crypto.randomUUID()),
        ]);
      } else {
        setQueue((prev) => [...prev, queueSendTask(payload, crypto.randomUUID())]);
      }
      setContent("");
      textareaRef.current?.focus();
      return;
    }

    try {
      setContent("");
      setDispatchHold(true);
      await runClassified(payload);
      textareaRef.current?.focus();
    } catch {
      setContent(payload);
      setDispatchHold(false);
      textareaRef.current?.focus();
    } finally {
      if (!sessionBusy && !multiRoleBusy) {
        window.setTimeout(() => setDispatchHold(false), 400);
      }
    }
  };

  const startWithWorkflow = (workflowId: string | null) => {
    if (!canCompose || !multiRoleReady) return;
    const task = content.trim();
    if (!task) return;
    const polished = polishOrchestrationTask(task, patterns).polished || task;
    if (sessionBusy || multiRoleBusy || dispatchHold) {
      setQueue((prev) => [
        ...prev,
        queueMultiRoleTask(polished, workflowId, crypto.randomUUID()),
      ]);
      setContent("");
      return;
    }
    setContent("");
    setDispatchHold(true);
    setDagEditorOpen(false);
    orchestrate.mutate(
      { task: polished, workflowId },
      {
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
      },
    );
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      void handleSend();
    }
  };

  const actionError = orchestrate.error ?? cancelOrchestrationMut.error;
  const willQueue = (sessionBusy || multiRoleBusy) && hasText;
  const sendLabel =
    isSubmitting || (orchestrate.isPending && !multiRoleBusy)
      ? t("agent.sending")
      : willQueue
        ? t("agent.queueSend")
        : t("agent.send");

  return (
    <div
      className={cn(
        "conversation-column bg-background mx-auto flex w-full flex-col gap-3 rounded-lg border p-3 shadow-xs",
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
                <span className="text-muted-foreground shrink-0 rounded border px-1 py-0.5 text-[10px]">
                  {item.mode === "multi-role"
                    ? t("orchestration.auto.badgeMulti")
                    : t("orchestration.auto.badgeSingle")}
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

      {/* Light auto-mode hint — not a second primary button */}
      {hasText && multiRoleReady && autoModeLabel ? (
        <div className="bg-muted/30 text-muted-foreground flex flex-wrap items-center gap-2 rounded-md border px-3 py-1.5 text-[11px] leading-5">
          <Sparkles className="h-3.5 w-3.5 shrink-0 text-violet-600" />
          <span>
            {t("orchestration.auto.hintPrefix")}
            <span className="text-foreground font-medium">{autoModeLabel}</span>
            {classified.mode.kind === "multi" ? (
              <span className="text-muted-foreground">
                {" "}
                · {t(classified.reasonKey)}
              </span>
            ) : (
              <span className="text-muted-foreground">
                {" "}
                · {t(classified.reasonKey)}
              </span>
            )}
          </span>
        </div>
      ) : null}

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
            {latest.plan.stages && latest.plan.stages.length > 0
              ? ` · ${latest.plan.workflow_title ?? latest.plan.workflow_id ?? "workflow"} · ${latest.plan.stages
                  .map((s) => `${s.id}(${s.status})`)
                  .join(" → ")}`
              : latest.plan.subagents.length > 0
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
          {multiRoleReady ? (
            <button
              type="button"
              className="text-muted-foreground hover:text-foreground flex items-center gap-1 text-[11px] underline-offset-2 hover:underline"
              title={t("orchestration.editDagHint")}
              onClick={() => setDagEditorOpen(true)}
            >
              <Pencil className="h-3 w-3" />
              {t("orchestration.advancedCustomize")}
            </button>
          ) : null}
          <Button
            type="button"
            disabled={!canCompose}
            onClick={() => void handleSend()}
            className="gap-1.5"
            title={
              autoModeLabel
                ? `${t("orchestration.auto.hintPrefix")}${autoModeLabel}`
                : t("agent.send")
            }
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

      {dagEditorOpen && multiRoleReady ? (
        <WorkflowDagEditor
          workspaceId={workspaceId}
          open={dagEditorOpen}
          onClose={() => setDagEditorOpen(false)}
          onStart={(workflowId) => {
            if (!content.trim()) {
              setDagEditorOpen(false);
              return;
            }
            startWithWorkflow(workflowId);
          }}
        />
      ) : null}
    </div>
  );
}
