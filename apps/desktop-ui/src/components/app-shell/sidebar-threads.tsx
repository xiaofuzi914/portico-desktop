import { Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MessageSquare, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { createThread, listThreads } from "@/lib/tauri-api";
import type { WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

interface SidebarThreadsProps {
  workspaceId: WorkspaceId;
}

export function SidebarThreads({ workspaceId }: SidebarThreadsProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: threads, isLoading } = useQuery({
    queryKey: ["workspaces", workspaceId, "threads"],
    queryFn: () => listThreads(workspaceId),
  });
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
    <div className="space-y-2">
      <Button
        variant="ghost"
        size="sm"
        className="h-8 w-full justify-start px-2"
        onClick={() => create.mutate()}
        disabled={create.isPending}
      >
        <Plus className="mr-2 h-4 w-4" />
        {t("sidebar.newThread")}
      </Button>

      {isLoading ? (
        <p className="text-muted-foreground px-2 text-sm">{t("sidebar.loadingThreads")}</p>
      ) : threads?.length ? (
        <ul className="space-y-0.5">
          {threads.map((thread) => (
            <li key={thread.id}>
              <Link
                to="/workspaces/$workspaceId/threads/$threadId"
                params={{ workspaceId, threadId: thread.id }}
                className="text-muted-foreground hover:bg-sidebar-accent hover:text-foreground flex h-8 items-center gap-2 rounded-md px-2 text-sm transition-colors"
                activeProps={{
                  className:
                    "flex h-8 items-center gap-2 rounded-md px-2 text-sm bg-sidebar-accent font-medium text-foreground",
                }}
              >
                <MessageSquare className="h-3.5 w-3.5 shrink-0" />
                <span className="min-w-0 flex-1 truncate">{thread.title}</span>
              </Link>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-muted-foreground px-2 text-sm">{t("sidebar.noThreads")}</p>
      )}
    </div>
  );
}
