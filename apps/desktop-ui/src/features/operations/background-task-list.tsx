import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { cancelBackgroundTask, listBackgroundTasks } from "@/lib/tauri-api";
import { asBackgroundTaskId, asWorkspaceId, type BackgroundTask } from "@/lib/schemas";
import { formatRelativeTime } from "@/lib/formatters";
import { useTranslation } from "@/lib/i18n-react";
import { backgroundTaskKeys } from "@/lib/query-keys";

export function BackgroundTaskList() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [workspaceId, setWorkspaceId] = useState("");

  const workspaceIdFilter = workspaceId.trim() ? asWorkspaceId(workspaceId.trim()) : null;

  const {
    data: tasks,
    isLoading,
    refetch,
  } = useQuery({
    queryKey: backgroundTaskKeys.list(workspaceIdFilter),
    queryFn: () => listBackgroundTasks(workspaceIdFilter),
  });

  const cancel = useMutation({
    mutationFn: (task: BackgroundTask) => cancelBackgroundTask(asBackgroundTaskId(task.id)),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: backgroundTaskKeys.list(workspaceIdFilter),
      });
    },
  });

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("operations.backgroundTasks")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
            <Input
              placeholder={t("operations.filterByWorkspaceId")}
              value={workspaceId}
              onChange={(e) => setWorkspaceId(e.target.value)}
              className="sm:max-w-xs"
            />
            <Button variant="outline" onClick={() => void refetch()}>
              {t("common.refresh")}
            </Button>
          </div>

          {isLoading ? (
            <p className="text-muted-foreground">{t("operations.loadingBackgroundTasks")}</p>
          ) : tasks?.length ? (
            <ul className="divide-y">
              {tasks.map((task) => (
                <li key={task.id} className="py-4">
                  <div className="flex items-start justify-between gap-4">
                    <div className="flex-1">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{task.task_kind}</span>
                        <StatusBadge status={task.status} />
                      </div>
                      <p className="text-muted-foreground text-sm">
                        {t("operations.priority")} {task.priority} · {t("operations.attempts")}{" "}
                        {task.attempts}/{task.max_attempts}
                      </p>
                      <div className="text-muted-foreground mt-1 flex flex-wrap gap-2 text-xs">
                        {task.workspace_id && (
                          <span>
                            {t("operations.workspace")} {task.workspace_id}
                          </span>
                        )}
                        {task.thread_id && (
                          <span>
                            {t("operations.thread")} {task.thread_id}
                          </span>
                        )}
                        {task.run_id && (
                          <span>
                            {t("operations.run")} {task.run_id}
                          </span>
                        )}
                        {task.scheduled_at && (
                          <span>
                            {t("operations.scheduled")} {formatRelativeTime(task.scheduled_at)}
                          </span>
                        )}
                        <span>
                          {t("operations.updated")} {formatRelativeTime(task.updated_at)}
                        </span>
                      </div>
                    </div>
                    {(task.status === "Queued" || task.status === "Running") && (
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => cancel.mutate(task)}
                        disabled={cancel.isPending}
                      >
                        {t("operations.cancel")}
                      </Button>
                    )}
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-muted-foreground">{t("operations.noBackgroundTasks")}</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function StatusBadge({ status }: { status: BackgroundTask["status"] }) {
  const styles: Record<BackgroundTask["status"], string> = {
    Queued: "bg-amber-100 text-amber-800",
    Running: "bg-blue-100 text-blue-800",
    Completed: "bg-green-100 text-green-800",
    Failed: "bg-red-100 text-red-800",
    Cancelled: "bg-gray-100 text-gray-800",
  };
  return (
    <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${styles[status]}`}>
      {status}
    </span>
  );
}
