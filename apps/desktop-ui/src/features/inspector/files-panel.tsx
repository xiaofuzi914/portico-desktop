import { useMemo } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { gitDiff, gitStatus, listWorkspaces } from "@/lib/tauri-api";
import type { WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

interface FilesPanelProps {
  workspaceId: WorkspaceId;
}

export function FilesPanel({ workspaceId }: FilesPanelProps) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  const {
    data: workspaces,
    isLoading: loadingWorkspaces,
    error: workspacesError,
  } = useQuery({
    queryKey: ["workspaces"],
    queryFn: listWorkspaces,
  });

  const workspace = useMemo(
    () => workspaces?.find((w) => w.id === workspaceId),
    [workspaces, workspaceId],
  );

  const repoPath = workspace?.root_path ?? "";

  const {
    data: status,
    isLoading: loadingStatus,
    error: statusError,
  } = useQuery({
    queryKey: ["workspaces", workspaceId, "git-status", repoPath],
    queryFn: () => gitStatus(workspaceId, repoPath),
    enabled: !!repoPath,
  });

  const {
    data: diff,
    isLoading: loadingDiff,
    error: diffError,
  } = useQuery({
    queryKey: ["workspaces", workspaceId, "git-diff", repoPath],
    queryFn: () => gitDiff(workspaceId, repoPath),
    enabled: !!repoPath,
  });

  function handleRefresh() {
    void queryClient.invalidateQueries({
      queryKey: ["workspaces", workspaceId, "git-status"],
    });
    void queryClient.invalidateQueries({
      queryKey: ["workspaces", workspaceId, "git-diff"],
    });
  }

  if (workspacesError) {
    return <InlineError title={t("inspector.loadWorkspaceFailed")} message={workspacesError.message} />;
  }
  if (loadingWorkspaces) return <PanelLoading />;
  if (!workspace) {
    return (
      <InlineError
        title={t("inspector.workspaceNotFound")}
        message={t("inspector.workspaceRootMissing")}
      />
    );
  }

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-3">
      <div className="flex items-center justify-between">
        <h3 className="text-muted-foreground text-xs font-semibold">Git</h3>
        <Button variant="outline" size="sm" onClick={handleRefresh} className="h-7 gap-1 px-2">
          <RefreshCw className="h-3 w-3" />
          {t("common.refresh")}
        </Button>
      </div>
      <GitCard
        title={t("inspector.status")}
        loading={loadingStatus}
        error={statusError}
        content={status}
      />
      <GitCard title={t("inspector.diff")} loading={loadingDiff} error={diffError} content={diff} />
    </div>
  );
}

function GitCard({
  title,
  loading,
  error,
  content,
}: {
  title: string;
  loading: boolean;
  error: Error | null;
  content?: string;
}) {
  const { t } = useTranslation();

  return (
    <section className="overflow-hidden rounded-md border bg-background">
      <div className="border-b px-3 py-2">
        <h4 className="text-xs font-medium">{title}</h4>
      </div>
      <div className="p-3">
        {loading && <p className="text-muted-foreground text-xs">{t("common.loading")}</p>}
        {error && <p className="text-xs text-red-600">{error.message}</p>}
        {!loading && !error && (
          <pre className="text-muted-foreground max-h-64 overflow-auto font-mono text-xs whitespace-pre-wrap">
            {content || t("inspector.noChanges")}
          </pre>
        )}
      </div>
    </section>
  );
}

function InlineError({ title, message }: { title: string; message: string }) {
  return (
    <div className="p-3">
      <div className="rounded border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950">
        <p className="font-semibold">{title}</p>
        <p>{message}</p>
      </div>
    </div>
  );
}

function PanelLoading() {
  const { t } = useTranslation();
  return <p className="text-muted-foreground p-3 text-xs">{t("inspector.loading")}</p>;
}
