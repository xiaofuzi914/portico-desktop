import { useMemo } from "react";
import {
  AlertCircle,
  Bot,
  CheckCircle2,
  Code2,
  FileBox,
  HelpCircle,
  ShieldAlert,
  User,
} from "lucide-react";
import { formatDateTime } from "@/lib/formatters";
import { cn } from "@/lib/utils";
import type {
  ConversationBlock,
  ConversationBlockKind,
  ConversationBlockTone,
} from "./event-view-models";

interface ConversationEventBlockProps {
  block: ConversationBlock;
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
  if (kind === "message" && title.toLowerCase() === "user") return User;
  if (kind === "message") return Bot;
  if (kind === "tool") return Code2;
  if (kind === "approval") return ShieldAlert;
  if (kind === "artifact") return FileBox;
  if (kind === "status") return CheckCircle2;
  if (kind === "error") return AlertCircle;
  return HelpCircle;
}

export function ConversationEventBlock({ block }: ConversationEventBlockProps) {
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
  const isPreformatted = block.kind !== "message";

  return (
    <article className={cn("rounded-lg border text-sm shadow-xs", toneClasses(block.tone))}>
      <div className="flex items-center justify-between gap-3 border-b border-current/10 px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <Icon className="h-4 w-4 shrink-0 opacity-75" />
          <span className="truncate font-medium">{block.title}</span>
          <span className="text-muted-foreground rounded border px-1.5 py-0.5 text-[10px] uppercase">
            {block.kind}
          </span>
        </div>
        <span className="text-muted-foreground shrink-0 text-xs">
          {formatDateTime(block.createdAt)}
        </span>
      </div>
      {isPreformatted ? (
        <pre className="text-foreground max-h-96 overflow-auto p-3 font-mono text-xs leading-5 whitespace-pre-wrap">
          {displayBody}
        </pre>
      ) : (
        <p className="p-3 leading-6 whitespace-pre-wrap">{displayBody}</p>
      )}
    </article>
  );
}
