import { useMutation } from "@tanstack/react-query";
import { Pause, Play, RotateCcw, Square, ThumbsDown, ThumbsUp } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import type { AgentRunId, AgentRunStatus } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { submitRunFeedback } from "@/lib/tauri-api";

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
  const [feedbackSent, setFeedbackSent] = useState<"Helpful" | "NotHelpful" | null>(null);

  const feedback = useMutation({
    mutationFn: (rating: "Helpful" | "NotHelpful") => {
      if (!runId) throw new Error("no run");
      return submitRunFeedback(runId, rating);
    },
    onSuccess: (_data, rating) => setFeedbackSent(rating),
  });

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
      {isTerminal && (
        <>
          <Button
            variant={feedbackSent === "Helpful" ? "default" : "outline"}
            size="sm"
            disabled={feedback.isPending || feedbackSent !== null}
            onClick={() => feedback.mutate("Helpful")}
            title={t("run.feedback.helpful")}
          >
            <ThumbsUp className="h-3.5 w-3.5" />
            {t("run.feedback.helpful")}
          </Button>
          <Button
            variant={feedbackSent === "NotHelpful" ? "default" : "outline"}
            size="sm"
            disabled={feedback.isPending || feedbackSent !== null}
            onClick={() => feedback.mutate("NotHelpful")}
            title={t("run.feedback.notHelpful")}
          >
            <ThumbsDown className="h-3.5 w-3.5" />
            {t("run.feedback.notHelpful")}
          </Button>
        </>
      )}
    </div>
  );
}
