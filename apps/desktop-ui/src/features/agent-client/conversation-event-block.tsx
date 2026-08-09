import { useMemo, useState } from "react";
import {
  AlertCircle,
  Bot,
  CheckCircle2,
  Code2,
  FileBox,
  HelpCircle,
  Loader2,
  RotateCcw,
  ShieldAlert,
  User,
} from "lucide-react";
import { MarkdownBody } from "@/components/markdown/markdown-body";
import { Button } from "@/components/ui/button";
import { formatDateTime } from "@/lib/formatters";
import { useTranslation } from "@/lib/i18n-react";
import { cn } from "@/lib/utils";
import type { AgentRunId, ArtifactPreview, WorkspaceId } from "@/lib/schemas";
import { previewWorkspaceMarkdown } from "@/lib/tauri-api";
import { MarkdownPreviewDialog } from "@/features/markdown-provider/markdown-preview-dialog";
import type {
  ConversationBlock,
  ConversationBlockKind,
  ConversationBlockTone,
} from "./event-view-models";
import { MessageFeedback } from "@/features/memory/message-feedback";
import { MessageFileChanges } from "./message-file-changes";
import { isPreviewablePath } from "./message-file-refs";

interface ConversationEventBlockProps {
  block: ConversationBlock;
  workspaceId?: WorkspaceId;
  /** Original user text for this turn (shown above errors; used by Retry). */
  userPrompt?: string | null;
  onRetry?: (content: string) => void;
  retryDisabled?: boolean;
  /** Active turn: soft pulse background while the agent is working. */
  isRunning?: boolean;
  /** Canvas deep-link: highlight this message block. */
  isHighlighted?: boolean;
  /** Terminal run — show feedback under assistant message. */
  runIsTerminal?: boolean;
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
  if (block.kind === "message") return true;
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
  workspaceId,
  userPrompt,
  onRetry,
  retryDisabled = false,
  isRunning = false,
  isHighlighted = false,
  runIsTerminal = false,
}: ConversationEventBlockProps) {
  const { t } = useTranslation();
  const [inlinePreview, setInlinePreview] = useState<ArtifactPreview | null>(null);
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
  const isUserBubble = block.kind === "message" && block.role === "user";
  const isAssistantBubble = block.kind === "message" && block.role === "assistant";
  const showRetry =
    (block.kind === "error" || block.tone === "danger") &&
    Boolean(userPrompt?.trim()) &&
    typeof onRetry === "function";
  const messageId = block.id.startsWith("message-")
    ? block.id.slice("message-".length)
    : undefined;
  const runId = block.raw.run_id;

  const openInlinePath = (path: string) => {
    if (!workspaceId || !isPreviewablePath(path)) return;
    void previewWorkspaceMarkdown(workspaceId, path, null)
      .then(setInlinePreview)
      .catch(() => undefined);
  };

  // Chat bubbles: user right (compact), assistant left (use full column width
  // so tables / long deliverables are not squeezed on wide screens).
  if (isUserBubble || isAssistantBubble) {
    return (
      <div
        className={cn(
          "flex w-full min-w-0",
          isUserBubble ? "justify-end" : "justify-start",
        )}
      >
        <article
          id={messageId ? `msg-${messageId}` : undefined}
          data-message-id={messageId}
          className={cn(
            "min-w-0 rounded-2xl border px-3.5 py-2.5 text-sm shadow-xs transition-colors",
            // User: short prompts stay bubble-sized; grow on large screens a bit.
            isUserBubble &&
              "max-w-[min(100%,28rem)] sm:max-w-[min(100%,32rem)] lg:max-w-[min(100%,40rem)] border-user-bubble-border bg-user-bubble rounded-br-md",
            // Assistant: fill the conversation column (tables / docs need width).
            isAssistantBubble &&
              "w-full max-w-full border-border bg-muted/40 rounded-bl-md",
            isRunning && "conversation-block-running",
            isHighlighted && "ring-primary ring-2 ring-offset-2",
          )}
          data-running={isRunning ? "true" : undefined}
        >
          {asMarkdown ? (
            <MarkdownBody
              content={displayBody}
              compact
              className="!m-0 !border-0 !bg-transparent !p-0 shadow-none"
              onOpenFilePath={
                isAssistantBubble && workspaceId ? openInlinePath : undefined
              }
            />
          ) : (
            <p className="leading-6 whitespace-pre-wrap">{displayBody}</p>
          )}
          {isAssistantBubble && workspaceId && !isRunning ? (
            <MessageFileChanges
              workspaceId={workspaceId}
              runId={runId}
              messageText={displayBody}
            />
          ) : null}
          {isAssistantBubble &&
          runIsTerminal &&
          !isRunning &&
          runId &&
          runId !== ("unknown" as AgentRunId) ? (
            <MessageFeedback runId={runId as AgentRunId} />
          ) : null}
          <div
            className={cn(
              "text-muted-foreground mt-1.5 flex items-center gap-1.5 text-[11px]",
              isUserBubble ? "justify-end" : "justify-start",
            )}
          >
            {isUserBubble ? (
              <User className="h-3 w-3 shrink-0" aria-hidden />
            ) : (
              <Bot className="h-3 w-3 shrink-0" aria-hidden />
            )}
            <span className="font-medium">
              {isUserBubble ? t("agent.you") : t("agent.assistant")}
            </span>
            {isRunning ? (
              <>
                <span aria-hidden>·</span>
                <span className="conversation-running-pill inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium">
                  <span className="conversation-running-dot" aria-hidden />
                  {t("agent.runningPill")}
                </span>
              </>
            ) : null}
            <span aria-hidden>·</span>
            <span className="shrink-0">{formatDateTime(block.createdAt)}</span>
          </div>
          {inlinePreview ? (
            <MarkdownPreviewDialog
              preview={inlinePreview}
              onClose={() => setInlinePreview(null)}
            />
          ) : null}
        </article>
      </div>
    );
  }

  // Tool / error / status / diagnostic: full-width cards (not chat bubbles).
  return (
    <article
      id={messageId ? `msg-${messageId}` : undefined}
      data-message-id={messageId}
      className={cn(
        "w-full min-w-0 rounded-lg border text-sm shadow-xs transition-colors",
        toneClasses(block.tone),
        isRunning && "conversation-block-running",
        isHighlighted && "ring-primary ring-2 ring-offset-2",
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
        <MarkdownBody content={displayBody} compact className="!border-0" />
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
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onRetry?.(userPrompt);
            }}
          >
            {retryDisabled ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RotateCcw className="h-3.5 w-3.5" />
            )}
            {retryDisabled ? t("agent.retrying") : t("agent.retry")}
          </Button>
        </div>
      ) : null}
    </article>
  );
}
