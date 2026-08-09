import { useMutation } from "@tanstack/react-query";
import { ThumbsDown, ThumbsUp } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import type { AgentRunId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { submitRunFeedback } from "@/lib/tauri-api";
import { cn } from "@/lib/utils";

interface MessageFeedbackProps {
  runId: AgentRunId;
  /** Optional learning summary line, e.g. "使用 3 条经验". */
  experienceSummary?: string | null;
  onOpenExperience?: () => void;
}

/**
 * Lightweight thumbs feedback for terminal assistant turns.
 * Only render after the run is finished.
 */
export function MessageFeedback({
  runId,
  experienceSummary,
  onOpenExperience,
}: MessageFeedbackProps) {
  const { t } = useTranslation();
  const [rating, setRating] = useState<"Helpful" | "NotHelpful" | null>(null);

  const mutation = useMutation({
    mutationFn: (next: "Helpful" | "NotHelpful") => submitRunFeedback(runId, next),
    onSuccess: (_d, next) => setRating(next),
  });

  return (
    <div className="mt-2 flex flex-wrap items-center gap-2 border-t border-current/10 pt-2">
      <Button
        type="button"
        size="sm"
        variant={rating === "Helpful" ? "default" : "outline"}
        className="h-7 px-2 text-[11px]"
        aria-pressed={rating === "Helpful"}
        disabled={mutation.isPending}
        onClick={() => mutation.mutate("Helpful")}
      >
        <ThumbsUp className="h-3 w-3" />
        {t("run.feedback.helpful")}
      </Button>
      <Button
        type="button"
        size="sm"
        variant={rating === "NotHelpful" ? "default" : "outline"}
        className="h-7 px-2 text-[11px]"
        aria-pressed={rating === "NotHelpful"}
        disabled={mutation.isPending}
        onClick={() => mutation.mutate("NotHelpful")}
      >
        <ThumbsDown className="h-3 w-3" />
        {t("run.feedback.notHelpful")}
      </Button>
      {experienceSummary ? (
        <button
          type="button"
          className={cn(
            "text-muted-foreground hover:text-foreground text-[11px] underline-offset-2 hover:underline",
          )}
          onClick={onOpenExperience}
        >
          {experienceSummary}
        </button>
      ) : null}
    </div>
  );
}
