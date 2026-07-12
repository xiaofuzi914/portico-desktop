import { useQueries, useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import {
  Bot,
  Braces,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  CircleCheck,
  CircleDashed,
  Loader2,
  Pause,
  ShieldAlert,
  User,
  Wrench,
} from "lucide-react";
import { getRunTokenUsage, listMessages, listRunEvents, listRuns } from "@/lib/tauri-api";
import type { AgentRun, AgentRunId, Message, RunEvent, ThreadId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { cn } from "@/lib/utils";
import { EmptyState, InlineError, PanelLoading } from "./panel-primitives";
import { JsonTree } from "./json-tree";

interface TimelinePanelProps {
  threadId?: ThreadId;
  /** Currently selected run in the URL (highlighted when present). */
  activeRunId?: AgentRunId;
}

/** Cap how many recent turns we load events/usage for. */
const MAX_TURNS = 40;

type ExchangeKind = "llm" | "tool";

interface ExchangeStep {
  id: string;
  kind: ExchangeKind;
  stepIndex: number;
  /** Human-readable label (tool name or "LLM"). */
  label: string;
  request: unknown;
  response: unknown;
  durationMs: number | null;
  inputTokens: number | null;
  outputTokens: number | null;
  estimated: boolean;
  status: string | null;
  createdAt: string;
  /** Short text for readable mode. */
  requestPreview: string;
  responsePreview: string;
}

interface ApiTurn {
  run: AgentRun;
  /** What the user sent (API request body content). */
  request: string;
  /** What the model returned (final assistant text). */
  response: string;
  /** Optional system/error text for failed turns. */
  errorText: string | null;
  requestAt: string;
  responseAt: string | null;
  /** Fine-grained LLM / tool exchanges inside this turn. */
  exchanges: ExchangeStep[];
}

function truncate(text: string, max = 160): string {
  const cleaned = text.replace(/\s+/g, " ").trim();
  if (cleaned.length <= max) return cleaned;
  return `${cleaned.slice(0, max - 1)}…`;
}

function formatClock(iso: string): string {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return iso;
  }
}

function formatDuration(run: AgentRun, nowMs = Date.now()): string {
  const startIso = run.started_at ?? run.created_at;
  const start = Date.parse(startIso);
  if (Number.isNaN(start)) return "—";
  const end = run.completed_at ? Date.parse(run.completed_at) : nowMs;
  if (Number.isNaN(end) || end < start) return "—";
  return formatMs(end - start);
}

function formatMs(ms: number): string {
  if (ms < 1000) return `${Math.max(0, Math.round(ms))}ms`;
  const sec = ms / 1000;
  if (sec < 60) return `${sec < 10 ? sec.toFixed(1) : Math.round(sec)}s`;
  const m = Math.floor(sec / 60);
  const s = Math.round(sec % 60);
  return `${m}m${s.toString().padStart(2, "0")}s`;
}

function formatTokens(input: number, output: number, estimated = false): string {
  const total = input + output;
  if (total <= 0) return "—";
  const fmt = (n: number) =>
    n >= 10_000 ? `${(n / 1000).toFixed(1)}k` : n.toLocaleString();
  const prefix = estimated ? "~" : "";
  return `${prefix}${fmt(total)} · ↑${fmt(input)} · ↓${fmt(output)}`;
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

function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() && !Number.isNaN(Number(value))) {
    return Number(value);
  }
  return null;
}

function prettyJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function previewFromJson(value: unknown, max = 140): string {
  if (value == null) return "—";
  if (typeof value === "string") return truncate(value, max);
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return truncate(JSON.stringify(value), max);
  } catch {
    return "—";
  }
}

function llmRequestPreview(request: unknown): string {
  const rec = payloadRecord(request);
  const messages = rec.messages;
  if (Array.isArray(messages) && messages.length > 0) {
    const last = payloadRecord(messages[messages.length - 1]);
    const role = firstString(last.role) ?? "message";
    const content = firstString(last.content) ?? previewFromJson(last.message_type ?? last, 100);
    return `${role}: ${truncate(content, 120)}`;
  }
  return previewFromJson(request, 140);
}

function llmResponsePreview(response: unknown): string {
  const rec = payloadRecord(response);
  const text = firstString(rec.text);
  if (text) return truncate(text, 140);
  const calls = rec.tool_calls;
  if (Array.isArray(calls) && calls.length > 0) {
    const names = calls
      .map((c) => {
        const r = payloadRecord(c);
        const fn = payloadRecord(r.function);
        return firstString(fn.name, r.name) ?? "tool";
      })
      .join(", ");
    return `tool_calls: ${names}`;
  }
  return previewFromJson(response, 140);
}

function parseExchangeEvents(events: RunEvent[]): ExchangeStep[] {
  const steps: ExchangeStep[] = [];
  for (const event of events) {
    const type = event.event_type.toLowerCase();
    if (type !== "llm_exchange" && type !== "tool_exchange") continue;
    const payload = payloadRecord(event.payload);
    const kind: ExchangeKind =
      type === "tool_exchange" || payload.kind === "tool" ? "tool" : "llm";
    const request = payload.request ?? null;
    const response = payload.response ?? null;
    const usage = payloadRecord(payload.usage);
    const toolName = firstString(payload.tool_name, payloadRecord(request).tool_name);
    const stepIndex = asNumber(payload.step) ?? steps.length;
    const durationMs = asNumber(payload.duration_ms);
    const inputTokens = asNumber(usage.input_tokens);
    const outputTokens = asNumber(usage.output_tokens);
    const estimated = usage.estimated === true;

    if (kind === "llm") {
      steps.push({
        id: `llm-${event.id}`,
        kind: "llm",
        stepIndex,
        label: "LLM",
        request,
        response,
        durationMs,
        inputTokens,
        outputTokens,
        estimated,
        status: null,
        createdAt: event.created_at,
        requestPreview: llmRequestPreview(request),
        responsePreview: llmResponsePreview(response),
      });
    } else {
      steps.push({
        id: `tool-${event.id}`,
        kind: "tool",
        stepIndex,
        label: toolName ?? "tool",
        request,
        response,
        durationMs,
        inputTokens: null,
        outputTokens: null,
        estimated: false,
        status: firstString(payload.status) ?? null,
        createdAt: event.created_at,
        requestPreview: previewFromJson(payloadRecord(request).arguments ?? request, 140),
        responsePreview: previewFromJson(response, 140),
      });
    }
  }
  return steps;
}

/** Fallback when older runs have no llm_exchange / tool_exchange rows. */
function buildLegacyExchanges(
  run: AgentRun,
  messages: Message[],
  events: RunEvent[],
): ExchangeStep[] {
  const runMessages = messages.filter((m) => m.run_id === run.id);
  const userMsgs = runMessages.filter((m) => m.role === "User");
  const assistantMsgs = runMessages.filter((m) => m.role === "Assistant");
  const steps: ExchangeStep[] = [];

  if (userMsgs.length || assistantMsgs.length) {
    const requestText = userMsgs.map((m) => m.content).join("\n\n").trim() || "—";
    const responseText =
      [...assistantMsgs].reverse()[0]?.content?.trim() ??
      (run.status === "Running" || run.status === "Queued" ? "" : "—");
    steps.push({
      id: `legacy-llm-${run.id}`,
      kind: "llm",
      stepIndex: 0,
      label: "LLM",
      request: { messages: [{ role: "user", content: requestText }] },
      response: { text: responseText, tool_calls: [] },
      durationMs: null,
      inputTokens: null,
      outputTokens: null,
      estimated: false,
      status: null,
      createdAt: userMsgs[0]?.created_at ?? run.created_at,
      requestPreview: truncate(requestText, 140),
      responsePreview: responseText ? truncate(responseText, 140) : "—",
    });
  }

  // Surface tool events from the live bus if any were persisted by other paths.
  for (const event of events) {
    const type = event.event_type.toLowerCase();
    if (!type.includes("tool")) continue;
    if (type === "tool_exchange") continue;
    const payload = payloadRecord(event.payload);
    const name = firstString(payload.tool_name, payload.name, payload.tool);
    if (!name) continue;
    steps.push({
      id: `legacy-tool-${event.id}`,
      kind: "tool",
      stepIndex: steps.length,
      label: name,
      request: {
        tool_name: name,
        arguments: payload.arguments ?? payload,
      },
      response: payload.result ?? payload.error ?? payload,
      durationMs: null,
      inputTokens: null,
      outputTokens: null,
      estimated: false,
      status: type.includes("fail") ? "error" : type.includes("complete") ? "ok" : null,
      createdAt: event.created_at,
      requestPreview: previewFromJson(payload.arguments ?? payload, 140),
      responsePreview: previewFromJson(payload.result ?? payload.error ?? payload, 140),
    });
  }

  return steps;
}

function buildTurn(run: AgentRun, messages: Message[], events: RunEvent[]): ApiTurn {
  const runMessages = messages.filter((m) => m.run_id === run.id);
  const userMsgs = runMessages.filter((m) => m.role === "User");
  const assistantMsgs = runMessages.filter((m) => m.role === "Assistant");
  const systemMsgs = runMessages.filter((m) => m.role === "System");

  const request = userMsgs.map((m) => m.content).join("\n\n").trim();
  const response = [...assistantMsgs].reverse()[0]?.content?.trim() ?? "";
  const errorText =
    systemMsgs.length > 0
      ? systemMsgs.map((m) => m.content).join("\n").trim()
      : run.status === "Failed" || run.status === "Interrupted"
        ? firstString(
            ...events
              .filter(
                (e) =>
                  e.event_type.toLowerCase().includes("fail") ||
                  e.event_type.toLowerCase().includes("error"),
              )
              .map((e) => {
                const p = payloadRecord(e.payload);
                return firstString(p.message, p.error, p.content);
              }),
          ) ?? null
        : null;

  const requestAt = userMsgs[0]?.created_at ?? run.started_at ?? run.created_at;
  const responseAt = assistantMsgs.at(-1)?.created_at ?? run.completed_at ?? null;

  const exchanges = parseExchangeEvents(events);
  const finalExchanges =
    exchanges.length > 0 ? exchanges : buildLegacyExchanges(run, messages, events);

  return {
    run,
    request: request || "—",
    response,
    errorText,
    requestAt,
    responseAt,
    exchanges: finalExchanges,
  };
}

function statusTone(status: AgentRun["status"]): string {
  switch (status) {
    case "Completed":
      return "text-emerald-700 bg-emerald-50 border-emerald-200";
    case "Failed":
    case "Interrupted":
    case "Cancelled":
      return "text-red-700 bg-red-50 border-red-200";
    case "Running":
    case "Queued":
      return "text-sky-700 bg-sky-50 border-sky-200";
    case "WaitingApproval":
      return "text-amber-700 bg-amber-50 border-amber-200";
    case "Paused":
      return "text-muted-foreground bg-muted border-border";
    default:
      return "text-muted-foreground bg-muted border-border";
  }
}

function StatusIcon({ status }: { status: AgentRun["status"] }) {
  const className = "h-3.5 w-3.5 shrink-0";
  switch (status) {
    case "Completed":
      return <CircleCheck className={cn(className, "text-emerald-600")} />;
    case "Failed":
    case "Interrupted":
    case "Cancelled":
      return <CircleAlert className={cn(className, "text-red-600")} />;
    case "Running":
    case "Queued":
      return <Loader2 className={cn(className, "animate-spin text-sky-600")} />;
    case "WaitingApproval":
      return <ShieldAlert className={cn(className, "text-amber-600")} />;
    case "Paused":
      return <Pause className={cn(className, "text-muted-foreground")} />;
    default:
      return <CircleDashed className={cn(className, "text-muted-foreground")} />;
  }
}

function statusLabel(status: AgentRun["status"], t: (key: string) => string): string {
  const key = `inspector.runStatus.${status}` as const;
  const translated = t(key);
  return translated === key ? status : translated;
}

function readableRequestBody(exchange: ExchangeStep): string {
  if (exchange.kind === "tool") {
    const args = payloadRecord(exchange.request).arguments ?? exchange.request;
    return prettyJson(args);
  }
  const rec = payloadRecord(exchange.request);
  const messages = rec.messages;
  if (Array.isArray(messages)) {
    return messages
      .map((m, i) => {
        const msg = payloadRecord(m);
        const role = firstString(msg.role) ?? `msg${i}`;
        const content = firstString(msg.content);
        if (content) return `[${role}] ${content}`;
        // tool_use / tool_result live in message_type
        return `[${role}] ${previewFromJson(msg.message_type ?? msg, 400)}`;
      })
      .join("\n\n");
  }
  return prettyJson(exchange.request);
}

function readableResponseBody(exchange: ExchangeStep): string {
  if (exchange.kind === "tool") {
    return prettyJson(exchange.response);
  }
  const rec = payloadRecord(exchange.response);
  const text = firstString(rec.text);
  const calls = rec.tool_calls;
  const parts: string[] = [];
  if (text) parts.push(text);
  if (Array.isArray(calls) && calls.length > 0) {
    parts.push(`tool_calls:\n${prettyJson(calls)}`);
  }
  if (parts.length) return parts.join("\n\n");
  return prettyJson(exchange.response);
}

function ExchangeCard({
  exchange,
  index,
  jsonMode,
  t,
}: {
  exchange: ExchangeStep;
  index: number;
  jsonMode: boolean;
  t: (key: string) => string;
}) {
  const [open, setOpen] = useState(index === 0 || exchange.kind === "llm");
  const isLlm = exchange.kind === "llm";

  return (
    <div
      className={cn(
        "rounded-md border",
        isLlm ? "border-violet-100 bg-violet-50/30" : "border-amber-100 bg-amber-50/30",
      )}
    >
      <button
        type="button"
        className="hover:bg-muted/30 flex w-full items-start gap-2 px-2 py-1.5 text-left transition-colors"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <span className="text-muted-foreground mt-0.5">
          {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
        </span>
        <span className="mt-0.5">
          {isLlm ? (
            <Bot className="h-3.5 w-3.5 text-violet-800" />
          ) : (
            <Wrench className="h-3.5 w-3.5 text-amber-800" />
          )}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="text-[11px] font-semibold">
              {t("inspector.timelineStepN")
                .replace("{n}", String(index + 1))
                .replace("{label}", isLlm ? t("inspector.timelineStepLlm") : exchange.label)}
            </span>
            {exchange.durationMs != null ? (
              <span className="text-muted-foreground bg-background/70 rounded-full border px-1.5 py-px text-[10px] tabular-nums">
                ⏱ {formatMs(exchange.durationMs)}
              </span>
            ) : null}
            {isLlm && exchange.inputTokens != null && exchange.outputTokens != null ? (
              <span
                className="text-muted-foreground bg-background/70 rounded-full border px-1.5 py-px text-[10px] tabular-nums"
                title={
                  exchange.estimated
                    ? t("inspector.timelineTokensEstimated")
                    : t("inspector.timelineTokens")
                }
              >
                {formatTokens(exchange.inputTokens, exchange.outputTokens, exchange.estimated)}
              </span>
            ) : null}
            {exchange.status ? (
              <span className="text-muted-foreground rounded-full border px-1.5 py-px text-[10px]">
                {exchange.status}
              </span>
            ) : null}
          </div>
          {!open ? (
            <p className="text-muted-foreground mt-0.5 line-clamp-1 text-[10px]">
              {truncate(exchange.requestPreview, 80)}
              {exchange.responsePreview ? ` → ${truncate(exchange.responsePreview, 60)}` : ""}
            </p>
          ) : null}
        </div>
      </button>

      {open ? (
        <div className="space-y-2 border-t px-2 py-2">
          {jsonMode ? (
            <>
              <JsonTree
                key={`${exchange.id}-req`}
                title={t("inspector.timelineJsonRequest")}
                value={exchange.request}
                defaultExpandDepth={1}
                expandAllLabel={t("inspector.timelineJsonExpandAll")}
                collapseAllLabel={t("inspector.timelineJsonCollapse")}
              />
              <JsonTree
                key={`${exchange.id}-res`}
                title={t("inspector.timelineJsonResponse")}
                value={exchange.response}
                defaultExpandDepth={1}
                expandAllLabel={t("inspector.timelineJsonExpandAll")}
                collapseAllLabel={t("inspector.timelineJsonCollapse")}
              />
            </>
          ) : (
            <>
              <section className="rounded border border-sky-100 bg-sky-50/50 px-2 py-1.5">
                <div className="mb-0.5 flex items-center gap-1">
                  <User className="h-3 w-3 text-sky-800" />
                  <span className="text-[10px] font-semibold text-sky-900">
                    {t("inspector.timelineRequestBody")}
                  </span>
                </div>
                <pre className="text-foreground max-h-56 overflow-auto whitespace-pre-wrap break-words font-mono text-[10px] leading-relaxed">
                  {readableRequestBody(exchange)}
                </pre>
              </section>
              <section className="rounded border border-violet-100 bg-violet-50/40 px-2 py-1.5">
                <div className="mb-0.5 flex items-center gap-1">
                  <Bot className="h-3 w-3 text-violet-800" />
                  <span className="text-[10px] font-semibold text-violet-900">
                    {t("inspector.timelineResponseBody")}
                  </span>
                </div>
                <pre className="text-foreground max-h-56 overflow-auto whitespace-pre-wrap break-words font-mono text-[10px] leading-relaxed">
                  {readableResponseBody(exchange) || t("inspector.timelineResponseEmpty")}
                </pre>
              </section>
            </>
          )}
        </div>
      ) : null}
    </div>
  );
}

export function TimelinePanel({ threadId, activeRunId }: TimelinePanelProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [jsonMode, setJsonMode] = useState(false);

  const runsQuery = useQuery({
    queryKey: ["inspector-timeline-runs", threadId ?? "none"],
    queryFn: () => listRuns(threadId!),
    enabled: !!threadId,
    refetchInterval: 4_000,
  });

  const messagesQuery = useQuery({
    queryKey: ["inspector-timeline-messages", threadId ?? "none"],
    queryFn: () => listMessages(threadId!),
    enabled: !!threadId,
    refetchInterval: 4_000,
  });

  const runs = useMemo(() => {
    // One card per user-facing API turn (runs that have a User message).
    const userRunIds = new Set(
      (messagesQuery.data ?? [])
        .filter((m) => m.role === "User" && m.run_id)
        .map((m) => m.run_id as string),
    );
    const list = [...(runsQuery.data ?? [])].filter((run) => {
      if (userRunIds.size === 0) return true;
      return userRunIds.has(run.id);
    });
    list.sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at));
    return list.slice(0, MAX_TURNS);
  }, [runsQuery.data, messagesQuery.data]);

  const eventsQueries = useQueries({
    queries: runs.map((run) => ({
      queryKey: ["inspector-timeline-events", run.id] as const,
      queryFn: () => listRunEvents(run.id),
      staleTime: 3_000,
      refetchInterval: run.status === "Running" || run.status === "Queued" ? 2_000 : false,
    })),
  });

  const usageQueries = useQueries({
    queries: runs.map((run) => ({
      queryKey: ["inspector-timeline-usage", run.id] as const,
      queryFn: () => getRunTokenUsage(run.id),
      staleTime: 5_000,
      refetchInterval: run.status === "Running" || run.status === "Queued" ? 2_500 : false,
    })),
  });

  const turns: ApiTurn[] = useMemo(() => {
    const messages = messagesQuery.data ?? [];
    return runs.map((run, index) => {
      const events = eventsQueries[index]?.data ?? [];
      return buildTurn(run, messages, events);
    });
  }, [runs, eventsQueries, messagesQuery.data]);

  if (!threadId) {
    return <EmptyState message={t("inspector.openThreadTimeline")} />;
  }
  if (runsQuery.isLoading || messagesQuery.isLoading) {
    return <PanelLoading />;
  }
  if (runsQuery.error) {
    return (
      <InlineError title={t("inspector.loadTimelineFailed")} message={runsQuery.error.message} />
    );
  }
  if (!turns.length) {
    return <EmptyState message={t("inspector.timelineEmpty")} />;
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="border-b px-3 py-2">
        <p className="text-muted-foreground text-[11px] leading-relaxed">
          {t("inspector.timelineHint")}
        </p>
        <div className="mt-1.5 flex items-center gap-2">
          <button
            type="button"
            className={cn(
              "inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-[11px] font-medium transition-colors",
              jsonMode
                ? "border-violet-300 bg-violet-50 text-violet-900"
                : "border-border bg-background text-muted-foreground hover:bg-muted/50",
            )}
            onClick={() => setJsonMode((v) => !v)}
            aria-pressed={jsonMode}
            title={t("inspector.timelineJsonModeHint")}
          >
            <Braces className="h-3 w-3" />
            {t("inspector.timelineJsonMode")}
          </button>
          <span className="text-muted-foreground text-[10px]">
            {jsonMode ? t("inspector.timelineJsonModeOn") : t("inspector.timelineJsonModeOff")}
          </span>
        </div>
      </div>
      <ol className="flex-1 space-y-0 overflow-y-auto px-3 py-3">
        {turns.map((turn, index) => {
          const runId = turn.run.id;
          const isOpen = expanded[runId] ?? (activeRunId === runId || index === 0);
          const isActive = activeRunId === runId;
          const usage = usageQueries[index]?.data;
          const n = turns.length - index;
          const llmSteps = turn.exchanges.filter((e) => e.kind === "llm").length;
          const toolSteps = turn.exchanges.filter((e) => e.kind === "tool").length;

          return (
            <li key={runId} className="relative flex gap-2 pb-4 last:pb-1">
              <div className="flex w-4 shrink-0 flex-col items-center">
                <span
                  className={cn(
                    "mt-1.5 h-2.5 w-2.5 rounded-full border-2 border-background ring-2",
                    turn.run.status === "Completed" && "bg-emerald-500 ring-emerald-200",
                    (turn.run.status === "Failed" ||
                      turn.run.status === "Cancelled" ||
                      turn.run.status === "Interrupted") &&
                      "bg-red-500 ring-red-200",
                    (turn.run.status === "Running" || turn.run.status === "Queued") &&
                      "bg-sky-500 ring-sky-200",
                    turn.run.status === "WaitingApproval" && "bg-amber-500 ring-amber-200",
                    turn.run.status === "Paused" && "bg-muted-foreground ring-border",
                  )}
                  aria-hidden
                />
                {index < turns.length - 1 ? (
                  <span className="bg-border mt-1 w-px min-h-[12px] flex-1" aria-hidden />
                ) : null}
              </div>

              <div
                className={cn(
                  "bg-card min-w-0 flex-1 rounded-lg border shadow-xs",
                  isActive && "ring-ring/40 ring-2",
                )}
              >
                <button
                  type="button"
                  className="hover:bg-muted/40 flex w-full items-start gap-2 px-2.5 py-2 text-left transition-colors"
                  onClick={() =>
                    setExpanded((prev) => ({
                      ...prev,
                      [runId]: !isOpen,
                    }))
                  }
                  aria-expanded={isOpen}
                >
                  <span className="text-muted-foreground mt-0.5">
                    {isOpen ? (
                      <ChevronDown className="h-3.5 w-3.5" />
                    ) : (
                      <ChevronRight className="h-3.5 w-3.5" />
                    )}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <StatusIcon status={turn.run.status} />
                      <span className="text-xs font-semibold">
                        {t("inspector.timelineTurn").replace("{n}", String(n))}
                      </span>
                      <span
                        className={cn(
                          "rounded-full border px-1.5 py-px text-[10px] font-medium",
                          statusTone(turn.run.status),
                        )}
                      >
                        {statusLabel(turn.run.status, t)}
                      </span>
                      <span
                        className="text-muted-foreground bg-muted/40 rounded-full border px-1.5 py-px text-[10px] font-medium tabular-nums"
                        title={t("inspector.timelineDuration")}
                      >
                        ⏱ {formatDuration(turn.run)}
                      </span>
                      <span
                        className="text-muted-foreground bg-muted/40 rounded-full border px-1.5 py-px text-[10px] font-medium tabular-nums"
                        title={
                          usage && usage.inputTokens + usage.outputTokens > 0
                            ? `${t("inspector.timelineTokens")}: ↑${usage.inputTokens.toLocaleString()} ↓${usage.outputTokens.toLocaleString()}`
                            : t("inspector.timelineTokens")
                        }
                      >
                        {usage
                          ? formatTokens(usage.inputTokens, usage.outputTokens)
                          : usageQueries[index]?.isLoading
                            ? t("inspector.timelineTokensLoading")
                            : "—"}
                      </span>
                      {turn.exchanges.length > 0 ? (
                        <span className="text-muted-foreground bg-muted/40 rounded-full border px-1.5 py-px text-[10px]">
                          {t("inspector.timelineExchangeSummary")
                            .replace("{llm}", String(llmSteps))
                            .replace("{tool}", String(toolSteps))}
                        </span>
                      ) : null}
                      <span className="text-muted-foreground ml-auto text-[10px] tabular-nums">
                        {formatClock(turn.requestAt)}
                      </span>
                    </div>
                    <p className="text-foreground mt-1 line-clamp-2 text-[11px] leading-snug">
                      <span className="text-muted-foreground font-medium">
                        {t("inspector.timelineYou")}:{" "}
                      </span>
                      {truncate(turn.request, 100)}
                    </p>
                    {!isOpen && turn.response ? (
                      <p className="text-muted-foreground mt-0.5 line-clamp-1 text-[11px] leading-snug">
                        <span className="font-medium">{t("inspector.timelineAi")}: </span>
                        {truncate(turn.response, 90)}
                      </p>
                    ) : null}
                  </div>
                </button>

                {isOpen ? (
                  <div className="space-y-2.5 border-t px-2.5 py-2.5">
                    {/* Turn-level summary (user message + final reply) */}
                    <section className="rounded-md border border-sky-100 bg-sky-50/50 px-2.5 py-2">
                      <div className="mb-1 flex items-center gap-1.5">
                        <User className="h-3.5 w-3.5 text-sky-800" />
                        <span className="text-[11px] font-semibold text-sky-900">
                          {t("inspector.timelineRequestBody")}
                        </span>
                        <span className="text-muted-foreground ml-auto text-[10px] tabular-nums">
                          {formatClock(turn.requestAt)}
                        </span>
                      </div>
                      {jsonMode ? (
                        <JsonTree
                          key={`${runId}-user`}
                          value={{ role: "user", content: turn.request }}
                          defaultExpandDepth={1}
                          maxHeightClassName="max-h-48"
                          expandAllLabel={t("inspector.timelineJsonExpandAll")}
                          collapseAllLabel={t("inspector.timelineJsonCollapse")}
                        />
                      ) : (
                        <p className="text-foreground whitespace-pre-wrap break-words text-[11px] leading-relaxed">
                          {turn.request}
                        </p>
                      )}
                    </section>

                    <section className="rounded-md border border-violet-100 bg-violet-50/40 px-2.5 py-2">
                      <div className="mb-1 flex items-center gap-1.5">
                        <Bot className="h-3.5 w-3.5 text-violet-800" />
                        <span className="text-[11px] font-semibold text-violet-900">
                          {t("inspector.timelineResponseBody")}
                        </span>
                        {turn.responseAt ? (
                          <span className="text-muted-foreground ml-auto text-[10px] tabular-nums">
                            {formatClock(turn.responseAt)}
                          </span>
                        ) : null}
                      </div>
                      {jsonMode ? (
                        <JsonTree
                          key={`${runId}-assistant`}
                          value={{
                            role: "assistant",
                            content: turn.response || null,
                            status: turn.run.status,
                          }}
                          defaultExpandDepth={1}
                          maxHeightClassName="max-h-48"
                          expandAllLabel={t("inspector.timelineJsonExpandAll")}
                          collapseAllLabel={t("inspector.timelineJsonCollapse")}
                        />
                      ) : turn.response ? (
                        <p className="text-foreground whitespace-pre-wrap break-words text-[11px] leading-relaxed">
                          {truncate(turn.response, 2_400)}
                        </p>
                      ) : turn.run.status === "Running" || turn.run.status === "Queued" ? (
                        <p className="text-muted-foreground text-[11px] italic">
                          {t("inspector.timelineResponsePending")}
                        </p>
                      ) : (
                        <p className="text-muted-foreground text-[11px]">
                          {t("inspector.timelineResponseEmpty")}
                        </p>
                      )}
                    </section>

                    {turn.errorText ? (
                      <section className="rounded-md border border-red-200 bg-red-50/60 px-2.5 py-2">
                        <div className="mb-1 flex items-center gap-1.5">
                          <CircleAlert className="h-3.5 w-3.5 text-red-700" />
                          <span className="text-[11px] font-semibold text-red-800">
                            {t("inspector.timelineStepError")}
                          </span>
                        </div>
                        <p className="whitespace-pre-wrap break-words text-[11px] leading-relaxed text-red-900/90">
                          {truncate(turn.errorText, 800)}
                        </p>
                      </section>
                    ) : null}

                    {/* Fine-grained exchanges */}
                    {turn.exchanges.length > 0 ? (
                      <div className="space-y-1.5">
                        <div className="text-muted-foreground text-[10px] font-semibold tracking-wide uppercase">
                          {t("inspector.timelineExchanges")}
                          <span className="ml-1 font-normal normal-case">
                            ({turn.exchanges.length})
                          </span>
                        </div>
                        <div className="space-y-1.5">
                          {turn.exchanges.map((exchange, ei) => (
                            <ExchangeCard
                              key={exchange.id}
                              exchange={exchange}
                              index={ei}
                              jsonMode={jsonMode}
                              t={t}
                            />
                          ))}
                        </div>
                      </div>
                    ) : null}

                    {/* Metrics row */}
                    <div className="text-muted-foreground flex flex-wrap gap-x-3 gap-y-1 border-t pt-2 text-[10px] tabular-nums">
                      <span>
                        {t("inspector.timelineDuration")}:{" "}
                        <strong className="text-foreground font-medium">
                          {formatDuration(turn.run)}
                        </strong>
                      </span>
                      <span>
                        {t("inspector.timelineTokens")}:{" "}
                        <strong className="text-foreground font-medium">
                          {usage
                            ? formatTokens(usage.inputTokens, usage.outputTokens)
                            : usageQueries[index]?.isLoading
                              ? t("inspector.timelineTokensLoading")
                              : "—"}
                        </strong>
                      </span>
                    </div>
                  </div>
                ) : null}
              </div>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
