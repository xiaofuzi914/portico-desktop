import { Link, getRouteApi, useNavigate } from "@tanstack/react-router";
import { useQueries, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { ArrowRight, FolderOpen, MessageSquare, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { listWorkspaces, listThreads } from "@/lib/tauri-api";
import { formatRelativeTime } from "@/lib/formatters";
import { asWorkspaceId } from "@/lib/schemas";
import type { Thread, ThreadId, Workspace, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";
import { typography } from "@/components/ui/typography";
import { cn } from "@/lib/utils";
import { buildAgentHomeViewModel, type GlobalThreadItem } from "./agent-home-view-models";

const routeApi = getRouteApi("/");

export function AgentClientPage() {
  const { t } = useTranslation();
  const { workspaceId: workspaceIdParam } = routeApi.useSearch();

  const { data: workspaces, isLoading: workspacesLoading } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });

  if (workspacesLoading) {
    return (
      <main className="flex h-full items-center justify-center">
        <p className="text-muted-foreground text-sm">{t("sidebar.loadingProjects")}</p>
      </main>
    );
  }

  if (!workspaces?.length) {
    return <EmptyProjectState />;
  }

  return (
    <GlobalHomeOverview
      workspaces={workspaces}
      preferredWorkspaceId={workspaceIdParam ? asWorkspaceId(workspaceIdParam) : undefined}
    />
  );
}

function EmptyProjectState() {
  const { t } = useTranslation();

  return (
    <main className="h-full overflow-auto px-6 py-20 sm:py-28">
      <section className="mx-auto w-full max-w-xl space-y-5">
        <div className="bg-muted flex h-12 w-12 items-center justify-center rounded-lg border">
          <FolderOpen className="h-6 w-6" />
        </div>
        <div className="space-y-2">
          <h1 className={typography.pageTitle}>{t("home.emptyTitle")}</h1>
          <p className={`max-w-lg ${typography.pageDescription}`}>{t("home.emptyBody")}</p>
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

function GlobalHomeOverview({
  workspaces,
  preferredWorkspaceId,
}: {
  workspaces: Workspace[];
  preferredWorkspaceId?: WorkspaceId;
}) {
  const { t } = useTranslation();
  const navigate = useNavigate({ from: "/" });

  const threadQueries = useQueries({
    queries: workspaces.map((workspace) => ({
      queryKey: workspaceKeys.threads(workspace.id),
      queryFn: () => listThreads(workspace.id),
    })),
  });

  const threadsLoading = threadQueries.some((query) => query.isLoading);
  const allThreads = useMemo(
    () => threadQueries.flatMap((query) => query.data ?? []),
    [threadQueries],
  );

  const homeViewModel = useMemo(
    () => buildAgentHomeViewModel(workspaces, allThreads, preferredWorkspaceId),
    [workspaces, allThreads, preferredWorkspaceId],
  );

  const [selectedThreadId, setSelectedThreadId] = useState<ThreadId | undefined>();

  // Keep selection valid as data refreshes; prefer newest global conversation.
  useEffect(() => {
    const items = homeViewModel.globalThreads;
    if (!items.length) {
      setSelectedThreadId(undefined);
      return;
    }
    const stillValid = items.some((item) => item.thread.id === selectedThreadId);
    if (!stillValid) {
      setSelectedThreadId(items[0]?.thread.id);
    }
  }, [homeViewModel.globalThreads, selectedThreadId]);

  const selectedItem =
    homeViewModel.globalThreads.find((item) => item.thread.id === selectedThreadId) ??
    homeViewModel.globalThreads[0];

  const previewWorkspace =
    selectedItem?.workspace ?? homeViewModel.activeWorkspace ?? workspaces[0];

  function openConversation(item: GlobalThreadItem) {
    void navigate({
      to: "/workspaces/$workspaceId/threads/$threadId",
      params: {
        workspaceId: item.workspace.id,
        threadId: item.thread.id,
      },
    });
  }

  return (
    <main className="flex h-full flex-col overflow-hidden">
      <section className="border-b px-6 py-6">
        <div className="mx-auto max-w-6xl">
          <h1 className={`max-w-2xl ${typography.pageTitle}`}>{t("home.commandCenter")}</h1>
          <p className={`mt-3 max-w-2xl ${typography.pageDescription}`}>
            {t("home.globalConversationsBody")}
          </p>
        </div>
      </section>

      <section className="min-h-0 flex-1 overflow-auto p-6">
        <div className="mx-auto grid max-w-6xl gap-5 lg:grid-cols-[minmax(0,1fr)_320px]">
          <div className="agent-panel">
            <div className="flex items-center justify-between gap-3 border-b px-4 py-3">
              <h2 className={`flex items-center gap-2 ${typography.sectionTitle}`}>
                <MessageSquare className="h-4 w-4" />
                {t("home.allConversations")}
              </h2>
              <span className="text-muted-foreground text-xs">
                {homeViewModel.globalThreads.length} {t("common.total")}
              </span>
            </div>
            <div className="divide-y">
              {threadsLoading ? (
                <p className="text-muted-foreground p-4 text-sm">{t("sidebar.loadingThreads")}</p>
              ) : homeViewModel.globalThreads.length ? (
                homeViewModel.globalThreads.map((item) => (
                  <GlobalThreadRow
                    key={item.thread.id}
                    item={item}
                    selected={item.thread.id === selectedItem?.thread.id}
                    onSelect={() => setSelectedThreadId(item.thread.id)}
                    onOpen={() => openConversation(item)}
                  />
                ))
              ) : (
                <p className="text-muted-foreground p-4 text-sm">{t("home.noThreadsHint")}</p>
              )}
            </div>
          </div>

          {previewWorkspace ? (
            <ProjectPreviewPanel
              workspace={previewWorkspace}
              selectedThread={selectedItem?.thread}
              onOpenConversation={selectedItem ? () => openConversation(selectedItem) : undefined}
            />
          ) : null}
        </div>
      </section>
    </main>
  );
}

function GlobalThreadRow({
  item,
  selected,
  onSelect,
  onOpen,
}: {
  item: GlobalThreadItem;
  selected: boolean;
  onSelect: () => void;
  onOpen: () => void;
}) {
  const { t } = useTranslation();

  return (
    <button
      type="button"
      onClick={onSelect}
      onDoubleClick={onOpen}
      className={cn(
        "flex w-full items-center justify-between gap-4 px-4 py-3 text-left transition-colors",
        selected ? "bg-muted" : "hover:bg-muted/70",
      )}
    >
      <div className="min-w-0">
        <p className={`truncate ${typography.itemTitle}`}>{item.thread.title}</p>
        <p className={`mt-1 truncate ${typography.metadata}`}>
          {item.workspace.name}
          {" · "}
          {t("thread.updated")} {formatRelativeTime(item.thread.updated_at)}
        </p>
      </div>
      <ArrowRight className="text-muted-foreground h-4 w-4 shrink-0" />
    </button>
  );
}

function ProjectPreviewPanel({
  workspace,
  selectedThread,
  onOpenConversation,
}: {
  workspace: Workspace;
  selectedThread?: Thread;
  onOpenConversation?: () => void;
}) {
  const { t } = useTranslation();

  return (
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
          <Link to="/workspaces/$workspaceId" params={{ workspaceId: workspace.id }}>
            {t("home.manageProject")}
          </Link>
        </Button>
      </div>

      {selectedThread ? (
        <div className="bg-muted/30 mt-4 rounded-md border px-3 py-2 text-sm">
          <p className="text-muted-foreground text-xs">{t("home.selectedConversation")}</p>
          <p className="mt-1 truncate font-medium">{selectedThread.title}</p>
          <p className={`mt-1 ${typography.metadata}`}>{t("home.doubleClickToOpen")}</p>
          {onOpenConversation ? (
            <Button className="mt-3 w-full" size="sm" onClick={onOpenConversation}>
              {t("home.openConversation")}
            </Button>
          ) : null}
        </div>
      ) : null}

      <dl className="mt-5 space-y-3 text-sm">
        <div>
          <dt className="text-muted-foreground text-xs">{t("home.projectFolder")}</dt>
          <dd className="mt-1 truncate font-mono text-xs">{workspace.root_path}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground text-xs">{t("home.trust")}</dt>
          <dd className="mt-1">{workspace.trusted ? t("home.trusted") : t("home.untrusted")}</dd>
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
  );
}
