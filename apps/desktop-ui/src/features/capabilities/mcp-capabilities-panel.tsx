import { useQuery } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { listMcpServers } from "@/lib/tauri-api";
import { useTranslation } from "@/lib/i18n-react";

export function McpCapabilitiesPanel() {
  const { t } = useTranslation();
  const { data: mcpServers, isLoading } = useQuery({
    queryKey: ["mcp-servers"],
    queryFn: listMcpServers,
  });

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.registeredMcpServers")}</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-muted-foreground mb-4 text-sm">
            {t("capabilities.mcpReadOnly")}
          </p>

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
