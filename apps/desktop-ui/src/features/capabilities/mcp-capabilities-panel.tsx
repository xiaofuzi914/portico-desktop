import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  addMcpServer,
  listMcpServers,
  refreshMcpTools,
  removeMcpServer,
} from "@/lib/tauri-api";
import {
  mcpServerConfigSchema,
  type McpServerConfig,
  type McpTransport,
} from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { mcpKeys } from "@/lib/query-keys";

function defaultMcpConfig(): Omit<McpServerConfig, "id"> {
  return {
    name: "",
    transport: "Stdio",
    command: null,
    args: [],
    url: null,
    env: {},
    enabled: true,
  };
}

export function McpCapabilitiesPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [mcpConfig, setMcpConfig] = useState<Omit<McpServerConfig, "id">>(defaultMcpConfig());
  const [envKey, setEnvKey] = useState("");
  const [envValue, setEnvValue] = useState("");
  const [formError, setFormError] = useState<string | null>(null);

  const { data: mcpServers = [], isLoading } = useQuery({
    queryKey: mcpKeys.list(),
    queryFn: listMcpServers,
  });

  const refreshTools = useMutation({
    mutationFn: refreshMcpTools,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: mcpKeys.list() });
    },
  });

  const addMcp = useMutation({
    mutationFn: () => {
      setFormError(null);
      const result = mcpServerConfigSchema.omit({ id: true }).safeParse(mcpConfig);
      if (!result.success) {
        throw new Error(result.error.errors.map((e) => e.message).join("; "));
      }
      return addMcpServer(result.data);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: mcpKeys.list() });
      void refreshMcpTools();
      setMcpConfig(defaultMcpConfig());
      setEnvKey("");
      setEnvValue("");
    },
    onError: (err) => {
      setFormError(err instanceof Error ? err.message : String(err));
    },
  });

  const removeMcp = useMutation({
    mutationFn: (id: number) => removeMcpServer(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: mcpKeys.list() });
      void refreshMcpTools();
    },
  });

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.mcpModuleTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="text-muted-foreground space-y-2 text-sm leading-relaxed">
          <p>{t("capabilities.mcpModuleBody")}</p>
          <ul className="list-disc space-y-1 pl-5">
            <li>{t("capabilities.mcpModulePoint1")}</li>
            <li>{t("capabilities.mcpModulePoint2")}</li>
            <li>{t("capabilities.mcpModulePoint3")}</li>
          </ul>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.addMcpServer")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <Input
              placeholder={t("capabilities.serverName")}
              value={mcpConfig.name}
              onChange={(e) => setMcpConfig((prev) => ({ ...prev, name: e.target.value }))}
            />
            <select
              className="border-input bg-background h-9 rounded-md border px-3 text-sm"
              value={mcpConfig.transport}
              onChange={(e) =>
                setMcpConfig((prev) => ({
                  ...prev,
                  transport: e.target.value as McpTransport,
                }))
              }
            >
              <option value="Stdio">Stdio</option>
              <option value="Http">HTTP</option>
            </select>
          </div>

          {mcpConfig.transport === "Stdio" ? (
            <div className="grid gap-3 sm:grid-cols-2">
              <Input
                placeholder={t("capabilities.command")}
                value={mcpConfig.command ?? ""}
                onChange={(e) =>
                  setMcpConfig((prev) => ({
                    ...prev,
                    command: e.target.value || null,
                  }))
                }
              />
              <Input
                placeholder={t("capabilities.arguments")}
                value={mcpConfig.args.join(" ")}
                onChange={(e) =>
                  setMcpConfig((prev) => ({
                    ...prev,
                    args: e.target.value.split(" ").filter(Boolean),
                  }))
                }
              />
            </div>
          ) : (
            <Input
              placeholder={t("capabilities.url")}
              value={mcpConfig.url ?? ""}
              onChange={(e) =>
                setMcpConfig((prev) => ({
                  ...prev,
                  url: e.target.value || null,
                }))
              }
            />
          )}

          <div className="space-y-2">
            <p className="text-sm font-medium">{t("capabilities.environmentVariables")}</p>
            <div className="flex flex-col gap-2 sm:flex-row">
              <Input
                placeholder={t("capabilities.key")}
                value={envKey}
                onChange={(e) => setEnvKey(e.target.value)}
              />
              <Input
                placeholder={t("capabilities.value")}
                value={envValue}
                onChange={(e) => setEnvValue(e.target.value)}
              />
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  if (!envKey.trim()) return;
                  setMcpConfig((prev) => ({
                    ...prev,
                    env: { ...prev.env, [envKey.trim()]: envValue },
                  }));
                  setEnvKey("");
                  setEnvValue("");
                }}
              >
                {t("capabilities.add")}
              </Button>
            </div>
            {Object.entries(mcpConfig.env).map(([key, value]) => (
              <div key={key} className="flex items-center justify-between text-sm">
                <span>
                  {key}={value}
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() =>
                    setMcpConfig((prev) => {
                      const next = { ...prev.env };
                      delete next[key];
                      return { ...prev, env: next };
                    })
                  }
                >
                  {t("capabilities.remove")}
                </Button>
              </div>
            ))}
          </div>

          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={mcpConfig.enabled}
              onChange={(e) => setMcpConfig((prev) => ({ ...prev, enabled: e.target.checked }))}
              className="h-4 w-4"
            />
            {t("operations.enabled")}
          </label>

          {formError && <p className="text-destructive text-sm">{formError}</p>}
          {addMcp.error && (
            <p className="text-destructive text-sm">
              {addMcp.error instanceof Error ? addMcp.error.message : String(addMcp.error)}
            </p>
          )}

          <Button
            onClick={() => addMcp.mutate()}
            disabled={addMcp.isPending || !mcpConfig.name.trim()}
          >
            {t("capabilities.addMcpServer")}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.registeredMcpServers")}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="mb-4 flex items-center justify-between gap-4">
            <p className="text-muted-foreground text-sm">{t("capabilities.mcpListBody")}</p>
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
          ) : mcpServers.length ? (
            <ul className="divide-y">
              {mcpServers.map((server) => (
                <li key={server.id} className="flex items-center justify-between gap-4 py-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-medium">{server.name}</span>
                      <span className="text-muted-foreground text-sm">{server.transport}</span>
                      <span
                        className={`text-xs ${server.enabled ? "text-green-600" : "text-amber-600"}`}
                      >
                        {server.enabled ? t("common.enabled") : t("common.disabled")}
                      </span>
                    </div>
                    <p className="text-muted-foreground truncate text-xs">
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
                  </div>
                  <Button
                    variant="destructive"
                    size="sm"
                    onClick={() => {
                      if (window.confirm(t("capabilities.confirmRemoveMcp"))) {
                        removeMcp.mutate(server.id);
                      }
                    }}
                    disabled={removeMcp.isPending}
                  >
                    {t("capabilities.remove")}
                  </Button>
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
