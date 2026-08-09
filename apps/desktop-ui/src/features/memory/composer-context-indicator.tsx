import { useQuery } from "@tanstack/react-query";
import { ChevronDown, Sparkles } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "@/lib/i18n-react";
import { learningKeys } from "@/lib/query-keys";
import type { ThreadId, WorkspaceId } from "@/lib/schemas";
import {
  getLearningOverview,
  listMemories,
  recallWorkflowPatterns,
} from "@/lib/tauri-api";
import { cn } from "@/lib/utils";

interface ComposerContextIndicatorProps {
  workspaceId?: WorkspaceId;
  threadId?: ThreadId;
  draft: string;
}

/**
 * Pre-send hint: how many preferences / patterns may condition the next turn.
 * Not the authoritative snapshot — Inspector shows the real run snapshot.
 */
export function ComposerContextIndicator({
  workspaceId,
  threadId,
  draft,
}: ComposerContextIndicatorProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const overviewQuery = useQuery({
    queryKey: learningKeys.overview(),
    queryFn: getLearningOverview,
    staleTime: 60_000,
  });

  const memoriesQuery = useQuery({
    queryKey: ["memories", "preview", workspaceId, threadId],
    queryFn: async () => {
      const user = await listMemories("User", null, null);
      const ws = workspaceId
        ? await listMemories("Workspace", workspaceId, null)
        : [];
      return [...user, ...ws].filter((m) => !m.sensitive).slice(0, 5);
    },
    staleTime: 30_000,
  });

  const patternsQuery = useQuery({
    queryKey: ["pattern-preview", workspaceId, draft.slice(0, 80)],
    queryFn: () =>
      recallWorkflowPatterns(draft.trim() || "general task", workspaceId ?? null),
    enabled: draft.trim().length > 8,
    staleTime: 15_000,
  });

  const prefCount =
    memoriesQuery.data?.length ?? overviewQuery.data?.confirmed_preferences ?? 0;
  const patternCount = patternsQuery.data?.length ?? 0;

  return (
    <div className="text-muted-foreground border-border/60 border-t px-1 pt-1.5 text-[11px]">
      <button
        type="button"
        className="hover:text-foreground flex w-full items-center gap-1.5 text-left"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <Sparkles className="h-3 w-3 shrink-0" />
        <span className="min-w-0 flex-1 truncate">
          {t("memory.center.composerHint")} · {prefCount} {t("memory.center.stat.confirmed")}
          {patternCount > 0 ? ` · ${patternCount} patterns` : ""}
        </span>
        <ChevronDown className={cn("h-3 w-3 transition-transform", open && "rotate-180")} />
      </button>
      {open && (
        <div className="bg-muted/40 mt-1.5 space-y-1 rounded-md border p-2">
          <p className="text-[10px] font-medium">{t("memory.center.composerPreviewNote")}</p>
          {(memoriesQuery.data ?? []).slice(0, 3).map((m) => (
            <p key={m.id} className="truncate">
              · {m.value}
            </p>
          ))}
          {(patternsQuery.data ?? []).slice(0, 2).map((p) => (
            <p key={p.id} className="truncate">
              · pattern: {p.name}
            </p>
          ))}
          {(memoriesQuery.data?.length ?? 0) === 0 && patternCount === 0 && (
            <p>{t("memory.center.composerEmpty")}</p>
          )}
        </div>
      )}
    </div>
  );
}
