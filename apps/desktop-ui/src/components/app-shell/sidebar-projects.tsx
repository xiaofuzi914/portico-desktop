import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { Folder, FolderOpen } from "lucide-react";
import { listWorkspaces } from "@/lib/tauri-api";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";
import { buildSidebarProjectItems } from "./sidebar-projects-model";
import { projectAbbreviation } from "./project-abbreviation";
import { useEffect, useMemo, useState } from "react";
import type { WorkspaceId } from "@/lib/schemas";
import {
  readRunningWorkspaceIds,
  subscribeWorkspaceRunActivityChanged,
} from "./workspace-activity-store";

const projectLastUsedStorageKey = "portico.sidebarProjectLastUsedAt";

interface SidebarProjectsProps {
  activeWorkspaceId?: WorkspaceId;
  /** Icon / abbreviation-only chips for the collapsed sidebar. */
  compact?: boolean;
}

export function SidebarProjects({ activeWorkspaceId, compact = false }: SidebarProjectsProps) {
  const { t } = useTranslation();
  const [lastUsedAtById, setLastUsedAtById] = useState<Record<string, string>>(() =>
    readProjectLastUsedAt(),
  );
  const [runningWorkspaceIds, setRunningWorkspaceIds] = useState<Set<WorkspaceId>>(() =>
    readRunningWorkspaceIds(),
  );
  const { data: workspaces, isLoading } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
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

  if (compact) {
    return (
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
              >
                {projectAbbreviation(item.workspace.name)}
              </Link>
              {item.isRunning && (
                <span className="absolute top-0.5 right-0.5 h-1.5 w-1.5 rounded-full bg-emerald-500" />
              )}
            </li>
          ),
        )}
      </ul>
    );
  }

  return (
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
            >
              <Folder className="h-3.5 w-3.5 shrink-0" />
              <span className="min-w-0 flex-1 truncate">{item.workspace.name}</span>
              {item.isRunning && (
                <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-500" />
              )}
            </Link>
          </li>
        ),
      )}
    </ul>
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
