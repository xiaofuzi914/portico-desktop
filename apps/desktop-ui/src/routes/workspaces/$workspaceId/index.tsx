import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  ArrowRight,
  Bot,
  GitBranch,
  MessageSquare,
  Plus,
  ShieldCheck,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  createThread,
  listThreads,
  listWorkspaces,
  trustWorkspace,
} from "@/lib/tauri-api";
import { formatRelativeTime } from "@/lib/formatters";
import { asWorkspaceId } from "@/lib/schemas";
import { useEffect, useMemo, useRef } from "react";
import { useTranslation } from "@/lib/i18n-react";
import { typography } from "@/components/ui/typography";
import { workspaceKeys } from "@/lib/query-keys";
import { ConversationComposer } from "@/features/agent-client/conversation-composer";

export const Route = createFileRoute("/workspaces/$workspaceId/")({
  component: ProjectDetailPage,
});

function ProjectDetailPage() {
  const { workspaceId: workspaceIdParam } = Route.useParams();
  const workspaceId = asWorkspaceId(workspaceIdParam);
  const queryClient = useQueryClient();
  const navigate = useNavigate({ from: Route.fullPath });
  const { t } = useTranslation();

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
  const autoCreateStarted = useRef(false);

  useEffect(() => {
    autoCreateStarted.current = false;
  }, [workspaceId]);

  useEffect(() => {
    if (threadsLoading || !threads || threads.length > 0 || autoCreateStarted.current) {
      return;
    }

    autoCreateStarted.current = true;
    create.mutate();
  }, [create, threads, threadsLoading]);

  const trust = useMutation({
    mutationFn: (trusted: boolean) => trustWorkspace(workspaceId, trusted),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
    },
  });
  const createErrorMessage =
    create.error instanceof Error ? create.error.message : String(create.error);
  const hasThreads = !!threads?.length;
  const shouldShowThreadList = threadsLoading || hasThreads;

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
        {shouldShowThreadList ? (
          <div className="min-h-0 flex-1 overflow-auto p-6">
            <div className="mx-auto grid max-w-4xl gap-6">
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
                        onClick={() => {
                          autoCreateStarted.current = true;
                          create.mutate();
                        }}
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
                    ) : (
                      threads?.map((thread) => (
                        <Link
                          key={thread.id}
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
                      ))
                    )}
                  </div>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="min-h-0 flex-1" />
        )}

        {!shouldShowThreadList && (
          <div className="bg-surface/70 shrink-0 border-t px-6 py-4">
            <ConversationComposer
              disabled
              placeholder={create.isError ? t("thread.createFailed") : t("thread.creatingSession")}
              onSubmit={() => undefined}
              isSubmitting={create.isPending}
              controls={
                create.isError ? (
                  <div className="flex flex-wrap items-center gap-3">
                    <p className="text-destructive text-xs">{createErrorMessage}</p>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        autoCreateStarted.current = true;
                        create.mutate();
                      }}
                      disabled={create.isPending}
                    >
                      <Plus className="h-4 w-4" />
                      {t("sidebar.newThread")}
                    </Button>
                  </div>
                ) : (
                  <p className="text-muted-foreground text-xs">{t("thread.creatingSession")}</p>
                )
              }
            />
          </div>
        )}
      </section>
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
