import { useQuery } from "@tanstack/react-query";
import { Bot, Folder, GitBranch, MessageSquare } from "lucide-react";
import { listThreads, listWorkspaces, listWorktrees } from "@/lib/tauri-api";
import { cn } from "@/lib/utils";
import type { AgentRunId, AgentRunStatus, ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { typography } from "@/components/ui/typography";

interface ThreadHeaderProps {
  workspaceId: WorkspaceId;
  threadId: ThreadId;
  runId?: AgentRunId;
  status?: AgentRunStatus;
}

function RunStatusBadge({ status }: { status: AgentRunStatus }) {
  const colors: Record<AgentRunStatus, string> = {
    Queued: "bg-muted text-muted-foreground",
    Running: "bg-primary text-primary-foreground",
    WaitingApproval: "bg-warning text-warning-foreground",
    Paused: "bg-muted text-foreground",
    Cancelled: "bg-muted text-muted-foreground",
    Failed: "bg-destructive text-destructive-foreground",
    Completed: "bg-success text-success-foreground",
  };

  return (
    <span className={cn("rounded px-2 py-0.5 text-xs font-medium", colors[status])}>
      {status}
    </span>
  );
}

export function ThreadHeader({ workspaceId, threadId, runId, status }: ThreadHeaderProps) {
  const { t } = useTranslation();
  const { data: workspaces } = useQuery({
    queryKey: ["workspaces"],
    queryFn: listWorkspaces,
  });

  const { data: threads } = useQuery({
    queryKey: ["workspaces", workspaceId, "threads"],
    queryFn: () => listThreads(workspaceId),
  });

  const { data: worktrees } = useQuery({
    queryKey: ["workspaces", workspaceId, "worktrees"],
    queryFn: () => listWorktrees(workspaceId),
  });

  const workspace = workspaces?.find((w) => w.id === workspaceId);
  const thread = threads?.find((t) => t.id === threadId);
  const worktree = worktrees?.find((candidate) => candidate.thread_id === threadId);

  return (
    <header className="bg-background/95 flex h-[68px] shrink-0 items-center justify-between gap-4 border-b px-6">
      <div className="flex min-w-0 flex-col gap-1">
        <div className={`flex min-w-0 items-center gap-2 ${typography.metadata}`}>
          <Folder className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">{workspace?.name ?? workspaceId}</span>
          <span>/</span>
          <MessageSquare className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">{thread?.title ?? t("thread.thread")}</span>
        </div>
        <h1 className={`truncate ${typography.pageTitle}`}>
          {thread?.title ?? t("thread.thread")}
        </h1>
      </div>

      <div className="hidden shrink-0 items-center gap-2 lg:flex">
        {status && <RunStatusBadge status={status} />}
        <MetaPill icon={Bot} label={t("agent.model")} value={t("agent.default")} />
        <MetaPill
          icon={GitBranch}
          label={t("agent.worktree")}
          value={worktree?.name ?? t("agent.project")}
        />
        {runId && (
          <span className="text-muted-foreground max-w-32 truncate rounded-md border bg-muted/40 px-2 py-1 font-mono text-xs">
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
  value: string;
}) {
  return (
    <span className="text-muted-foreground flex h-7 items-center gap-1 rounded-md border bg-muted/30 px-2 text-xs">
      <Icon className="h-3.5 w-3.5" />
      <span>{label}:</span>
      <span className="text-foreground max-w-28 truncate">{value}</span>
    </span>
  );
}
