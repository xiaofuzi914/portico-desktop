import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  dismissNotification,
  listNotifications,
  markNotificationRead,
} from "@/lib/tauri-api";
import {
  asNotificationId,
  asWorkspaceId,
  type Notification,
} from "@/lib/schemas";
import { formatRelativeTime } from "@/lib/formatters";
import { useTranslation } from "@/lib/i18n-react";
import { notificationKeys } from "@/lib/query-keys";

export function NotificationInbox() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [workspaceId, setWorkspaceId] = useState("");
  const [unreadOnly, setUnreadOnly] = useState(false);

  const workspaceIdFilter = workspaceId.trim()
    ? asWorkspaceId(workspaceId.trim())
    : null;

  const { data: notifications, isLoading, refetch } = useQuery({
    queryKey: notificationKeys.list(workspaceIdFilter, unreadOnly),
    queryFn: () => listNotifications(workspaceIdFilter, unreadOnly),
  });

  const markRead = useMutation({
    mutationFn: (notification: Notification) =>
      markNotificationRead(asNotificationId(notification.id)),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: notificationKeys.list(workspaceIdFilter, unreadOnly),
      });
    },
  });

  const dismiss = useMutation({
    mutationFn: (notification: Notification) =>
      dismissNotification(asNotificationId(notification.id)),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: notificationKeys.list(workspaceIdFilter, unreadOnly),
      });
    },
  });

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("notifications.title")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
            <Input
              placeholder={t("operations.filterByWorkspaceId")}
              value={workspaceId}
              onChange={(e) => setWorkspaceId(e.target.value)}
              className="sm:max-w-xs"
            />
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={unreadOnly}
                onChange={(e) => setUnreadOnly(e.target.checked)}
                className="h-4 w-4"
              />
              {t("operations.unreadOnly")}
            </label>
            <Button variant="outline" onClick={() => void refetch()}>
              {t("common.refresh")}
            </Button>
          </div>

          {isLoading ? (
            <p className="text-muted-foreground">{t("notifications.loading")}</p>
          ) : notifications?.length ? (
            <ul className="divide-y">
              {notifications.map((notification) => (
                <li key={notification.id} className="py-4">
                  <div className="flex items-start justify-between gap-4">
                    <div className="flex-1">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{notification.title}</span>
                        {!notification.read && (
                          <span className="bg-primary rounded-full px-2 py-0.5 text-xs font-medium text-white">
                            {t("operations.unread")}
                          </span>
                        )}
                      </div>
                      <p className="text-muted-foreground text-sm">
                        {notification.body}
                      </p>
                      <p className="text-muted-foreground mt-1 text-xs">
                        {notification.category} ·{" "}
                        {formatRelativeTime(notification.created_at)}
                      </p>
                    </div>
                    <div className="flex items-center gap-2">
                      {!notification.read && (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => markRead.mutate(notification)}
                          disabled={markRead.isPending}
                        >
                          {t("operations.markRead")}
                        </Button>
                      )}
                      <Button
                        variant="destructive"
                        size="sm"
                        onClick={() => dismiss.mutate(notification)}
                        disabled={dismiss.isPending}
                      >
                        {t("notifications.dismiss")}
                      </Button>
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-muted-foreground">{t("operations.noNotifications")}</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
