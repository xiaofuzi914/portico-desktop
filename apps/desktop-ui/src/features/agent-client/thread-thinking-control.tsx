import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Brain, Gauge } from "lucide-react";
import { listModels, listProviders, resolveActiveModel } from "@/lib/tauri-api";
import { modelKeys, providerKeys } from "@/lib/query-keys";
import type { ReasoningEffort, ThinkingMode, ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { cn } from "@/lib/utils";
import {
  loadThinkingPrefs,
  resolveThinkingControl,
  saveReasoningEffort,
  saveThinkingMode,
  type ThinkingControlState,
} from "./model-thinking-prefs";

interface ThreadThinkingControlProps {
  workspaceId: WorkspaceId;
  threadId: ThreadId;
  className?: string;
  /** Notify parent when control state changes (for send payload). */
  onChange?: (state: ThinkingControlState) => void;
}

const THINKING_OPTIONS: { value: ThinkingMode; labelKey: string }[] = [
  { value: "off", labelKey: "agent.thinkingOff" },
  { value: "on", labelKey: "agent.thinkingOn" },
  { value: "auto", labelKey: "agent.thinkingAuto" },
];

const EFFORT_OPTIONS: { value: ReasoningEffort; labelKey: string }[] = [
  { value: "low", labelKey: "agent.effortLow" },
  { value: "medium", labelKey: "agent.effortMedium" },
  { value: "high", labelKey: "agent.effortHigh" },
];

export function ThreadThinkingControl({
  workspaceId,
  threadId,
  className,
  onChange,
}: ThreadThinkingControlProps) {
  const { t } = useTranslation();
  const [prefs, setPrefs] = useState(loadThinkingPrefs);

  const { data: providers = [] } = useQuery({
    queryKey: providerKeys.list(),
    queryFn: listProviders,
  });
  const { data: models = [] } = useQuery({
    queryKey: modelKeys.list(),
    queryFn: () => listModels(),
  });
  const { data: active } = useQuery({
    queryKey: ["active-model", "resolved", workspaceId, threadId],
    queryFn: () => resolveActiveModel(workspaceId, threadId),
    retry: false,
  });

  const activeModel = useMemo(
    () => models.find((m) => m.id === active?.model_id) ?? null,
    [models, active?.model_id],
  );

  const control = useMemo(
    () => resolveThinkingControl(activeModel, providers, prefs),
    [activeModel, providers, prefs],
  );

  useEffect(() => {
    onChange?.(control);
  }, [control, onChange]);

  if (control.kind === "none") {
    return null;
  }

  if (control.kind === "deepseek_toggle") {
    return (
      <label
        className={cn(
          "border-border bg-muted/40 text-muted-foreground hover:bg-muted/70 inline-flex h-8 max-w-full items-center gap-1.5 rounded-lg border px-2 text-xs",
          className,
        )}
        title={t("agent.thinkingHint")}
      >
        <Brain className="h-3.5 w-3.5 shrink-0 text-indigo-600 dark:text-indigo-400" aria-hidden />
        <span className="sr-only">{t("agent.thinkingLabel")}</span>
        <select
          aria-label={t("agent.thinkingLabel")}
          className="text-foreground max-w-[6.5rem] cursor-pointer bg-transparent font-medium outline-none"
          value={control.thinkingMode}
          onChange={(e) => {
            const mode = e.target.value as ThinkingMode;
            saveThinkingMode(mode);
            setPrefs((p) => ({ ...p, thinkingMode: mode }));
          }}
        >
          {THINKING_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {t(opt.labelKey)}
            </option>
          ))}
        </select>
      </label>
    );
  }

  // reasoning_effort
  return (
    <label
      className={cn(
        "border-border bg-muted/40 text-muted-foreground hover:bg-muted/70 inline-flex h-8 max-w-full items-center gap-1.5 rounded-lg border px-2 text-xs",
        className,
      )}
      title={t("agent.effortHint")}
    >
      <Gauge className="h-3.5 w-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />
      <span className="sr-only">{t("agent.effortLabel")}</span>
      <select
        aria-label={t("agent.effortLabel")}
        className="text-foreground max-w-[5.5rem] cursor-pointer bg-transparent font-medium outline-none"
        value={control.reasoningEffort}
        onChange={(e) => {
          const effort = e.target.value as ReasoningEffort;
          saveReasoningEffort(effort);
          setPrefs((p) => ({ ...p, reasoningEffort: effort }));
        }}
      >
        {EFFORT_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {t(opt.labelKey)}
          </option>
        ))}
      </select>
    </label>
  );
}
