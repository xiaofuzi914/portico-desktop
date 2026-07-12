import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bot, Folder, MessageSquare, Pencil } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { listThreads, listWorkspaces, updateThreadTitle } from "@/lib/tauri-api";
import { cn } from "@/lib/utils";
import type { AgentRunId, AgentRunStatus, ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";
import { typography } from "@/components/ui/typography";
import { ThreadModelSelector } from "./thread-model-selector";

interface ThreadHeaderProps {
  workspaceId: WorkspaceId;
  threadId: ThreadId;
  runId?: AgentRunId;
  status?: AgentRunStatus;
}

function RunStatusBadge({ status }: { status: AgentRunStatus }) {
  const colors: Record<AgentRunStatus, string> = {
    Queued: "bg-muted text-muted-foreground ring-1 ring-border",
    Running:
      "bg-running text-white shadow-sm shadow-running/25 ring-1 ring-running/40",
    WaitingApproval: "bg-warning text-warning-foreground",
    Paused: "bg-muted text-foreground ring-1 ring-border",
    Cancelled: "bg-muted text-muted-foreground",
    Failed: "bg-destructive text-destructive-foreground",
    Interrupted: "bg-warning text-warning-foreground",
    Completed: "bg-success text-success-foreground",
  };

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium",
        colors[status],
        status === "Running" && "run-status-running",
      )}
    >
      {status === "Running" ? (
        <span className="conversation-running-dot bg-white" aria-hidden />
      ) : null}
      {status}
    </span>
  );
}

export function ThreadHeader({ workspaceId, threadId, runId, status }: ThreadHeaderProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const inputRef = useRef<HTMLInputElement>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  const { data: workspaces } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });

  const { data: threads } = useQuery({
    queryKey: workspaceKeys.threads(workspaceId),
    queryFn: () => listThreads(workspaceId),
  });

  const workspace = workspaces?.find((w) => w.id === workspaceId);
  const thread = threads?.find((t) => t.id === threadId);
  const title = thread?.title ?? t("thread.thread");

  const rename = useMutation({
    mutationFn: (next: string) => updateThreadTitle(threadId, next),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      setEditing(false);
    },
  });

  useEffect(() => {
    if (!editing) return;
    const el = inputRef.current;
    if (!el) return;
    el.focus();
    el.select();
  }, [editing]);

  function beginEdit() {
    setDraft(thread?.title ?? "");
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    setDraft("");
  }

  function commitEdit() {
    const next = draft.trim();
    if (!next) {
      cancelEdit();
      return;
    }
    if (next === (thread?.title ?? "").trim()) {
      setEditing(false);
      return;
    }
    rename.mutate(next);
  }

  const titleInput = (
    <div className="flex min-w-0 items-center gap-2">
      <input
        ref={inputRef}
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commitEdit();
          }
          if (event.key === "Escape") {
            event.preventDefault();
            cancelEdit();
          }
        }}
        onBlur={() => {
          window.setTimeout(() => {
            if (document.activeElement === inputRef.current) return;
            commitEdit();
          }, 0);
        }}
        disabled={rename.isPending}
        maxLength={80}
        aria-label={t("thread.editTitle")}
        className={cn(
          "border-input bg-background text-foreground h-8 w-full max-w-xl min-w-0 rounded-md border px-2 text-lg font-semibold outline-none",
          "focus-visible:ring-ring focus-visible:ring-2",
        )}
      />
      {rename.isError ? (
        <span className="text-destructive shrink-0 text-xs">
          {rename.error instanceof Error ? rename.error.message : t("thread.renameFailed")}
        </span>
      ) : null}
    </div>
  );

  return (
    <header className="bg-background/95 flex h-[68px] shrink-0 items-center justify-between gap-4 border-b px-6">
      <div className="flex min-w-0 flex-col gap-1">
        <div className={`flex min-w-0 items-center gap-2 ${typography.metadata}`}>
          <Folder className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">{workspace?.name ?? workspaceId}</span>
          <span>/</span>
          <MessageSquare className="h-3.5 w-3.5 shrink-0" />
          {/* Breadcrumb segment — double-click to rename (matches user selection in UI) */}
          <button
            type="button"
            className="hover:bg-muted/60 max-w-[14rem] truncate rounded px-1 text-left"
            title={t("thread.editTitleHint")}
            onDoubleClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              beginEdit();
            }}
          >
            {title}
          </button>
        </div>
        {editing ? (
          titleInput
        ) : (
          <button
            type="button"
            onDoubleClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              beginEdit();
            }}
            className="group hover:bg-muted/50 flex max-w-full min-w-0 items-center gap-1.5 rounded-md text-left"
            title={t("thread.editTitleHint")}
          >
            <h1 className={`truncate ${typography.pageTitle}`}>{title}</h1>
            <Pencil
              className="text-muted-foreground h-3.5 w-3.5 shrink-0 opacity-40 transition-opacity group-hover:opacity-100"
              aria-hidden
            />
          </button>
        )}
      </div>

      <div className="hidden shrink-0 items-center gap-2 lg:flex">
        {status && <RunStatusBadge status={status} />}
        <MetaPill
          icon={Bot}
          label={t("agent.model")}
          value={<ThreadModelSelector workspaceId={workspaceId} threadId={threadId} />}
        />
        {runId && (
          <span className="text-muted-foreground bg-muted/40 max-w-32 truncate rounded-md border px-2 py-1 font-mono text-xs">
            {runId}
          </span>
        )}
      </div>
    </header>
  );
}

function MetaPill({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof Bot;
  label: string;
  value: ReactNode;
}) {
  return (
    <span className="text-muted-foreground bg-muted/30 flex h-7 items-center gap-1 rounded-md border px-2 text-xs">
      <Icon className="h-3.5 w-3.5" />
      <span>{label}:</span>
      <span className="text-foreground max-w-56 truncate">{value}</span>
    </span>
  );
}
