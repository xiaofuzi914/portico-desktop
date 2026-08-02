import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Bot,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  CircleDashed,
  Clock3,
  Coins,
  Cpu,
  Layers,
  Loader2,
  Pause,
  Play,
  Square,
  Users,
  Wrench,
  XCircle,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ErrorAlert } from "@/components/ui/error-alert";
import {
  cancelOrchestration,
  cancelRun,
  getRunModelSnapshot,
  getRunTokenUsage,
  listMessages,
  listRunEvents,
  listRuns,
  continueOrchestration,
  listThreadOrchestrations,
  retryOrchestrationStage,
} from "@/lib/tauri-api";
import type {
  AgentRun,
  AgentRunId,
  Message,
  Orchestration,
  OrchestrationId,
  RunEvent,
  ThreadId,
} from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { cn } from "@/lib/utils";
import { PanelLoading } from "@/features/inspector/panel-primitives";
import {
  type BoardFilter,
  type BoardListItem,
  type BoardTaskView,
  boardStats,
  buildBoardDetail,
  buildBoardList,
  filterBoardList,
} from "./execution-board-model";

function isActive(status: string): boolean {
  return (
    status === "Running" ||
    status === "Queued" ||
    status === "Planning" ||
    status === "WaitingApproval" ||
    status === "Paused" ||
    status === "Pending"
  );
}

function statusTone(status: string): string {
  switch (status) {
    case "Completed":
      return "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 border-emerald-500/30";
    case "PartialCompleted":
    case "Interrupted":
      return "bg-amber-500/15 text-amber-800 dark:text-amber-300 border-amber-500/30";
    case "Running":
    case "Queued":
    case "Planning":
    case "Pending":
      return "bg-sky-500/15 text-sky-700 dark:text-sky-300 border-sky-500/30";
    case "WaitingApproval":
    case "Paused":
      return "bg-amber-500/15 text-amber-800 dark:text-amber-300 border-amber-500/30";
    case "Failed":
      return "bg-destructive/15 text-destructive border-destructive/30";
    default:
      return "bg-muted text-muted-foreground border-border";
  }
}

function StatusIcon({ status }: { status: string }) {
  const cls = "h-3.5 w-3.5 shrink-0";
  if (status === "Completed") return <CheckCircle2 className={cn(cls, "text-emerald-600")} />;
  if (status === "PartialCompleted")
    return <CheckCircle2 className={cn(cls, "text-amber-600")} />;
  if (isActive(status) && status !== "Pending")
    return <Loader2 className={cn(cls, "text-sky-600 animate-spin")} />;
  if (status === "WaitingApproval" || status === "Paused")
    return <Pause className={cn(cls, "text-amber-600")} />;
  if (status === "Failed") return <XCircle className={cn(cls, "text-destructive")} />;
  if (status === "Cancelled") return <Square className={cn(cls, "text-muted-foreground")} />;
  return <CircleDashed className={cn(cls, "text-muted-foreground")} />;
}

function statusLabel(t: (k: string) => string, status: string): string {
  const key = `execution.status.${status}`;
  const translated = t(key);
  return translated === key ? status : translated;
}

function truncate(text: string, max = 140): string {
  const t = text.replace(/\s+/g, " ").trim();
  if (t.length <= max) return t;
  return `${t.slice(0, max - 1)}…`;
}

function formatClock(iso: string | null | undefined): string {
  if (!iso) return "—";
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

function userPromptForRun(messages: Message[], runId: string): string {
  return messages
    .filter((m) => m.run_id === runId && m.role === "User")
    .map((m) => m.content)
    .join("\n")
    .trim();
}

function payloadRecord(payload: unknown): Record<string, unknown> {
  return typeof payload === "object" && payload !== null
    ? (payload as Record<string, unknown>)
    : {};
}

function firstString(...values: unknown[]): string | undefined {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return undefined;
}

function flatStepsFromEvents(events: RunEvent[]): BoardTaskView[] {
  const out: BoardTaskView[] = [];
  for (const event of events) {
    const type = event.event_type.toLowerCase();
    const payload = payloadRecord(event.payload);
    if (type === "llm_exchange") {
      out.push({
        id: `llm-${event.id}`,
        label: "LLM",
        status: "Completed",
        outputPreview: firstString(payloadRecord(payload.response).text) ?? null,
        itemIndex: out.length,
      });
    } else if (type === "tool_exchange" || (type.includes("tool") && type !== "tool_started")) {
      const name =
        firstString(payload.tool_name, payload.name, payload.tool) ??
        firstString(payloadRecord(payload.request).tool_name) ??
        "tool";
      out.push({
        id: `tool-${event.id}`,
        label: name,
        status: type.includes("fail") ? "Failed" : "Completed",
        outputPreview: firstString(
          typeof payload.result === "string" ? payload.result : null,
          typeof payload.error === "string" ? payload.error : null,
        ) ?? null,
        itemIndex: out.length,
      });
    }
  }
  return out;
}

interface ExecutionBoardProps {
  threadId: ThreadId;
  activeRunId?: AgentRunId;
  onSelectRun?: (runId: AgentRunId) => void;
  onOpenChat?: () => void;
}

export function ExecutionBoard({
  threadId,
  activeRunId: _activeRunId,
  onSelectRun,
  onOpenChat,
}: ExecutionBoardProps) {
  void _activeRunId;
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState<BoardFilter>("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [expandedStageId, setExpandedStageId] = useState<string | null>(null);

  const orchestrationsQuery = useQuery({
    queryKey: ["orchestrations", threadId],
    queryFn: () => listThreadOrchestrations(threadId),
    refetchInterval: (q) => {
      const data = q.state.data as Orchestration[] | undefined;
      return data?.some((o) => isActive(o.status)) ? 800 : false;
    },
  });
  const runsQuery = useQuery({
    queryKey: ["runs", threadId],
    queryFn: () => listRuns(threadId),
    refetchInterval: (q) => {
      const data = q.state.data as AgentRun[] | undefined;
      return data?.some((r) => isActive(r.status)) ? 600 : false;
    },
  });
  const messagesQuery = useQuery({
    queryKey: ["messages", threadId],
    queryFn: () => listMessages(threadId),
    refetchInterval: () => {
      // Keep prompts in sync with the board while a turn is live.
      const runs = runsQuery.data;
      if (runs?.some((r) => isActive(r.status))) return 1_200;
      return false;
    },
  });

  const orchestrations = orchestrationsQuery.data ?? [];
  const runs = runsQuery.data ?? [];
  const messages = messagesQuery.data ?? [];

  const promptByRunId = useMemo(() => {
    const map: Record<string, string> = {};
    for (const m of messages) {
      if (m.role === "User" && m.run_id) {
        map[m.run_id] = map[m.run_id] ? `${map[m.run_id]}\n${m.content}` : m.content;
      }
    }
    return map;
  }, [messages]);

  const list = useMemo(
    () => buildBoardList(orchestrations, runs, promptByRunId),
    [orchestrations, runs, promptByRunId],
  );
  const filtered = useMemo(() => filterBoardList(list, filter), [list, filter]);
  const stats = useMemo(() => boardStats(list), [list]);

  const effectiveSelectedId =
    selectedId && filtered.some((i) => i.id === selectedId)
      ? selectedId
      : (filtered[0]?.id ?? null);
  const selected: BoardListItem | null =
    filtered.find((i) => i.id === effectiveSelectedId) ?? null;

  const detailRunId: AgentRunId | null =
    selected?.kind === "run"
      ? selected.run.id
      : selected?.kind === "orchestration"
        ? selected.orchestration.parent_run_id
        : null;

  const detailQuery = useQuery({
    queryKey: ["execution-detail", detailRunId],
    enabled: Boolean(detailRunId),
    queryFn: async () => {
      const runId = detailRunId!;
      const [events, tokens, snapshot] = await Promise.all([
        listRunEvents(runId),
        getRunTokenUsage(runId).catch(() => ({
          inputTokens: 0,
          outputTokens: 0,
          totalTokens: 0,
        })),
        getRunModelSnapshot(runId).catch(() => null),
      ]);
      return { events, tokens, snapshot };
    },
    refetchInterval: () =>
      selected && isActive(selected.status) ? 700 : false,
  });

  const cancelOrch = useMutation({
    mutationFn: (id: Orchestration["id"]) => cancelOrchestration(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["orchestrations", threadId] });
      await queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
    },
  });
  const cancelSingle = useMutation({
    mutationFn: (runId: AgentRunId) => cancelRun(runId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
    },
  });
  const retryStage = useMutation({
    mutationFn: (args: { orchestrationId: OrchestrationId; stageId: string }) =>
      retryOrchestrationStage(args.orchestrationId, args.stageId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["orchestrations", threadId] });
      await queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
      await queryClient.invalidateQueries({ queryKey: ["messages", threadId] });
    },
  });
  const continueOrch = useMutation({
    mutationFn: (id: OrchestrationId) => continueOrchestration(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["orchestrations", threadId] });
      await queryClient.invalidateQueries({ queryKey: ["runs", threadId] });
      await queryClient.invalidateQueries({ queryKey: ["messages", threadId] });
    },
  });

  if (orchestrationsQuery.isLoading || runsQuery.isLoading) return <PanelLoading />;
  const error = orchestrationsQuery.error ?? runsQuery.error;
  if (error) {
    return (
      <div className="p-4">
        <ErrorAlert
          title={t("execution.loadFailed")}
          message={error instanceof Error ? error.message : String(error)}
        />
      </div>
    );
  }

  const selectItem = (item: BoardListItem) => {
    setSelectedId(item.id);
    setExpandedStageId(null);
    if (item.kind === "run") onSelectRun?.(item.run.id);
    else onSelectRun?.(item.orchestration.parent_run_id);
  };

  let flatFallback: BoardTaskView[] = [];
  if (selected?.kind === "run") {
    flatFallback = flatStepsFromEvents(detailQuery.data?.events ?? []);
    if (flatFallback.length === 0) {
      const prompt = userPromptForRun(messages, selected.run.id);
      const assistant = messages
        .filter((m) => m.run_id === selected.run.id && m.role === "Assistant")
        .at(-1);
      flatFallback = [
        {
          id: "user",
          label: t("execution.phaseUser"),
          status: "Completed",
          outputPreview: truncate(prompt || "—", 160),
          itemIndex: 0,
        },
        {
          id: "model",
          label: t("execution.phaseModel"),
          status: selected.run.status,
          outputPreview: truncate(assistant?.content ?? selected.run.status, 160),
          itemIndex: 1,
        },
      ];
    }
  }

  const detail = selected ? buildBoardDetail(selected, flatFallback) : null;

  const filters: { id: BoardFilter; label: string }[] = [
    { id: "all", label: t("execution.filterAll") },
    { id: "multi", label: t("execution.filterMulti") },
    { id: "single", label: t("execution.filterSingle") },
    { id: "active", label: t("execution.filterActive") },
    { id: "failed", label: t("execution.filterFailed") },
  ];

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <header className="shrink-0 border-b px-4 py-3">
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div>
            <h2 className="text-sm font-semibold tracking-tight">{t("execution.title")}</h2>
            <p className="text-muted-foreground mt-1 max-w-xl text-xs leading-5">
              {t("execution.helpRich")}
            </p>
          </div>
          {onOpenChat ? (
            <Button type="button" size="sm" variant="outline" onClick={onOpenChat}>
              <Play className="h-3.5 w-3.5" />
              {t("execution.goChat")}
            </Button>
          ) : null}
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          <Stat label={t("execution.statTotal")} value={stats.total} />
          <Stat label={t("execution.statActive")} value={stats.active} accent="sky" />
          <Stat label={t("execution.statMulti")} value={stats.multi} />
          <Stat label={t("execution.statDone")} value={stats.completed} accent="emerald" />
          <Stat label={t("execution.statFailed")} value={stats.failed} accent="danger" />
        </div>
        <div className="bg-muted/40 mt-3 flex flex-wrap gap-1 rounded-lg p-0.5">
          {filters.map((f) => (
            <button
              key={f.id}
              type="button"
              className={cn(
                "rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors",
                filter === f.id
                  ? "bg-background text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground",
              )}
              onClick={() => setFilter(f.id)}
            >
              {f.label}
            </button>
          ))}
        </div>
      </header>

      {filtered.length === 0 ? (
        <div className="flex flex-1 items-center justify-center p-8">
          <div className="max-w-sm text-center">
            <Bot className="text-muted-foreground mx-auto h-9 w-9 opacity-70" />
            <p className="mt-3 text-sm font-medium">{t("execution.emptyTitle")}</p>
            <p className="text-muted-foreground mt-1.5 text-xs leading-5">
              {t("execution.emptyBodyRich")}
            </p>
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col lg:flex-row">
          <div className="border-border min-h-0 w-full shrink-0 overflow-y-auto border-b lg:w-[320px] lg:border-r lg:border-b-0">
            <ul className="divide-y">
              {filtered.map((item) => {
                const active = item.id === effectiveSelectedId;
                return (
                  <li key={item.id}>
                    <button
                      type="button"
                      className={cn(
                        "hover:bg-muted/40 flex w-full flex-col gap-1 px-3 py-3 text-left transition-colors",
                        active && "bg-primary/5 border-l-primary border-l-2",
                      )}
                      onClick={() => selectItem(item)}
                    >
                      <div className="flex items-center gap-2">
                        {item.kind === "orchestration" ? (
                          <Users className="h-3.5 w-3.5 shrink-0 text-violet-600" />
                        ) : (
                          <StatusIcon status={item.status} />
                        )}
                        <span className="text-[10px] font-semibold tracking-wide uppercase">
                          {item.kind === "orchestration"
                            ? item.workflowTitle || t("execution.multiRole")
                            : t("execution.singleAgentRun")}
                        </span>
                        <span
                          className={cn(
                            "ml-auto rounded-full border px-1.5 py-0.5 text-[10px]",
                            statusTone(item.status),
                          )}
                        >
                          {statusLabel(t, item.status)}
                        </span>
                      </div>
                      <p className="line-clamp-2 text-sm font-medium leading-snug">{item.title}</p>
                      <div className="text-muted-foreground flex flex-wrap gap-2 text-[10px]">
                        {item.kind === "orchestration" && item.stageCount > 0 ? (
                          <span>
                            {item.completedStages}/{item.stageCount} {t("execution.stepsUnit")}
                          </span>
                        ) : null}
                        <span>{formatClock(item.createdAt)}</span>
                      </div>
                      {item.kind === "orchestration" && item.stageCount > 0 ? (
                        <span className="bg-muted mt-0.5 h-1 w-full overflow-hidden rounded-full">
                          <span
                            className="bg-sky-500 block h-full rounded-full transition-all"
                            style={{
                              width: `${Math.round(
                                (item.completedStages / Math.max(item.stageCount, 1)) * 100,
                              )}%`,
                            }}
                          />
                        </span>
                      ) : null}
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>

          <div className="min-h-0 min-w-0 flex-1 overflow-y-auto">
            {detail && selected ? (
              <div>
                <div className="border-b px-4 py-3">
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <div className="min-w-0 flex-1">
                      {detail.workflowTitle ? (
                        <p className="text-muted-foreground text-[10px] font-semibold tracking-wide uppercase">
                          {detail.workflowTitle}
                        </p>
                      ) : null}
                      <h3 className="mt-0.5 text-base font-semibold leading-snug tracking-tight">
                        {detail.title}
                      </h3>
                      {detail.rationale ? (
                        <p className="text-muted-foreground mt-1 text-xs leading-5">
                          {detail.rationale}
                        </p>
                      ) : null}
                    </div>
                    <div className="flex gap-1">
                      {onOpenChat ? (
                        <Button type="button" size="sm" variant="outline" className="h-8" onClick={onOpenChat}>
                          {t("execution.openChat")}
                        </Button>
                      ) : null}
                      {isActive(detail.status) ? (
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          className="h-8"
                          disabled={cancelOrch.isPending || cancelSingle.isPending}
                          onClick={() => {
                            if (selected.kind === "orchestration") {
                              cancelOrch.mutate(selected.orchestration.id);
                            } else {
                              cancelSingle.mutate(selected.run.id);
                            }
                          }}
                        >
                          {t("agent.stop")}
                        </Button>
                      ) : null}
                      {selected?.kind === "orchestration" &&
                      (detail.status === "Interrupted" ||
                        detail.status === "PartialCompleted" ||
                        detail.status === "Failed") ? (
                        <Button
                          type="button"
                          size="sm"
                          variant="default"
                          className="h-8"
                          disabled={continueOrch.isPending}
                          onClick={() => continueOrch.mutate(selected.orchestration.id)}
                        >
                          {t("execution.continueSession")}
                        </Button>
                      ) : null}
                    </div>
                  </div>
                  <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
                    <Meta
                      icon={Cpu}
                      label={t("execution.metaModel")}
                      value={
                        detailQuery.data?.snapshot
                          ? `${detailQuery.data.snapshot.provider_name} / ${detailQuery.data.snapshot.model_name}`
                          : "—"
                      }
                    />
                    <Meta
                      icon={Coins}
                      label={t("execution.metaTokens")}
                      value={
                        detailQuery.data?.tokens && detailQuery.data.tokens.totalTokens > 0
                          ? String(detailQuery.data.tokens.totalTokens)
                          : "—"
                      }
                    />
                    <Meta
                      icon={Layers}
                      label={t("execution.metaSteps")}
                      value={String(
                        detail.stages.length > 0
                          ? detail.stages.length
                          : detail.flatSteps.length,
                      )}
                    />
                    <Meta
                      icon={Clock3}
                      label={t("execution.metaDuration")}
                      value={formatClock(
                        selected.kind === "run"
                          ? selected.run.created_at
                          : selected.orchestration.created_at,
                      )}
                    />
                  </div>
                </div>

                {/* Stage pipeline — progressive disclosure */}
                <div className="px-4 py-3">
                  <h4 className="mb-2 text-xs font-semibold tracking-wide uppercase">
                    {detail.stages.length > 0
                      ? t("execution.stagesTitle")
                      : t("execution.pipelineTitle")}
                  </h4>

                  {detail.stages.length > 0 ? (
                    <ol className="space-y-2">
                      {detail.stages.map((stage, idx) => {
                        const open = expandedStageId === stage.id || detail.stages.length <= 2;
                        return (
                          <li key={stage.id} className="overflow-hidden rounded-xl border">
                            <button
                              type="button"
                              className="hover:bg-muted/40 flex w-full items-center gap-2 px-3 py-2.5 text-left"
                              onClick={() =>
                                setExpandedStageId((cur) =>
                                  cur === stage.id ? null : stage.id,
                                )
                              }
                            >
                              <span className="text-muted-foreground w-5 font-mono text-[11px]">
                                {idx + 1}
                              </span>
                              <StatusIcon status={stage.status} />
                              <div className="min-w-0 flex-1">
                                <div className="flex flex-wrap items-center gap-1.5">
                                  <span className="text-sm font-medium">{stage.title}</span>
                                  <KindPill kind={stage.kind} t={t} />
                                  <span
                                    className={cn(
                                      "rounded-full border px-1.5 py-0.5 text-[10px]",
                                      statusTone(stage.status),
                                    )}
                                  >
                                    {statusLabel(t, stage.status)}
                                  </span>
                                </div>
                                <p className="text-muted-foreground mt-0.5 text-[11px]">
                                  {stage.agentName}
                                  {stage.modelTier ? ` · ${stage.modelTier}` : null}
                                  {stage.thinkingMode ? ` · 💭${stage.thinkingMode}` : null}
                                  {stage.kind === "loop" && stage.maxIterations
                                    ? ` · ${t("execution.loopRound")
                                        .replace(
                                          "{n}",
                                          String(stage.currentIteration ?? 0),
                                        )
                                        .replace("{max}", String(stage.maxIterations))}`
                                    : null}
                                  {stage.taskCount > 0
                                    ? ` · ${stage.completedTasks}/${stage.taskCount} ${t("execution.tasksUnit")}`
                                    : null}
                                </p>
                              </div>
                              {open ? (
                                <ChevronDown className="text-muted-foreground h-4 w-4" />
                              ) : (
                                <ChevronRight className="text-muted-foreground h-4 w-4" />
                              )}
                            </button>
                            {open ? (
                              <div className="bg-muted/20 border-t px-3 py-2">
                                {stage.errorMessage ? (
                                  <p className="text-destructive mb-2 text-xs">{stage.errorMessage}</p>
                                ) : null}
                                {stage.allowedTools.length > 0 ? (
                                  <p className="text-muted-foreground mb-2 text-[10px]">
                                    tools: {stage.allowedTools.slice(0, 8).join(", ")}
                                    {stage.allowedTools.length > 8 ? "…" : ""}
                                  </p>
                                ) : null}
                                {stage.canRetry && selected?.kind === "orchestration" ? (
                                  <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    className="mb-2 h-7 text-xs"
                                    disabled={retryStage.isPending}
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      retryStage.mutate({
                                        orchestrationId: selected.orchestration.id,
                                        stageId: stage.id,
                                      });
                                    }}
                                  >
                                    {t("execution.retryStage")}
                                  </Button>
                                ) : null}
                                {stage.tasks.length === 0 ? (
                                  <p className="text-muted-foreground text-xs">
                                    {isActive(stage.status)
                                      ? t("execution.planning")
                                      : t("execution.noTasks")}
                                  </p>
                                ) : (
                                  <ul className="space-y-1.5">
                                    {stage.tasks.map((task) => (
                                      <li
                                        key={task.id}
                                        className="bg-background flex items-start gap-2 rounded-lg border px-2.5 py-2"
                                      >
                                        <StatusIcon status={task.status} />
                                        <div className="min-w-0 flex-1">
                                          <p className="text-xs font-medium">{task.label}</p>
                                          {task.outputPreview ? (
                                            <p className="text-muted-foreground mt-0.5 line-clamp-3 text-[11px] leading-4">
                                              {task.outputPreview}
                                            </p>
                                          ) : null}
                                        </div>
                                      </li>
                                    ))}
                                  </ul>
                                )}
                              </div>
                            ) : null}
                          </li>
                        );
                      })}
                    </ol>
                  ) : (
                    <ol className="ml-2 space-y-2 border-l pl-4">
                      {detail.flatSteps.map((step, idx) => (
                        <li key={step.id} className="relative">
                          <span className="bg-background absolute top-2 -left-[21px] h-2.5 w-2.5 rounded-full border" />
                          <div className="rounded-lg border px-3 py-2">
                            <div className="flex items-center gap-2">
                              <span className="text-muted-foreground font-mono text-[10px]">
                                {idx + 1}
                              </span>
                              {step.label === "LLM" ? (
                                <Cpu className="h-3.5 w-3.5 text-sky-600" />
                              ) : step.label.includes("tool") || step.label.length < 24 ? (
                                <Wrench className="h-3.5 w-3.5 text-amber-700" />
                              ) : (
                                <Layers className="text-muted-foreground h-3.5 w-3.5" />
                              )}
                              <span className="text-sm font-medium">{step.label}</span>
                              <span
                                className={cn(
                                  "rounded-full border px-1.5 py-0.5 text-[10px]",
                                  statusTone(step.status),
                                )}
                              >
                                {statusLabel(t, step.status)}
                              </span>
                            </div>
                            {step.outputPreview ? (
                              <p className="text-muted-foreground mt-1 line-clamp-3 text-xs leading-4">
                                {step.outputPreview}
                              </p>
                            ) : null}
                          </div>
                        </li>
                      ))}
                    </ol>
                  )}
                </div>

                {detail.resultSummary ? (
                  <div className="border-t px-4 py-3">
                    <h4 className="text-muted-foreground text-[10px] font-semibold tracking-wide uppercase">
                      {t("execution.result")}
                    </h4>
                    <p className="mt-1 whitespace-pre-wrap text-sm leading-6">
                      {detail.resultSummary}
                    </p>
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>
        </div>
      )}

      {(cancelOrch.error || cancelSingle.error) && (
        <div className="border-t p-3">
          <ErrorAlert
            title={t("execution.actionFailed")}
            message={String(cancelOrch.error ?? cancelSingle.error)}
          />
        </div>
      )}
    </div>
  );
}

function Stat({
  label,
  value,
  accent,
}: {
  label: string;
  value: number;
  accent?: "sky" | "emerald" | "danger";
}) {
  return (
    <div className="bg-muted/40 flex items-center gap-1.5 rounded-lg border px-2 py-1 text-[11px]">
      <span className="text-muted-foreground">{label}</span>
      <span
        className={cn(
          "font-semibold tabular-nums",
          accent === "sky" && "text-sky-700",
          accent === "emerald" && "text-emerald-700",
          accent === "danger" && "text-destructive",
        )}
      >
        {value}
      </span>
    </div>
  );
}

function Meta({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof Cpu;
  label: string;
  value: string;
}) {
  return (
    <div className="bg-muted/30 rounded-lg border px-2.5 py-2">
      <div className="text-muted-foreground flex items-center gap-1 text-[10px]">
        <Icon className="h-3 w-3" />
        {label}
      </div>
      <p className="mt-0.5 truncate text-xs font-medium" title={value}>
        {value}
      </p>
    </div>
  );
}

function KindPill({ kind, t }: { kind: string; t: (k: string) => string }) {
  const label =
    kind === "foreach"
      ? t("execution.kindForeach")
      : kind === "reduce"
        ? t("execution.kindReduce")
        : kind === "loop"
          ? t("execution.kindLoop")
          : t("execution.kindSingle");
  return (
    <span className="bg-muted text-muted-foreground rounded px-1.5 py-0.5 text-[10px] font-medium">
      {label}
    </span>
  );
}

export function hasActiveExecution(
  orchestrations: Orchestration[] | undefined,
  runs: AgentRun[] | undefined,
): boolean {
  if (orchestrations?.some((o) => isActive(o.status))) return true;
  if (runs?.some((r) => isActive(r.status))) return true;
  return false;
}
