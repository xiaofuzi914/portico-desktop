import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { listMcpServers, refreshMcpTools } from "@/lib/tauri-api";
import { useTranslation } from "@/lib/i18n-react";
import { mcpKeys } from "@/lib/query-keys";

export function McpCapabilitiesPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: mcpServers, isLoading } = useQuery({
    queryKey: mcpKeys.list(),
    queryFn: listMcpServers,
  });

  const refreshTools = useMutation({
    mutationFn: refreshMcpTools,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: mcpKeys.list() });
    },
  });

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.registeredMcpServers")}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="mb-4 flex items-center justify-between gap-4">
            <p className="text-muted-foreground text-sm">
              {t("capabilities.mcpReadOnly")}
            </p>
            <Button
              size="sm"
              variant="outline"
              onClick={() => refreshTools.mutate()}
              disabled={refreshTools.isPending}
            >
              {refreshTools.isPending
                ? t("capabilities.refreshingMcpTools")
                : t("capabilities.refreshMcpTools")}
            </Button>
          </div>

          {refreshTools.error && (
            <p className="text-destructive mb-4 text-sm">
              {refreshTools.error instanceof Error
                ? refreshTools.error.message
                : String(refreshTools.error)}
            </p>
          )}

          {isLoading ? (
            <p className="text-muted-foreground">{t("capabilities.loadingMcpServers")}</p>
          ) : mcpServers?.length ? (
            <ul className="divide-y">
              {mcpServers.map((server) => (
                <li key={server.id} className="flex flex-col gap-1 py-3">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{server.name}</span>
                    <span className="text-muted-foreground text-sm">{server.transport}</span>
                    <span
                      className={`text-xs ${server.enabled ? "text-green-600" : "text-amber-600"}`}
                    >
                      {server.enabled ? t("common.enabled") : t("common.disabled")}
                    </span>
                  </div>
                  <p className="text-muted-foreground text-xs">
                    {server.transport === "Stdio"
                      ? [server.command, ...server.args].filter(Boolean).join(" ") ||
                        t("capabilities.noCommand")
                      : (server.url ?? t("capabilities.noUrl"))}
                  </p>
                  {Object.keys(server.env).length > 0 && (
                    <p className="text-muted-foreground text-xs">
                      {t("capabilities.env")} {Object.keys(server.env).join(", ")}
                    </p>
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-muted-foreground">{t("capabilities.noMcpServers")}</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
