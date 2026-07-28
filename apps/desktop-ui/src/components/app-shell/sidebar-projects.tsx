import { Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Folder, FolderOpen, Trash2 } from "lucide-react";
import { deleteWorkspace, listWorkspaces } from "@/lib/tauri-api";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";
import { buildSidebarProjectItems } from "./sidebar-projects-model";
import { projectAbbreviation } from "./project-abbreviation";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { WorkspaceId } from "@/lib/schemas";
import {
  readRunningWorkspaceIds,
  subscribeWorkspaceRunActivityChanged,
} from "./workspace-activity-store";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";

const projectLastUsedStorageKey = "portico.sidebarProjectLastUsedAt";

interface SidebarProjectsProps {
  activeWorkspaceId?: WorkspaceId;
  /** Icon / abbreviation-only chips for the collapsed sidebar. */
  compact?: boolean;
}

type ContextMenuState = Readonly<{
  workspaceId: WorkspaceId;
  name: string;
  x: number;
  y: number;
}>;

export function SidebarProjects({ activeWorkspaceId, compact = false }: SidebarProjectsProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const menuRef = useRef<HTMLDivElement>(null);
  const [lastUsedAtById, setLastUsedAtById] = useState<Record<string, string>>(() =>
    readProjectLastUsedAt(),
  );
  const [runningWorkspaceIds, setRunningWorkspaceIds] = useState<Set<WorkspaceId>>(() =>
    readRunningWorkspaceIds(),
  );
  const [menu, setMenu] = useState<ContextMenuState | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Readonly<{
    workspaceId: WorkspaceId;
    name: string;
  }> | null>(null);

  const { data: workspaces, isLoading } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });

  const remove = useMutation({
    mutationFn: (id: WorkspaceId) => deleteWorkspace(id),
    onSuccess: async (_data, deletedId) => {
      setMenu(null);
      setLastUsedAtById((current) => {
        const next = { ...current };
        delete next[deletedId];
        writeProjectLastUsedAt(next);
        return next;
      });
      // Drop caches scoped to this project id, then refresh the project list.
      queryClient.removeQueries({
        predicate: (query) => query.queryKey.includes(deletedId),
      });
      await queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });

      if (activeWorkspaceId === deletedId) {
        void navigate({ to: "/workspaces" });
      }
    },
  });

  useEffect(() => {
    if (!activeWorkspaceId) return;
    setLastUsedAtById((current) => {
      const next = { ...current, [activeWorkspaceId]: new Date().toISOString() };
      writeProjectLastUsedAt(next);
      return next;
    });
  }, [activeWorkspaceId]);

  const lastUsedAtMap = useMemo(() => new Map(Object.entries(lastUsedAtById)), [lastUsedAtById]);

  useEffect(
    () =>
      subscribeWorkspaceRunActivityChanged(() => {
        setRunningWorkspaceIds(readRunningWorkspaceIds());
      }),
    [],
  );

  useEffect(() => {
    if (!menu) return;
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (menuRef.current?.contains(target)) return;
      setMenu(null);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenu(null);
    };
    const onScroll = () => setMenu(null);
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [menu]);

  function openContextMenu(
    event: React.MouseEvent,
    workspaceId: WorkspaceId,
    name: string,
  ) {
    event.preventDefault();
    event.stopPropagation();
    const pad = 8;
    const menuW = 168;
    const menuH = 48;
    const x = Math.min(event.clientX, window.innerWidth - menuW - pad);
    const y = Math.min(event.clientY, window.innerHeight - menuH - pad);
    setMenu({
      workspaceId,
      name,
      x: Math.max(pad, x),
      y: Math.max(pad, y),
    });
  }

  if (isLoading) {
    if (compact) return null;
    return <p className="text-muted-foreground px-2 text-sm">{t("sidebar.loadingProjects")}</p>;
  }

  if (!workspaces?.length) {
    if (compact) return null;
    return <p className="text-muted-foreground px-2 text-sm">{t("sidebar.noProjects")}</p>;
  }

  const projectItems = buildSidebarProjectItems(workspaces, {
    lastUsedAtByWorkspaceId: lastUsedAtMap,
    runningWorkspaceIds,
  });

  const contextMenu = menu
    ? createPortal(
        <div
          ref={menuRef}
          role="menu"
          aria-label={t("projects.contextMenu")}
          className="bg-background text-foreground border-border fixed z-[100] min-w-[10.5rem] rounded-md border py-1 shadow-lg"
          style={{ left: menu.x, top: menu.y }}
        >
          <button
            type="button"
            role="menuitem"
            className="text-destructive hover:bg-destructive/10 flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs"
            disabled={remove.isPending}
            onClick={() => {
              setPendingDelete({ workspaceId: menu.workspaceId, name: menu.name });
              setMenu(null);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t("projects.delete")}
          </button>
        </div>,
        document.body,
      )
    : null;

  const confirmDialog = (
    <ConfirmDialog
      open={pendingDelete !== null}
      title={t("projects.delete")}
      description={
        pendingDelete
          ? t("projects.deleteConfirmNamed").replace("{name}", pendingDelete.name)
          : undefined
      }
      confirmLabel={t("projects.delete")}
      destructive
      onConfirm={() => {
        if (pendingDelete) {
          remove.mutate(pendingDelete.workspaceId);
        }
        setPendingDelete(null);
      }}
      onCancel={() => setPendingDelete(null)}
    />
  );

  if (compact) {
    return (
      <>
        <ul className="flex w-full flex-col items-center gap-1">
          {projectItems.map((item) =>
            item.kind === "overview" ? (
              <li key="overview">
                <Link
                  to="/workspaces"
                  title={t("projects.allProjects")}
                  aria-label={t("projects.allProjects")}
                  className="text-muted-foreground hover:bg-sidebar-accent hover:text-foreground flex h-8 w-8 items-center justify-center rounded-md transition-colors"
                  activeProps={{
                    className:
                      "flex h-8 w-8 items-center justify-center rounded-md bg-sidebar-accent text-foreground",
                  }}
                >
                  <FolderOpen className="h-3.5 w-3.5" />
                </Link>
              </li>
            ) : (
              <li key={item.workspace.id} className="relative">
                <Link
                  to="/workspaces/$workspaceId"
                  params={{ workspaceId: item.workspace.id }}
                  title={item.workspace.name}
                  aria-label={item.workspace.name}
                  className="text-muted-foreground hover:bg-sidebar-accent hover:text-foreground flex h-8 w-8 items-center justify-center rounded-md text-[10px] font-semibold tracking-tight transition-colors"
                  activeProps={{
                    className:
                      "flex h-8 w-8 items-center justify-center rounded-md bg-sidebar-accent text-foreground text-[10px] font-semibold tracking-tight",
                  }}
                  onContextMenu={(event) =>
                    openContextMenu(event, item.workspace.id, item.workspace.name)
                  }
                >
                  {projectAbbreviation(item.workspace.name)}
                </Link>
                {item.isRunning && (
                  <span className="absolute top-0.5 right-0.5 h-1.5 w-1.5 rounded-full bg-success" />
                )}
              </li>
            ),
          )}
        </ul>
        {contextMenu}
        {confirmDialog}
        {remove.isError ? (
          <p className="text-destructive max-w-[4.5rem] px-0.5 text-center text-[10px] leading-snug">
            {remove.error instanceof Error ? remove.error.message : String(remove.error)}
          </p>
        ) : null}
      </>
    );
  }

  return (
    <div onContextMenu={(event) => event.preventDefault()}>
      <ul className="space-y-0.5">
        {projectItems.map((item) =>
          item.kind === "overview" ? (
            <li key="overview">
              <Link
                to="/workspaces"
                className="text-muted-foreground hover:bg-sidebar-accent hover:text-foreground flex h-8 items-center gap-2 rounded-md px-2 text-sm transition-colors"
                activeProps={{
                  className:
                    "flex h-8 items-center gap-2 rounded-md px-2 text-sm bg-sidebar-accent font-medium text-foreground",
                }}
              >
                <FolderOpen className="h-3.5 w-3.5 shrink-0" />
                <span className="min-w-0 flex-1 truncate">{t("projects.allProjects")}</span>
                <span className="text-muted-foreground text-xs tabular-nums">
                  {item.overflowCount}
                </span>
              </Link>
            </li>
          ) : (
            <li key={item.workspace.id}>
              <Link
                to="/workspaces/$workspaceId"
                params={{ workspaceId: item.workspace.id }}
                className="text-muted-foreground hover:bg-sidebar-accent hover:text-foreground flex h-8 items-center gap-2 rounded-md px-2 text-sm transition-colors"
                activeProps={{
                  className:
                    "flex h-8 items-center gap-2 rounded-md px-2 text-sm bg-sidebar-accent font-medium text-foreground",
                }}
                title={t("projects.projectMenuHint")}
                onContextMenu={(event) =>
                  openContextMenu(event, item.workspace.id, item.workspace.name)
                }
              >
                <Folder className="h-3.5 w-3.5 shrink-0" />
                <span className="min-w-0 flex-1 truncate">{item.workspace.name}</span>
                {item.isRunning && (
                  <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-success" />
                )}
              </Link>
            </li>
          ),
        )}
      </ul>
      {contextMenu}
      {confirmDialog}
      {remove.isError ? (
        <p className="text-destructive px-2 py-1 text-[11px] leading-snug">
          {remove.error instanceof Error ? remove.error.message : String(remove.error)}
        </p>
      ) : null}
    </div>
  );
}

function readProjectLastUsedAt(): Record<string, string> {
  if (typeof window === "undefined") return {};
  try {
    const rawValue = window.localStorage.getItem(projectLastUsedStorageKey);
    if (!rawValue) return {};
    const parsedValue = JSON.parse(rawValue) as unknown;
    if (!parsedValue || typeof parsedValue !== "object" || Array.isArray(parsedValue)) return {};
    return Object.fromEntries(
      Object.entries(parsedValue).filter((entry): entry is [string, string] => {
        const [key, value] = entry;
        return typeof key === "string" && typeof value === "string";
      }),
    );
  } catch {
    return {};
  }
}

function writeProjectLastUsedAt(value: Record<string, string>) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(projectLastUsedStorageKey, JSON.stringify(value));
  } catch {
    // Sidebar ordering is a convenience; storage failures should not break navigation.
  }
}
