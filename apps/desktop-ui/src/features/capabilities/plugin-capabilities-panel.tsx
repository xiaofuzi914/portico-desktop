import { useEffect, useRef, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { FolderOpen, Github, HardDrive, Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  enablePlugin,
  getPorticoUserDirs,
  installCatalogPlugin,
  installPluginFromGithub,
  installPluginPackage,
  listPluginSources,
  listPlugins,
  openPorticoPluginsDir,
  seedCatalogPlugins,
  uninstallPlugin,
  type PluginSourceEntry,
} from "@/lib/tauri-api";
import { asPluginId, type PluginManifest } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { pluginKeys } from "@/lib/query-keys";
import { BUNDLED_PLUGIN_CATALOG, findCatalogEntry } from "./plugin-catalog";
import { cn } from "@/lib/utils";

const MARKDOWN_PROVIDER_STORAGE_KEY = "portico.markdownProvider";
const PLUGIN_INSTALL_PROGRESS_EVENT = "plugin-install-progress";

interface InstallLogLine {
  id: string;
  phase: string;
  message: string;
  level: string;
  ts: number;
}

function sourceForPlugin(
  plugin: PluginManifest,
  sources: PluginSourceEntry[],
): PluginSourceEntry | undefined {
  return sources.find(
    (s) => s.id === plugin.id || s.name === plugin.name || s.id === (plugin.id as string),
  );
}

function latestHint(
  plugin: PluginManifest,
  sources: PluginSourceEntry[],
): { label: string; canUpdate: boolean; source?: PluginSourceEntry } {
  const src = sourceForPlugin(plugin, sources);
  // Catalog display name often embeds version; prefer github recipe package version from installed compare.
  // For markdown-viewer we treat source as "update via github/local closed-loop".
  if (src?.github) {
    // Without remote fetch, "latest" means reinstall from registered source (always offered).
    return {
      label: src.github,
      canUpdate: true,
      source: src,
    };
  }
  if (src?.kind === "portico-package") {
    return { label: src.name, canUpdate: true, source: src };
  }
  return { label: "", canUpdate: false };
}

export function PluginCapabilitiesPanel() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [githubSource, setGithubSource] = useState(
    "https://github.com/markdown-viewer/markdown-viewer-extension",
  );
  const [installPhase, setInstallPhase] = useState<string | null>(null);
  const [installLog, setInstallLog] = useState<InstallLogLine[]>([]);
  const logEndRef = useRef<HTMLDivElement>(null);
  const logBoxRef = useRef<HTMLDivElement>(null);

  // Live install progress from Rust (scrollable so long builds don't look frozen).
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<{ phase: string; message: string; ts: number; level: string }>(
      PLUGIN_INSTALL_PROGRESS_EVENT,
      (event) => {
        const p = event.payload;
        setInstallLog((prev) => [
          ...prev,
          {
            id: `${p.ts}-${prev.length}-${Math.random().toString(36).slice(2, 7)}`,
            phase: p.phase,
            message: p.message,
            level: p.level || "info",
            ts: p.ts,
          },
        ]);
      },
    ).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ block: "end", behavior: "smooth" });
  }, [installLog]);

  const { data: plugins = [], isLoading: pluginsLoading } = useQuery({
    queryKey: pluginKeys.list(),
    queryFn: listPlugins,
  });

  const { data: userDirs } = useQuery({
    queryKey: pluginKeys.userDirs(),
    queryFn: getPorticoUserDirs,
  });

  useQuery({
    queryKey: pluginKeys.available(),
    queryFn: async () => {
      try {
        await seedCatalogPlugins();
      } catch {
        /* ignore */
      }
      return null;
    },
  });

  const { data: pluginSources = [] } = useQuery({
    queryKey: [...pluginKeys.list(), "sources"],
    queryFn: listPluginSources,
  });

  const clearFeedback = (opts?: { keepLog?: boolean }) => {
    setActionError(null);
    setActionMessage(null);
    setInstallPhase(null);
    if (!opts?.keepLog) setInstallLog([]);
  };

  const pushLocalLog = (phase: string, message: string, level = "info") => {
    setInstallLog((prev) => [
      ...prev,
      {
        id: `${Date.now()}-${prev.length}`,
        phase,
        message,
        level,
        ts: Date.now(),
      },
    ]);
  };

  const afterInstall = async (plugin: PluginManifest) => {
    await queryClient.invalidateQueries({ queryKey: pluginKeys.list() });
    if (plugin.capabilities.includes("markdown.preview")) {
      localStorage.setItem(MARKDOWN_PROVIDER_STORAGE_KEY, plugin.id);
    }
    setInstallPhase(null);
    pushLocalLog("done", `${plugin.display_name} v${plugin.version}`, "ok");
    setActionMessage(
      `${t("capabilities.pluginInstallSuccess")}: ${plugin.display_name} v${plugin.version}`,
    );
  };

  /** Fast local/catalog install — never starts multi-minute GitHub build. */
  const installLocalCatalog = useMutation({
    mutationFn: async (packageName: string) => {
      clearFeedback();
      setInstallPhase(t("capabilities.installPhaseLocal"));
      pushLocalLog("ui", t("capabilities.installPhaseLocal"));
      return installCatalogPlugin(packageName);
    },
    onSuccess: (plugin) => {
      void afterInstall(plugin);
    },
    onError: (err) => {
      setInstallPhase(null);
      const msg = err instanceof Error ? err.message : String(err);
      pushLocalLog("error", msg, "error");
      setActionError(msg);
    },
  });

  const installFromGithub = useMutation({
    mutationFn: async (source: string) => {
      clearFeedback();
      setInstallPhase(t("capabilities.installPhaseGithub"));
      pushLocalLog("ui", t("capabilities.installPhaseGithub"));
      pushLocalLog("ui", source.trim());
      return installPluginFromGithub(source.trim());
    },
    onSuccess: (plugin) => {
      void afterInstall(plugin);
    },
    onError: (err) => {
      setInstallPhase(null);
      const msg = err instanceof Error ? err.message : String(err);
      pushLocalLog("error", msg, "error");
      setActionError(msg);
    },
  });

  const installCustom = useMutation({
    mutationFn: async () => {
      clearFeedback();
      setInstallPhase(t("capabilities.installPhaseLocal"));
      pushLocalLog("ui", t("capabilities.installPhaseLocal"));
      const selection = await openDialog({
        directory: true,
        multiple: false,
        defaultPath: userDirs?.plugins_available,
      });
      if (!selection || Array.isArray(selection)) {
        setInstallPhase(null);
        pushLocalLog("ui", t("capabilities.installCancelled"), "warn");
        return null;
      }
      pushLocalLog("ui", String(selection));
      return installPluginPackage(selection);
    },
    onSuccess: (plugin) => {
      if (!plugin) {
        setInstallPhase(null);
        return;
      }
      void afterInstall(plugin);
    },
    onError: (err) => {
      setInstallPhase(null);
      const msg = err instanceof Error ? err.message : String(err);
      pushLocalLog("error", msg, "error");
      setActionError(msg);
    },
  });

  const toggleEnable = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      enablePlugin(asPluginId(id), enabled),
    onSuccess: (_void, vars) => {
      void queryClient.invalidateQueries({ queryKey: pluginKeys.list() });
      if (!vars.enabled && localStorage.getItem(MARKDOWN_PROVIDER_STORAGE_KEY) === vars.id) {
        localStorage.setItem(MARKDOWN_PROVIDER_STORAGE_KEY, "builtin");
      }
    },
    onError: (err) => {
      setActionError(err instanceof Error ? err.message : String(err));
    },
  });

  const uninstall = useMutation({
    mutationFn: async (id: string) => {
      clearFeedback();
      await uninstallPlugin(asPluginId(id));
      return id;
    },
    onSuccess: async (id) => {
      await queryClient.invalidateQueries({ queryKey: pluginKeys.list() });
      if (localStorage.getItem(MARKDOWN_PROVIDER_STORAGE_KEY) === id) {
        localStorage.setItem(MARKDOWN_PROVIDER_STORAGE_KEY, "builtin");
      }
      setActionMessage(
        `${t("capabilities.pluginUninstallSuccess")} (${t("capabilities.pluginFilesRemoved")})`,
      );
    },
    onError: (err) => {
      setActionError(err instanceof Error ? err.message : String(err));
    },
  });

  const busy =
    installFromGithub.isPending ||
    installLocalCatalog.isPending ||
    installCustom.isPending;

  return (
    <div className="space-y-6">
      {/* ——— Section 1: Install ——— */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle>{t("capabilities.pluginInstallSection")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4 text-sm">
          <p className="text-muted-foreground">{t("capabilities.pluginInstallSectionBody")}</p>
          {userDirs && (
            <div className="text-muted-foreground flex flex-wrap items-center gap-2 text-xs">
              <code className="bg-muted max-w-full truncate rounded px-1.5 py-0.5">
                {userDirs.plugins}
              </code>
              <Button
                size="sm"
                variant="ghost"
                className="h-7 px-2"
                onClick={() => void openPorticoPluginsDir("available")}
              >
                <FolderOpen className="mr-1 h-3.5 w-3.5" />
                {t("capabilities.openAvailableDir")}
              </Button>
            </div>
          )}

          {/* 1a GitHub */}
          <div className="space-y-2 rounded-md border p-3">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Github className="h-4 w-4" />
              {t("capabilities.installFromGithub")}
            </div>
            <p className="text-muted-foreground text-xs">{t("capabilities.installFromGithubBody")}</p>
            <div className="flex flex-col gap-2 sm:flex-row">
              <input
                className="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex h-9 w-full rounded-md border px-3 text-sm focus-visible:ring-2 focus-visible:outline-none"
                value={githubSource}
                onChange={(e) => setGithubSource(e.target.value)}
                placeholder="https://github.com/owner/repo"
                spellCheck={false}
                disabled={busy}
              />
              <Button
                size="sm"
                className="shrink-0 gap-1.5"
                disabled={busy || !githubSource.trim()}
                onClick={() => installFromGithub.mutate(githubSource)}
              >
                {installFromGithub.isPending ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Github className="h-3.5 w-3.5" />
                )}
                {installFromGithub.isPending
                  ? t("capabilities.installingFromGithub")
                  : t("capabilities.installFromGithubAction")}
              </Button>
            </div>
            {pluginSources.some((s) => s.github) && (
              <p className="text-muted-foreground text-[11px]">
                {t("capabilities.knownGithubSources")}:{" "}
                {pluginSources
                  .filter((s) => s.github)
                  .map((s) => (
                    <button
                      key={s.name}
                      type="button"
                      className="text-foreground underline-offset-2 hover:underline"
                      disabled={busy}
                      onClick={() => setGithubSource(s.github || "")}
                    >
                      {s.display_name || s.github}
                    </button>
                  ))
                  .reduce<React.ReactNode[]>((acc, node, i) => {
                    if (i > 0) acc.push(" · ");
                    acc.push(node);
                    return acc;
                  }, [])}
              </p>
            )}
          </div>

          {/* 1b Local */}
          <div className="space-y-2 rounded-md border p-3">
            <div className="flex items-center gap-2 text-sm font-medium">
              <HardDrive className="h-4 w-4" />
              {t("capabilities.installFromLocal")}
            </div>
            <p className="text-muted-foreground text-xs">{t("capabilities.installFromLocalBody")}</p>
            <div className="flex flex-wrap gap-2">
              <Button
                size="sm"
                variant="outline"
                className="gap-1.5"
                disabled={busy}
                onClick={() => installCustom.mutate()}
              >
                {installCustom.isPending ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <FolderOpen className="h-3.5 w-3.5" />
                )}
                {t("capabilities.installPluginPackage")}
              </Button>
              {/* Quick local catalog packages (monorepo / available) */}
              {BUNDLED_PLUGIN_CATALOG.map((entry) => (
                <Button
                  key={entry.id}
                  size="sm"
                  variant="outline"
                  disabled={busy}
                  onClick={() => installLocalCatalog.mutate(entry.name)}
                  title={t("capabilities.installLocalCatalogHint")}
                >
                  {installLocalCatalog.isPending && installLocalCatalog.variables === entry.name
                    ? t("common.loading")
                    : `${t("capabilities.install")}: ${t(entry.displayNameKey)}`}
                </Button>
              ))}
            </div>
          </div>

          {(installPhase || installLog.length > 0) && (
            <div className="space-y-2">
              {installPhase && (
                <p className="text-muted-foreground flex items-center gap-2 text-xs">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {installPhase}
                </p>
              )}
              <div
                ref={logBoxRef}
                className="bg-muted/40 max-h-56 overflow-y-auto rounded-md border px-3 py-2 font-mono text-[11px] leading-5"
                aria-live="polite"
                aria-label={t("capabilities.installLog")}
              >
                {installLog.length === 0 ? (
                  <p className="text-muted-foreground">{t("capabilities.installLogWaiting")}</p>
                ) : (
                  installLog.map((line) => (
                    <div
                      key={line.id}
                      className={cn(
                        "whitespace-pre-wrap break-all",
                        line.level === "error" && "text-destructive",
                        line.level === "ok" && "text-green-700",
                        line.level === "warn" && "text-amber-700",
                        line.level === "info" && "text-muted-foreground",
                      )}
                    >
                      <span className="text-foreground/70">[{line.phase}]</span> {line.message}
                    </div>
                  ))
                )}
                <div ref={logEndRef} />
              </div>
            </div>
          )}
          {actionError && <p className="text-destructive text-sm whitespace-pre-wrap">{actionError}</p>}
          {actionMessage && <p className="text-sm text-green-700">{actionMessage}</p>}
        </CardContent>
      </Card>

      {/* ——— Section 2: Installed + version ——— */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle>{t("capabilities.installedPluginsSection")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <p className="text-muted-foreground text-sm">{t("capabilities.installedPluginsSectionBody")}</p>
          {pluginsLoading ? (
            <p className="text-muted-foreground text-sm">{t("common.loading")}</p>
          ) : plugins.length === 0 ? (
            <p className="text-muted-foreground text-sm">{t("capabilities.noPlugins")}</p>
          ) : (
            <ul className="divide-y rounded-md border">
              {plugins.map((plugin) => {
                const catalog = findCatalogEntry(plugin.id);
                const update = latestHint(plugin, pluginSources);
                return (
                  <li
                    key={plugin.id}
                    className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between"
                  >
                    <div className="min-w-0 space-y-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-medium">
                          {catalog ? t(catalog.displayNameKey) : plugin.display_name}
                        </span>
                        <span
                          className="bg-muted text-foreground rounded px-1.5 py-0.5 font-mono text-[11px] tabular-nums"
                          title={t("capabilities.installedVersion")}
                        >
                          v{plugin.version}
                        </span>
                        <span className="text-xs text-green-600">
                          {plugin.enabled
                            ? t("capabilities.catalogInstalledEnabled")
                            : t("capabilities.catalogInstalledDisabled")}
                        </span>
                      </div>
                      <p className="text-muted-foreground text-sm line-clamp-2">
                        {plugin.description}
                      </p>
                      {plugin.capabilities?.length > 0 && (
                        <div className="flex flex-wrap gap-1">
                          {plugin.capabilities.map((cap) => (
                            <span
                              key={cap}
                              className="bg-muted text-muted-foreground rounded px-1.5 py-0.5 text-[10px]"
                            >
                              {cap}
                            </span>
                          ))}
                        </div>
                      )}
                      {update.canUpdate && update.source?.github && (
                        <p className="text-muted-foreground text-[11px]">
                          {t("capabilities.updateSource")}: {update.source.github}
                        </p>
                      )}
                    </div>
                    <div className="flex shrink-0 flex-wrap gap-2">
                      {update.canUpdate && (
                        <Button
                          size="sm"
                          variant="outline"
                          className="gap-1"
                          disabled={busy}
                          title={t("capabilities.checkUpdateHint")}
                          onClick={() => {
                            if (update.source?.github) {
                              installFromGithub.mutate(update.source.github);
                            } else if (update.source?.name) {
                              installLocalCatalog.mutate(update.source.name);
                            } else if (catalog) {
                              installLocalCatalog.mutate(catalog.name);
                            }
                          }}
                        >
                          <RefreshCw className="h-3.5 w-3.5" />
                          {t("capabilities.updateToLatest")}
                        </Button>
                      )}
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={toggleEnable.isPending}
                        onClick={() =>
                          toggleEnable.mutate({
                            id: plugin.id,
                            enabled: !plugin.enabled,
                          })
                        }
                      >
                        {plugin.enabled ? t("capabilities.disable") : t("capabilities.enable")}
                      </Button>
                      <Button
                        variant="destructive"
                        size="sm"
                        disabled={uninstall.isPending}
                        onClick={() => {
                          if (window.confirm(t("capabilities.confirmUninstall"))) {
                            uninstall.mutate(plugin.id);
                          }
                        }}
                      >
                        {t("capabilities.uninstall")}
                      </Button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
