import { useMemo } from "react";
import { Link, useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, Folder, MessageSquare, Settings } from "lucide-react";
import { NotificationCenter } from "@/components/notifications/notification-center";
import { listThreads, listWorkspaces } from "@/lib/tauri-api";
import { asThreadId, asWorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";
import { LanguageToggle } from "./language-toggle";

export function AppTopbar() {
  const params = useParams({ strict: false }) as {
    workspaceId?: string;
    threadId?: string;
  };
  const workspaceId = params.workspaceId ? asWorkspaceId(params.workspaceId) : undefined;
  const threadId = params.threadId ? asThreadId(params.threadId) : undefined;
  const { t } = useTranslation();

  const { data: workspaces } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
    enabled: !!workspaceId,
  });

  const { data: threads } = useQuery({
    queryKey: workspaceKeys.threads(workspaceId!),
    queryFn: () => listThreads(workspaceId!),
    enabled: !!workspaceId,
  });

  const workspace = useMemo(
    () => workspaces?.find((w) => w.id === workspaceId),
    [workspaces, workspaceId],
  );

  const thread = useMemo(() => threads?.find((t) => t.id === threadId), [threads, threadId]);

  return (
    <header className="bg-surface flex h-[var(--topbar-height)] shrink-0 items-center justify-between border-b px-3 lg:px-4">
      <div className="flex min-w-0 items-center gap-2 text-sm">
        <Link to="/" className="flex items-center gap-2 font-semibold lg:hidden">
          <span className="bg-primary text-primary-foreground flex h-7 w-7 items-center justify-center rounded-md text-xs">
            P
          </span>
          <span>{t("app.name")}</span>
        </Link>
        <span className="text-muted-foreground hidden lg:inline">{t("app.tagline")}</span>
        {workspace && (
          <>
            <span className="text-muted-foreground hidden lg:inline">/</span>
            <Link
              to="/workspaces/$workspaceId"
              params={{ workspaceId: workspace.id }}
              className="text-muted-foreground hover:text-foreground hidden min-w-0 items-center gap-1 lg:flex"
            >
              <Folder className="h-3.5 w-3.5 shrink-0" />
              <span className="truncate">{workspace.name}</span>
            </Link>
          </>
        )}
        {thread && (
          <>
            <span className="text-muted-foreground hidden lg:inline">/</span>
            <span className="text-muted-foreground hidden min-w-0 items-center gap-1 lg:flex">
              <MessageSquare className="h-3.5 w-3.5 shrink-0" />
              <span className="truncate">{thread.title}</span>
            </span>
          </>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <div className="text-muted-foreground hidden items-center gap-1 rounded-md border px-2 py-1 text-xs lg:flex">
          <AlertTriangle className="h-3.5 w-3.5" />
          {t("app.recoveryInProgress")}
        </div>
        <LanguageToggle />
        <Link
          to="/settings"
          className="text-muted-foreground hover:bg-muted hover:text-foreground flex h-8 w-8 items-center justify-center rounded-md lg:hidden"
          aria-label={t("common.settings")}
        >
          <Settings className="h-4 w-4" />
        </Link>
        <NotificationCenter />
      </div>
    </header>
  );
}
