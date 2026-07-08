import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  addMcpServer,
  enablePlugin,
  installPlugin,
  listMcpServers,
  listPlugins,
  removeMcpServer,
  uninstallPlugin,
} from "@/lib/tauri-api";
import {
  asPluginId,
  mcpServerConfigSchema,
  pluginManifestSchema,
  type McpServerConfig,
  type McpTransport,
  type PluginPermissions,
} from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { mcpKeys, pluginKeys } from "@/lib/query-keys";

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

export function PluginCapabilitiesPanel() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  const [manifestJson, setManifestJson] = useState("");
  const [manifestError, setManifestError] = useState<string | null>(null);

  const [expandedPlugin, setExpandedPlugin] = useState<string | null>(null);

  const [mcpConfig, setMcpConfig] = useState<Omit<McpServerConfig, "id">>(defaultMcpConfig());
  const [envKey, setEnvKey] = useState("");
  const [envValue, setEnvValue] = useState("");

  const { data: plugins, isLoading: pluginsLoading } = useQuery({
    queryKey: pluginKeys.list(),
    queryFn: listPlugins,
  });

  const { data: mcpServers, isLoading: mcpServersLoading } = useQuery({
    queryKey: mcpKeys.list(),
    queryFn: listMcpServers,
  });

  const install = useMutation({
    mutationFn: async () => {
      setManifestError(null);
      let parsed: unknown;
      try {
        parsed = JSON.parse(manifestJson) as unknown;
      } catch (err) {
        throw new Error(err instanceof Error ? err.message : "Invalid JSON");
      }
      const result = pluginManifestSchema.safeParse(parsed);
      if (!result.success) {
        throw new Error(result.error.errors.map((e) => e.message).join("; "));
      }
      return installPlugin(result.data);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: pluginKeys.list() });
      setManifestJson("");
    },
    onError: (err) => {
      setManifestError(err instanceof Error ? err.message : String(err));
    },
  });

  const toggleEnable = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      enablePlugin(asPluginId(id), enabled),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: pluginKeys.list() });
    },
  });

  const uninstall = useMutation({
    mutationFn: (id: string) => uninstallPlugin(asPluginId(id)),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: pluginKeys.list() });
    },
  });

  const addMcp = useMutation({
    mutationFn: () => {
      const result = mcpServerConfigSchema.omit({ id: true }).safeParse(mcpConfig);
      if (!result.success) {
        throw new Error(result.error.errors.map((e) => e.message).join("; "));
      }
      return addMcpServer(result.data);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: mcpKeys.list() });
      setMcpConfig(defaultMcpConfig());
      setEnvKey("");
      setEnvValue("");
    },
  });

  const removeMcp = useMutation({
    mutationFn: (id: number) => removeMcpServer(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: mcpKeys.list() });
    },
  });

  const permissionsForDisplay = (permissions: PluginPermissions) => (
    <div className="text-muted-foreground space-y-1 text-sm">
      <p>
        {t("capabilities.filesystem")}{" "}
        <span className="text-foreground font-medium">{permissions.filesystem}</span>
      </p>
      <p>
        {t("capabilities.network")}{" "}
        {permissions.network.length ? permissions.network.join(", ") : t("common.none")}
      </p>
    </div>
  );

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.installPlugin")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-muted-foreground text-sm">
            {t("capabilities.installPluginBody")}
          </p>
          <Textarea
            placeholder='{"id":"...","name":"...","version":"...",...}'
            value={manifestJson}
            onChange={(e) => setManifestJson(e.target.value)}
            rows={6}
          />
          {manifestError && <p className="text-destructive text-sm">{manifestError}</p>}
          <Button
            onClick={() => install.mutate()}
            disabled={install.isPending || !manifestJson.trim()}
          >
            {t("capabilities.install")}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.installedPlugins")}</CardTitle>
        </CardHeader>
        <CardContent>
          {pluginsLoading ? (
            <p className="text-muted-foreground">{t("capabilities.loadingPlugins")}</p>
          ) : plugins?.length ? (
            <ul className="divide-y">
              {plugins.map((plugin) => {
                const isExpanded = expandedPlugin === plugin.id;
                return (
                  <li key={plugin.id} className="py-4">
                    <div className="flex items-start justify-between gap-4">
                      <div className="flex-1">
                        <div className="flex items-center gap-2">
                          <span className="font-medium">{plugin.display_name}</span>
                          <span className="text-muted-foreground text-xs">
                            {plugin.name}@{plugin.version}
                          </span>
                          {plugin.enabled ? (
                            <span className="text-xs text-green-600">{t("common.enabled")}</span>
                          ) : (
                            <span className="text-xs text-amber-600">{t("common.disabled")}</span>
                          )}
                        </div>
                        <p className="text-muted-foreground text-sm">{plugin.description}</p>
                        <div className="text-muted-foreground mt-1 flex flex-wrap gap-2 text-xs">
                          <span>
                            {plugin.skills.length} {t("capabilities.skillsCount")}
                          </span>
                          <span>
                            {plugin.tools.length} {t("capabilities.toolsCount")}
                          </span>
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setExpandedPlugin(isExpanded ? null : plugin.id)}
                        >
                          {isExpanded ? t("capabilities.hide") : t("capabilities.permissions")}
                        </Button>
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() =>
                            toggleEnable.mutate({ id: plugin.id, enabled: !plugin.enabled })
                          }
                          disabled={toggleEnable.isPending}
                        >
                          {plugin.enabled ? t("capabilities.disable") : t("capabilities.enable")}
                        </Button>
                        <Button
                          variant="destructive"
                          size="sm"
                          onClick={() => uninstall.mutate(plugin.id)}
                          disabled={uninstall.isPending}
                        >
                          {t("capabilities.uninstall")}
                        </Button>
                      </div>
                    </div>
                    {isExpanded && (
                      <div className="bg-muted mt-3 rounded-md p-3">
                        <p className="text-sm font-medium">{t("capabilities.permissions")}</p>
                        {permissionsForDisplay(plugin.permissions)}
                        <div className="text-muted-foreground mt-2 text-sm">
                          <p>
                            {t("capabilities.skills")}:{" "}
                            {plugin.skills.join(", ") || t("common.none")}
                          </p>
                          <p>
                            {t("capability.tools")}: {plugin.tools.join(", ") || t("common.none")}
                          </p>
                        </div>
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          ) : (
            <p className="text-muted-foreground">{t("capabilities.noPlugins")}</p>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.mcpServers")}</CardTitle>
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

          <Button
            onClick={() => addMcp.mutate()}
            disabled={addMcp.isPending || !mcpConfig.name.trim()}
          >
            {t("capabilities.addMcpServer")}
          </Button>

          {addMcp.error && (
            <p className="text-destructive text-sm">
              {addMcp.error instanceof Error ? addMcp.error.message : String(addMcp.error)}
            </p>
          )}

          <div className="pt-4">
            {mcpServersLoading ? (
              <p className="text-muted-foreground">{t("capabilities.loadingMcpServers")}</p>
            ) : mcpServers?.length ? (
              <ul className="divide-y">
                {mcpServers.map((server) => (
                  <li key={server.id} className="flex items-center justify-between py-3">
                    <div>
                      <span className="font-medium">{server.name}</span>
                      <span className="text-muted-foreground ml-2 text-sm">{server.transport}</span>
                      <span
                        className={`ml-2 text-xs ${server.enabled ? "text-green-600" : "text-amber-600"}`}
                      >
                        {server.enabled ? t("common.enabled") : t("common.disabled")}
                      </span>
                      <p className="text-muted-foreground text-xs">
                        {server.transport === "Stdio"
                          ? [server.command, ...server.args].filter(Boolean).join(" ")
                          : server.url}
                      </p>
                    </div>
                    <Button
                      variant="destructive"
                      size="sm"
                      onClick={() => removeMcp.mutate(server.id)}
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
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
