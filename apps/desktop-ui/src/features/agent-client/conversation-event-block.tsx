import { useMemo } from "react";
import {
  AlertCircle,
  Bot,
  CheckCircle2,
  Code2,
  FileBox,
  HelpCircle,
  RotateCcw,
  ShieldAlert,
  User,
} from "lucide-react";
import { MarkdownBody } from "@/components/markdown/markdown-body";
import { Button } from "@/components/ui/button";
import { formatDateTime } from "@/lib/formatters";
import { useTranslation } from "@/lib/i18n-react";
import { cn } from "@/lib/utils";
import type {
  ConversationBlock,
  ConversationBlockKind,
  ConversationBlockTone,
} from "./event-view-models";

interface ConversationEventBlockProps {
  block: ConversationBlock;
  /** Original user text for this turn (shown above errors; used by Retry). */
  userPrompt?: string | null;
  onRetry?: (content: string) => void;
  retryDisabled?: boolean;
  /** Active turn: soft pulse background while the agent is working. */
  isRunning?: boolean;
}

function toneClasses(tone: ConversationBlockTone): string {
  switch (tone) {
    case "success":
      return "border-success/30 bg-success/8 text-foreground";
    case "warning":
      return "border-warning/40 bg-warning/10 text-foreground";
    case "danger":
      return "border-destructive/35 bg-destructive/8 text-foreground";
    case "muted":
      return "border-border bg-muted/60 text-foreground";
    case "default":
    default:
      return "border-border bg-background text-foreground";
  }
}

function iconForKind(kind: ConversationBlockKind, title: string) {
  if (kind === "message" && ["user", "you"].includes(title.toLowerCase())) return User;
  if (kind === "message") return Bot;
  if (kind === "tool") return Code2;
  if (kind === "approval") return ShieldAlert;
  if (kind === "artifact") return FileBox;
  if (kind === "status") return CheckCircle2;
  if (kind === "error") return AlertCircle;
  return HelpCircle;
}

function shouldRenderMarkdown(block: ConversationBlock): boolean {
  // Assistant / user chat messages always go through MD.
  if (block.kind === "message") return true;
  // System / orchestration summaries often arrive as markdown-ish prose.
  if (block.kind === "error" || block.kind === "status" || block.kind === "diagnostic") {
    return looksLikeMarkdown(block.body);
  }
  return false;
}

function looksLikeMarkdown(text: string): boolean {
  return /(^|\n)\s{0,3}(#{1,6}\s|[-*+]\s|\d+\.\s|>\s|```|`[^`]+`|\*\*[^*]+\*\*|__[^_]+__)/m.test(
    text,
  );
}

export function ConversationEventBlock({
  block,
  userPrompt,
  onRetry,
  retryDisabled = false,
  isRunning = false,
}: ConversationEventBlockProps) {
  const { t } = useTranslation();
  const displayBody = useMemo(() => {
    if (block.kind === "tool") {
      try {
        const parsed = JSON.parse(block.body);
        return JSON.stringify(parsed, null, 2);
      } catch {
        return block.body;
      }
    }
    return block.body;
  }, [block]);
  const Icon = iconForKind(block.kind, block.title);
  const asMarkdown = shouldRenderMarkdown(block);
  const showRetry =
    (block.kind === "error" || block.tone === "danger") &&
    Boolean(userPrompt?.trim()) &&
    typeof onRetry === "function";

  return (
    <article
      className={cn(
        "rounded-lg border text-sm shadow-xs transition-colors",
        toneClasses(block.tone),
        isRunning && "conversation-block-running",
      )}
      data-running={isRunning ? "true" : undefined}
    >
      <div className="flex items-center justify-between gap-3 border-b border-current/10 px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <Icon className="h-4 w-4 shrink-0 opacity-75" />
          <span className="truncate font-medium">{block.title}</span>
          <span className="text-muted-foreground rounded border px-1.5 py-0.5 text-[10px] uppercase">
            {block.kind}
          </span>
          {isRunning ? (
            <span className="conversation-running-pill inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium">
              <span className="conversation-running-dot" aria-hidden />
              {t("agent.runningPill")}
            </span>
          ) : null}
        </div>
        <span className="text-muted-foreground shrink-0 text-xs">
          {formatDateTime(block.createdAt)}
        </span>
      </div>
      {showRetry && userPrompt ? (
        <div className="border-b border-current/10 bg-background/40 px-3 py-2">
          <p className="text-muted-foreground text-[11px] font-medium tracking-wide uppercase">
            {t("agent.yourRequest")}
          </p>
          <p className="text-foreground mt-1 text-sm leading-6 whitespace-pre-wrap">{userPrompt}</p>
        </div>
      ) : null}
      {asMarkdown ? (
        <MarkdownBody content={displayBody} compact />
      ) : (
        <pre className="text-foreground max-h-96 overflow-auto p-3 font-mono text-xs leading-5 whitespace-pre-wrap">
          {displayBody}
        </pre>
      )}
      {showRetry && userPrompt ? (
        <div className="flex justify-end border-t border-current/10 px-3 py-2">
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-8 gap-1.5 text-xs"
            disabled={retryDisabled}
            onClick={() => onRetry?.(userPrompt)}
          >
            <RotateCcw className="h-3.5 w-3.5" />
            {t("agent.retry")}
          </Button>
        </div>
      ) : null}
    </article>
  );
}
