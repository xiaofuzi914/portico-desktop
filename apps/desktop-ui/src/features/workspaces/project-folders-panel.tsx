import { useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderPlus, FolderTree, Star, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ErrorAlert } from "@/components/ui/error-alert";
import {
  addLinkedProjectFolder,
  removeLinkedProjectFolder,
  trustWorkspace,
} from "@/lib/tauri-api";
import type { Workspace, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";
import { normalizeDirectorySelection } from "@/lib/path-picker";
import { typography } from "@/components/ui/typography";
import { cn } from "@/lib/utils";

/** Linked folders = allowlist entries that are not the main root. */
export function linkedFoldersOf(workspace: Workspace): string[] {
  const root = workspace.root_path.replace(/\/+$/, "");
  const set = new Set<string>();
  for (const p of [...workspace.allowed_read_paths, ...workspace.allowed_write_paths]) {
    const n = p.replace(/\/+$/, "");
    if (n && n !== root) set.add(n);
  }
  return [...set].sort((a, b) => a.localeCompare(b));
}

function folderLabel(path: string): string {
  return path.split("/").filter(Boolean).at(-1) ?? path;
}

function normalizePath(path: string): string {
  return path.replace(/\/+$/, "");
}

interface ProjectFoldersPanelProps {
  workspace: Workspace;
  /**
   * `full` — project page card.
   * `compact` — inspector sidebar (selectable roots + add/remove).
   */
  variant?: "full" | "compact";
  /** Currently browsed root (compact). */
  activePath?: string | null;
  /** Select a root to browse (compact). */
  onSelectPath?: (path: string) => void;
}

export function ProjectFoldersPanel({
  workspace,
  variant = "full",
  activePath = null,
  onSelectPath,
}: ProjectFoldersPanelProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const mainRoot = normalizePath(workspace.root_path);
  const linked = useMemo(() => linkedFoldersOf(workspace), [workspace]);
  const active = normalizePath(activePath ?? mainRoot);

  const invalidate = async () => {
    await queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
    // File listings under each root.
    await queryClient.invalidateQueries({ queryKey: workspaceKeys.files(workspace.id) });
  };

  const addFolder = useMutation({
    mutationFn: async () => {
      setError(null);
      if (!workspace.trusted) {
        await trustWorkspace(workspace.id as WorkspaceId, true);
      }
      const selection = await openDialog({
        directory: true,
        multiple: false,
        title: t("folders.pickLinked"),
      });
      const path = normalizeDirectorySelection(selection);
      if (!path) return null;
      return addLinkedProjectFolder(workspace.id as WorkspaceId, path);
    },
    onSuccess: async (ws) => {
      await invalidate();
      // Switch browse to the newly added folder when in compact mode.
      if (ws && onSelectPath) {
        const added = linkedFoldersOf(ws).find((p) => !linked.includes(p));
        if (added) onSelectPath(added);
      }
    },
    onError: (err) => {
      setError(err instanceof Error ? err.message : String(err));
    },
  });

  const removeFolder = useMutation({
    mutationFn: (path: string) =>
      removeLinkedProjectFolder(workspace.id as WorkspaceId, path),
    onSuccess: async (_ws, path) => {
      await invalidate();
      if (onSelectPath && normalizePath(path) === active) {
        onSelectPath(mainRoot);
      }
    },
    onError: (err) => {
      setError(err instanceof Error ? err.message : String(err));
    },
  });

  if (variant === "compact") {
    return (
      <div className="shrink-0 border-b">
        <div className="flex items-center justify-between gap-2 px-3 py-2">
          <p className="text-muted-foreground flex items-center gap-1.5 text-[10px] font-medium tracking-wide uppercase">
            <FolderTree className="h-3 w-3" />
            {t("folders.title")}
          </p>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="h-7 gap-1 px-2 text-[11px]"
            disabled={addFolder.isPending}
            onClick={() => addFolder.mutate()}
          >
            <FolderPlus className="h-3.5 w-3.5" />
            {t("folders.addLinked")}
          </Button>
        </div>
        <ul className="max-h-36 overflow-y-auto px-2 pb-2">
          <li>
            <button
              type="button"
              className={cn(
                "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-xs",
                active === mainRoot ? "bg-muted" : "hover:bg-muted/60",
              )}
              onClick={() => onSelectPath?.(mainRoot)}
              title={workspace.root_path}
            >
              <Star className="text-amber-600 mt-0.5 h-3.5 w-3.5 shrink-0" />
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">
                  {folderLabel(mainRoot)}
                  <span className="text-muted-foreground ml-1 font-normal">
                    · {t("folders.mainRootShort")}
                  </span>
                </span>
                <span className="text-muted-foreground mt-0.5 block truncate font-mono text-[10px]">
                  {workspace.root_path}
                </span>
              </span>
            </button>
          </li>
          {linked.map((path) => (
            <li key={path} className="flex items-center gap-0.5">
              <button
                type="button"
                className={cn(
                  "flex min-w-0 flex-1 items-start gap-2 rounded-md px-2 py-1.5 text-left text-xs",
                  active === normalizePath(path) ? "bg-muted" : "hover:bg-muted/60",
                )}
                onClick={() => onSelectPath?.(path)}
                title={path}
              >
                <FolderTree className="text-muted-foreground mt-0.5 h-3.5 w-3.5 shrink-0" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium">{folderLabel(path)}</span>
                  <span className="text-muted-foreground mt-0.5 block truncate font-mono text-[10px]">
                    {path}
                  </span>
                </span>
              </button>
              <Button
                type="button"
                size="icon"
                variant="ghost"
                className="text-muted-foreground hover:text-destructive h-7 w-7 shrink-0"
                title={t("folders.removeLinked")}
                aria-label={t("folders.removeLinked")}
                disabled={removeFolder.isPending}
                onClick={() => removeFolder.mutate(path)}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </li>
          ))}
        </ul>
        {linked.length === 0 ? (
          <p className="text-muted-foreground px-3 pb-2 text-[11px] leading-4">
            {t("folders.noLinkedShort")}
          </p>
        ) : null}
        {error ? (
          <div className="px-2 pb-2">
            <ErrorAlert title={t("folders.updateFailed")} message={error} />
          </div>
        ) : null}
      </div>
    );
  }

  return (
    <div className="agent-panel overflow-hidden">
      <div className="flex items-center justify-between border-b px-4 py-3">
        <h2 className={`flex items-center gap-2 ${typography.sectionTitle}`}>
          <FolderTree className="h-4 w-4" />
          {t("folders.title")}
        </h2>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={addFolder.isPending}
          onClick={() => addFolder.mutate()}
        >
          <FolderPlus className="h-4 w-4" />
          {t("folders.addLinked")}
        </Button>
      </div>
      <div className="space-y-3 p-4">
        <p className="text-muted-foreground text-xs leading-5">{t("folders.help")}</p>

        <div className="rounded-lg border">
          <div className="bg-muted/40 flex items-start gap-3 border-b px-3 py-2.5">
            <Star className="text-amber-600 mt-0.5 h-4 w-4 shrink-0" />
            <div className="min-w-0 flex-1">
              <p className="text-xs font-semibold tracking-wide uppercase">
                {t("folders.mainRoot")}
              </p>
              <p className="mt-0.5 truncate font-mono text-xs" title={workspace.root_path}>
                {workspace.root_path}
              </p>
            </div>
          </div>

          {linked.length === 0 ? (
            <p className="text-muted-foreground px-3 py-3 text-sm">{t("folders.noLinked")}</p>
          ) : (
            <ul className="divide-y">
              {linked.map((path) => (
                <li key={path} className="flex items-center gap-2 px-3 py-2.5">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{folderLabel(path)}</p>
                    <p className="text-muted-foreground mt-0.5 truncate font-mono text-[11px]">
                      {path}
                    </p>
                  </div>
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    className={cn("text-muted-foreground hover:text-destructive h-8 w-8")}
                    title={t("folders.removeLinked")}
                    aria-label={t("folders.removeLinked")}
                    disabled={removeFolder.isPending}
                    onClick={() => removeFolder.mutate(path)}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>

        {error ? (
          <ErrorAlert title={t("folders.updateFailed")} message={error} />
        ) : null}
      </div>
    </div>
  );
}
