import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  ArrowRight,
  Bot,
  GitBranch,
  MessageSquare,
  Network,
  Plus,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import {
  archiveThread,
  createThread,
  listThreads,
  listWorkspaces,
  trustWorkspace,
} from "@/lib/tauri-api";
import { formatRelativeTime } from "@/lib/formatters";
import { asWorkspaceId, type ThreadId } from "@/lib/schemas";
import { useMemo, useState } from "react";
import { useTranslation } from "@/lib/i18n-react";
import { typography } from "@/components/ui/typography";
import { workspaceKeys } from "@/lib/query-keys";
import { ProjectFoldersPanel } from "@/features/workspaces/project-folders-panel";

export const Route = createFileRoute("/workspaces/$workspaceId/")({
  component: ProjectDetailPage,
});

function ProjectDetailPage() {
  const { workspaceId: workspaceIdParam } = Route.useParams();
  const workspaceId = asWorkspaceId(workspaceIdParam);
  const queryClient = useQueryClient();
  const navigate = useNavigate({ from: Route.fullPath });
  const { t } = useTranslation();
  const [threadPendingArchive, setThreadPendingArchive] = useState<ThreadId | null>(null);

  const { data: workspaces, isLoading: workspaceLoading } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });

  const workspace = useMemo(
    () => workspaces?.find((w) => w.id === workspaceId),
    [workspaces, workspaceId],
  );

  const { data: threads, isLoading: threadsLoading } = useQuery({
    queryKey: workspaceKeys.threads(workspaceId),
    queryFn: () => listThreads(workspaceId),
  });

  const create = useMutation({
    mutationFn: () => createThread(workspaceId, t("thread.defaultTitle")),
    onSuccess: (thread) => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      void navigate({
        to: "/workspaces/$workspaceId/threads/$threadId",
        params: { workspaceId, threadId: thread.id },
      });
    },
  });

  /** Product “delete” = soft archive (30-day retention). */
  const remove = useMutation({
    mutationFn: (threadId: ThreadId) => archiveThread(workspaceId, threadId),
    onSuccess: (_data, threadId) => {
      queryClient.removeQueries({ queryKey: ["messages", threadId] });
      queryClient.removeQueries({ queryKey: ["runs", threadId] });
      queryClient.removeQueries({
        predicate: (query) => query.queryKey.includes(threadId),
      });
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      void queryClient.invalidateQueries({
        queryKey: workspaceKeys.archivedThreads(workspaceId),
      });
    },
  });

  const trust = useMutation({
    mutationFn: (trusted: boolean) => trustWorkspace(workspaceId, trusted),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
    },
  });
  const hasThreads = !!threads?.length;

  return (
    <main className="flex h-full flex-col overflow-hidden">
      <section className="border-b px-6 py-5">
        <Link
          to="/workspaces"
          className="text-muted-foreground hover:text-foreground mb-3 inline-flex items-center gap-1 text-sm"
        >
          <ArrowLeft className="h-4 w-4" />
          {t("project.back")}
        </Link>
        <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
          <div className="min-w-0">
            <h1 className={`truncate ${typography.pageTitle}`}>
              {workspaceLoading ? "Project" : (workspace?.name ?? "Project")}
            </h1>
            <p className={`mt-1 truncate font-mono ${typography.metadata}`}>
              {workspace?.root_path}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <ProjectActionIcon
              to="/workspaces/$workspaceId/canvas"
              params={{ workspaceId }}
              icon={Network}
              label={t("project.canvas")}
            />
            <ProjectActionIcon
              to="/workspaces/$workspaceId/memory"
              params={{ workspaceId }}
              icon={Bot}
              label={t("project.memoryContext")}
            />
            <ProjectActionIcon
              to="/workspaces/$workspaceId/git"
              params={{ workspaceId }}
              icon={GitBranch}
              label={t("project.git")}
            />
            <Button
              variant={workspace?.trusted ? "default" : "outline"}
              onClick={() => trust.mutate(!workspace?.trusted)}
              disabled={trust.isPending || workspaceLoading}
            >
              <ShieldCheck className="h-4 w-4" />
              {workspace?.trusted ? t("project.trustedProject") : t("project.trustProject")}
            </Button>
          </div>
        </div>
      </section>

      <section className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div className="min-h-0 flex-1 overflow-auto p-6">
          <div className="mx-auto grid max-w-4xl gap-6">
            {workspace ? <ProjectFoldersPanel workspace={workspace} /> : null}
            <div className="space-y-6">
              <div className="agent-panel overflow-hidden">
                <div className="flex items-center justify-between border-b px-4 py-3">
                  <h2 className={`flex items-center gap-2 ${typography.sectionTitle}`}>
                    <MessageSquare className="h-4 w-4" />
                    {t("nav.threads")}
                  </h2>
                  <div className="flex items-center gap-3">
                    <span className={typography.metadata}>
                      {threads?.length ?? 0} {t("common.total")}
                    </span>
                    <Button
                      type="button"
                      onClick={() => create.mutate()}
                      disabled={create.isPending}
                    >
                      <Plus className="h-4 w-4" />
                      {t("sidebar.newThread")}
                    </Button>
                  </div>
                </div>
                <div className="divide-y">
                  {threadsLoading ? (
                    <p className="text-muted-foreground p-4 text-sm">
                      {t("sidebar.loadingThreads")}
                    </p>
                  ) : hasThreads ? (
                    threads?.map((thread) => (
                      <div key={thread.id} className="hover:bg-muted/70 flex items-center gap-2">
                        <Link
                          to="/workspaces/$workspaceId/threads/$threadId"
                          params={{ workspaceId, threadId: thread.id }}
                          className="flex min-w-0 flex-1 items-center justify-between gap-4 px-4 py-3 transition-colors"
                        >
                          <div className="min-w-0">
                            <p className={`truncate ${typography.itemTitle}`}>{thread.title}</p>
                            <p className={`mt-1 ${typography.metadata}`}>
                              {t("thread.updated")} {formatRelativeTime(thread.updated_at)}
                            </p>
                          </div>
                          <ArrowRight className="text-muted-foreground h-4 w-4 shrink-0" />
                        </Link>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon"
                          className="text-muted-foreground hover:text-destructive mr-3 shrink-0"
                          aria-label={t("thread.moveToArchive")}
                          title={t("thread.moveToArchive")}
                          disabled={remove.isPending}
                          onClick={() => setThreadPendingArchive(thread.id)}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    ))
                  ) : (
                    <div className="flex flex-col items-center gap-3 px-4 py-10 text-center">
                      <p className={typography.sectionTitle}>{t("thread.noThreads")}</p>
                      <p className="text-muted-foreground max-w-sm text-sm">
                        {t("thread.noThreadsHint")}
                      </p>
                      <Button
                        type="button"
                        onClick={() => create.mutate()}
                        disabled={create.isPending}
                      >
                        <Plus className="h-4 w-4" />
                        {t("sidebar.newThread")}
                      </Button>
                    </div>
                  )}
                </div>
                {remove.isError && (
                  <p className="text-destructive border-t px-4 py-3 text-sm">
                    {remove.error instanceof Error ? remove.error.message : String(remove.error)}
                  </p>
                )}
                {create.isError && (
                  <p className="text-destructive border-t px-4 py-3 text-sm">
                    {create.error instanceof Error ? create.error.message : String(create.error)}
                  </p>
                )}
              </div>
            </div>
          </div>
        </div>
      </section>

      <ConfirmDialog
        open={threadPendingArchive !== null}
        title={t("thread.moveToArchive")}
        description={t("thread.archiveConfirm")}
        confirmLabel={t("thread.moveToArchive")}
        destructive
        onConfirm={() => {
          if (threadPendingArchive) {
            remove.mutate(threadPendingArchive);
          }
          setThreadPendingArchive(null);
        }}
        onCancel={() => setThreadPendingArchive(null)}
      />
    </main>
  );
}

function ProjectActionIcon({
  to,
  params,
  icon: Icon,
  label,
}: {
  to: string;
  params?: Record<string, string>;
  icon: typeof Bot;
  label: string;
}) {
  return (
    <Button variant="outline" size="icon" title={label} aria-label={label} asChild>
      <Link to={to} params={params} aria-label={label}>
        <Icon className="h-4 w-4" />
      </Link>
    </Button>
  );
}
