import { Pause, Play, RotateCcw, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { AgentRunId, AgentRunStatus } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

interface RunControlsProps {
  runId?: AgentRunId;
  status?: AgentRunStatus;
  onStartRun: () => void;
  onCancel: () => void;
  onPause: () => void;
  onResume: () => void;
  isPending: boolean;
}

export function RunControls({
  runId,
  status,
  onStartRun,
  onCancel,
  onPause,
  onResume,
  isPending,
}: RunControlsProps) {
  const { t } = useTranslation();

  if (!runId) {
    return (
      <Button onClick={onStartRun} disabled={isPending} variant="outline" size="sm">
        <Play className="h-4 w-4" />
        {isPending ? t("agent.starting") : t("agent.startRun")}
      </Button>
    );
  }

  const isTerminal = status === "Completed" || status === "Failed" || status === "Cancelled";

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button variant="destructive" size="sm" onClick={onCancel} disabled={isPending || isTerminal}>
        <Square className="h-3.5 w-3.5" />
        {t("agent.stop")}
      </Button>
      <Button
        variant="outline"
        size="sm"
        onClick={onPause}
        disabled={isPending || status !== "Running"}
      >
        <Pause className="h-3.5 w-3.5" />
        {t("agent.pause")}
      </Button>
      <Button
        variant="outline"
        size="sm"
        onClick={onResume}
        disabled={isPending || status !== "Paused"}
      >
        <RotateCcw className="h-3.5 w-3.5" />
        {t("agent.resume")}
      </Button>
    </div>
  );
}
