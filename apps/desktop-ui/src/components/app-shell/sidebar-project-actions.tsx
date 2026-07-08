import { useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { FilePlus, FolderOpen, Plus } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { createWorkspace } from "@/lib/tauri-api";
import { deriveProjectNameFromPath, normalizeDirectorySelection } from "@/lib/path-picker";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";

export function SidebarProjectActions() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const addExistingProject = useMutation({
    mutationFn: async () => {
      setError(null);
      const selection = await openDialog({
        directory: true,
        multiple: false,
        title: t("projects.openExistingProject"),
      });
      const rootPath = normalizeDirectorySelection(selection);
      if (!rootPath) return null;
      return createWorkspace(deriveProjectNameFromPath(rootPath), rootPath, false);
    },
    onSuccess: (workspace) => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
      setOpen(false);
      if (workspace) {
        void navigate({ to: "/workspaces/$workspaceId", params: { workspaceId: workspace.id } });
      }
    },
    onError: (err) => {
      setError(err instanceof Error ? err.message : String(err));
    },
  });

  return (
    <div className="relative">
      <button
        type="button"
        className="hover:bg-sidebar-accent hover:text-foreground flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors"
        aria-label={t("projects.addProject")}
        onClick={() => {
          setError(null);
          setOpen((current) => !current);
        }}
      >
        <Plus className="h-3.5 w-3.5" />
      </button>

      {open && (
        <div className="bg-background absolute top-7 right-0 z-20 w-56 rounded-lg border p-1 shadow-lg">
          <button
            type="button"
            className="hover:bg-muted flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm"
            onClick={() => addExistingProject.mutate()}
            disabled={addExistingProject.isPending}
          >
            <FolderOpen className="h-4 w-4 shrink-0" />
            <span className="min-w-0 flex-1 truncate">{t("projects.openExistingProject")}</span>
          </button>
          <button
            type="button"
            className="hover:bg-muted flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm"
            onClick={() => {
              setOpen(false);
              void navigate({ to: "/workspaces", search: { mode: "new" } });
            }}
          >
            <FilePlus className="h-4 w-4 shrink-0" />
            <span className="min-w-0 flex-1 truncate">{t("projects.newProject")}</span>
          </button>
          {error && <p className="text-destructive px-2 py-1 text-xs leading-5">{error}</p>}
        </div>
      )}
    </div>
  );
}
