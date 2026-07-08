import { Link, useNavigate, getRouteApi } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { ArrowRight, FolderOpen, MessageSquare, Plus, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { createThread, listWorkspaces, listThreads } from "@/lib/tauri-api";
import { formatRelativeTime } from "@/lib/formatters";
import { asWorkspaceId } from "@/lib/schemas";
import type { Workspace, WorkspaceId, Thread } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { typography } from "@/components/ui/typography";
import { buildAgentHomeViewModel } from "./agent-home-view-models";

const routeApi = getRouteApi("/");

export function AgentClientPage() {
  const { t } = useTranslation();
  const { workspaceId: workspaceIdParam } = routeApi.useSearch();

  const { data: workspaces, isLoading: workspacesLoading } = useQuery({
    queryKey: ["workspaces"],
    queryFn: listWorkspaces,
  });

  const selectedWorkspace = useMemo(() => {
    if (!workspaces?.length) return undefined;
    if (workspaceIdParam) {
      const match = workspaces.find((workspace) => workspace.id === workspaceIdParam);
      if (match) return match;
    }
    return workspaces[0];
  }, [workspaces, workspaceIdParam]);

  if (workspacesLoading) {
    return (
      <main className="flex h-full items-center justify-center">
        <p className="text-muted-foreground text-sm">{t("sidebar.loadingProjects")}</p>
      </main>
    );
  }

  if (!workspaces?.length || !selectedWorkspace) {
    return <EmptyProjectState />;
  }

  const workspaceId = asWorkspaceId(selectedWorkspace.id);

  return <WorkspaceOverview workspace={selectedWorkspace} workspaceId={workspaceId} />;
}

function EmptyProjectState() {
  const { t } = useTranslation();

  return (
    <main className="h-full overflow-auto px-6 py-20 sm:py-28">
      <section className="mx-auto w-full max-w-xl space-y-5">
        <div className="flex h-12 w-12 items-center justify-center rounded-lg border bg-muted">
          <FolderOpen className="h-6 w-6" />
        </div>
        <div className="space-y-2">
          <h1 className={typography.pageTitle}>{t("home.emptyTitle")}</h1>
          <p className={`max-w-lg ${typography.pageDescription}`}>
            {t("home.emptyBody")}
          </p>
        </div>
        <Button asChild>
          <Link to="/workspaces">
            <Plus className="h-4 w-4" />
            {t("home.newProject")}
          </Link>
        </Button>
      </section>
    </main>
  );
}

function WorkspaceOverview({
  workspace,
  workspaceId,
}: {
  workspace: Workspace;
  workspaceId: WorkspaceId;
}) {
  const navigate = useNavigate({ from: "/" });
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  const { data: threads, isLoading: threadsLoading } = useQuery({
    queryKey: ["workspaces", workspaceId, "threads"],
    queryFn: () => listThreads(workspaceId),
  });
  const homeViewModel = buildAgentHomeViewModel([workspace], threads ?? [], workspaceId);
  const latestThread = homeViewModel.recentThreads[0];
  const create = useMutation({
    mutationFn: () => createThread(workspaceId, t("thread.defaultTitle")),
    onSuccess: (thread) => {
      void queryClient.invalidateQueries({ queryKey: ["workspaces", workspaceId, "threads"] });
      void navigate({
        to: "/workspaces/$workspaceId/threads/$threadId",
        params: { workspaceId, threadId: thread.id },
      });
    },
  });

  return (
    <main className="flex h-full flex-col overflow-hidden">
      <section className="border-b px-6 py-6">
        <div className="mx-auto flex max-w-6xl flex-col gap-5">
          <div className="flex items-center gap-2 text-xs font-semibold tracking-wide text-muted-foreground uppercase">
            <Sparkles className="h-3.5 w-3.5" />
            {t("home.nextStep")}
          </div>
          <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_320px] lg:items-end">
            <div className="min-w-0">
              <h1 className={`max-w-2xl ${typography.pageTitle}`}>
                {t("home.commandCenter")}
              </h1>
              <p className={`mt-3 max-w-2xl ${typography.pageDescription}`}>
                {t("home.commandCenterBody")}
              </p>
            </div>
            <PrimaryAction
              workspaceId={workspaceId}
              latestThread={latestThread}
              onCreateThread={() => create.mutate()}
              isCreatingThread={create.isPending}
            />
          </div>
        </div>
      </section>

      <section className="min-h-0 flex-1 overflow-auto p-6">
        <div className="mx-auto grid max-w-6xl gap-5 lg:grid-cols-[minmax(0,1fr)_320px]">
          <div className="agent-panel">
            <div className="flex items-center justify-between gap-3 border-b px-4 py-3">
              <h2 className={`flex items-center gap-2 ${typography.sectionTitle}`}>
                <MessageSquare className="h-4 w-4" />
                {t("home.recentThreads")}
              </h2>
              <span className="text-muted-foreground text-xs">
                {homeViewModel.recentThreads.length} {t("common.total")}
              </span>
            </div>
            <div className="divide-y">
              {threadsLoading ? (
                <p className="text-muted-foreground p-4 text-sm">{t("sidebar.loadingThreads")}</p>
              ) : homeViewModel.recentThreads.length ? (
                homeViewModel.recentThreads.map((thread) => (
                  <ThreadRow key={thread.id} workspaceId={workspaceId} thread={thread} />
                ))
              ) : (
                <p className="text-muted-foreground p-4 text-sm">
                  {t("home.noThreadsHint")}
                </p>
              )}
            </div>
          </div>

          <div className="agent-panel h-fit p-4">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="text-muted-foreground flex items-center gap-1 text-xs font-semibold tracking-wide uppercase">
                  <FolderOpen className="h-3.5 w-3.5" />
                  {t("home.activeProject")}
                </p>
                <h2 className={`mt-1 truncate ${typography.cardTitle}`}>{workspace.name}</h2>
              </div>
              <Button variant="outline" size="sm" asChild>
                <Link to="/workspaces/$workspaceId" params={{ workspaceId }}>
                  {t("home.manageProject")}
                </Link>
              </Button>
            </div>
            <dl className="mt-5 space-y-3 text-sm">
              <div>
                <dt className="text-muted-foreground text-xs">{t("home.projectFolder")}</dt>
                <dd className="mt-1 truncate font-mono text-xs">{workspace.root_path}</dd>
              </div>
              <div>
                <dt className="text-muted-foreground text-xs">{t("home.trust")}</dt>
                <dd className="mt-1">
                  {workspace.trusted ? t("home.trusted") : t("home.untrusted")}
                </dd>
              </div>
              <div>
                <dt className="text-muted-foreground text-xs">{t("home.readPaths")}</dt>
                <dd className="mt-1 truncate">{workspace.allowed_read_paths.length}</dd>
              </div>
              <div>
                <dt className="text-muted-foreground text-xs">{t("home.writePaths")}</dt>
                <dd className="mt-1 truncate">{workspace.allowed_write_paths.length}</dd>
              </div>
            </dl>
          </div>
        </div>
      </section>
    </main>
  );
}

function PrimaryAction({
  workspaceId,
  latestThread,
  onCreateThread,
  isCreatingThread,
}: {
  workspaceId: WorkspaceId;
  latestThread?: Thread;
  onCreateThread: () => void;
  isCreatingThread?: boolean;
}) {
  const { t } = useTranslation();

  return (
    <div className="rounded-lg border bg-background p-3 shadow-xs">
      {latestThread ? (
        <Button className="h-11 w-full justify-between" asChild>
          <Link
            to="/workspaces/$workspaceId/threads/$threadId"
            params={{ workspaceId, threadId: latestThread.id }}
          >
            <span>{t("home.continueLatest")}</span>
            <ArrowRight className="h-4 w-4" />
          </Link>
        </Button>
      ) : (
        <Button
          className="h-11 w-full justify-between"
          onClick={onCreateThread}
          disabled={isCreatingThread}
        >
          <span>{t("sidebar.newThread")}</span>
          <Plus className="h-4 w-4" />
        </Button>
      )}
      <Button variant="ghost" className="mt-2 h-9 w-full justify-between" asChild>
        <Link to="/workspaces/$workspaceId" params={{ workspaceId }}>
          <span>{t("home.manageProject")}</span>
          <ArrowRight className="h-4 w-4" />
        </Link>
      </Button>
    </div>
  );
}

function ThreadRow({ workspaceId, thread }: { workspaceId: WorkspaceId; thread: Thread }) {
  const { t } = useTranslation();

  return (
    <Link
      to="/workspaces/$workspaceId/threads/$threadId"
      params={{ workspaceId, threadId: thread.id }}
      className="hover:bg-muted/70 flex items-center justify-between gap-4 px-4 py-3 transition-colors"
    >
      <div className="min-w-0">
        <p className={`truncate ${typography.itemTitle}`}>{thread.title}</p>
        <p className={`mt-1 ${typography.metadata}`}>
          {t("thread.updated")} {formatRelativeTime(thread.updated_at)}
        </p>
      </div>
      <ArrowRight className="text-muted-foreground h-4 w-4 shrink-0" />
    </Link>
  );
}
