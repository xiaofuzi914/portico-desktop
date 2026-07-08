import { useState, useRef, useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Bell, Check, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { dismissNotification, listNotifications, markNotificationRead } from "@/lib/tauri-api";
import { asNotificationId, type Notification } from "@/lib/schemas";
import { formatRelativeTime } from "@/lib/formatters";
import { useTranslation } from "@/lib/i18n-react";
import { notificationKeys } from "@/lib/query-keys";

export function NotificationCenter() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const { data: notifications, isLoading } = useQuery({
    queryKey: notificationKeys.list(null, true),
    queryFn: () => listNotifications(null, true),
  });

  const markRead = useMutation({
    mutationFn: (id: string) => markNotificationRead(asNotificationId(id)),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: notificationKeys.list(null, true) });
    },
  });

  const dismiss = useMutation({
    mutationFn: (id: string) => dismissNotification(asNotificationId(id)),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: notificationKeys.list(null, true) });
    },
  });

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    if (open) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [open]);

  const unreadCount = notifications?.length ?? 0;

  return (
    <div className="relative" ref={ref}>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("notifications.aria")}
        onClick={() => setOpen((prev) => !prev)}
        className="relative"
      >
        <Bell className="h-5 w-5" />
        {unreadCount > 0 && (
          <span className="bg-destructive absolute top-1 right-1 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] font-medium text-white">
            {unreadCount > 99 ? "99+" : unreadCount}
          </span>
        )}
      </Button>

      {open && (
        <Card className="absolute top-full right-0 z-50 mt-2 w-80 shadow-lg">
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">{t("notifications.title")}</CardTitle>
              <Button variant="ghost" size="icon" onClick={() => setOpen(false)}>
                <X className="h-4 w-4" />
              </Button>
            </div>
          </CardHeader>
          <CardContent className="max-h-96 overflow-y-auto pt-0">
            {isLoading ? (
              <p className="text-muted-foreground text-sm">{t("notifications.loading")}</p>
            ) : notifications?.length ? (
              <ul className="space-y-2">
                {notifications.map((notification) => (
                  <NotificationItem
                    key={notification.id}
                    notification={notification}
                    onMarkRead={() => markRead.mutate(notification.id)}
                    onDismiss={() => dismiss.mutate(notification.id)}
                  />
                ))}
              </ul>
            ) : (
              <p className="text-muted-foreground text-sm">{t("notifications.empty")}</p>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}

interface NotificationItemProps {
  notification: Notification;
  onMarkRead: () => void;
  onDismiss: () => void;
}

function NotificationItem({ notification, onMarkRead, onDismiss }: NotificationItemProps) {
  const { t } = useTranslation();

  return (
    <li className="rounded-md border p-3">
      <div className="flex items-start justify-between gap-2">
        <div className="flex-1">
          <p className="text-sm font-medium">{notification.title}</p>
          <p className="text-muted-foreground text-sm">{notification.body}</p>
          <p className="text-muted-foreground mt-1 text-xs">
            {notification.category} · {formatRelativeTime(notification.created_at)}
          </p>
        </div>
        <div className="flex flex-col gap-1">
          {!notification.read && (
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={onMarkRead}
              aria-label={t("notifications.markRead")}
            >
              <Check className="h-4 w-4" />
            </Button>
          )}
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={onDismiss}
            aria-label={t("notifications.dismiss")}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </li>
  );
}
