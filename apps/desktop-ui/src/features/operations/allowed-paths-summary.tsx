import { useQuery } from "@tanstack/react-query";
import { Folder, FolderLock, FolderOpen } from "lucide-react";
import { listWorkspaces } from "@/lib/tauri-api";
import type { WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";
import { cn } from "@/lib/utils";
import {
  buildAllowedPathsSummary,
  type AllowedFolderItem,
} from "./allowed-paths-summary-model";

interface AllowedPathsSummaryProps {
  workspaceId?: WorkspaceId | null;
  compact?: boolean;
}

export function AllowedPathsSummary({
  workspaceId = null,
  compact = false,
}: AllowedPathsSummaryProps) {
  const { t } = useTranslation();
  const { data: workspaces, isLoading } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });

  const items = buildAllowedPathsSummary(workspaces ?? [], workspaceId);

  return (
    <section className="rounded-md border p-3">
      <div className="flex items-start gap-2">
        <FolderLock className="mt-0.5 h-4 w-4 shrink-0" />
        <div className="min-w-0">
          <h3 className="text-sm font-semibold">{t("operations.allowedPathsSummary")}</h3>
          {!compact && (
            <p className="text-muted-foreground mt-1 text-xs leading-5">
              {t("operations.allowedPathsSummaryBody")}
            </p>
          )}
        </div>
      </div>

      <div className="mt-3 space-y-3">
        {isLoading ? (
          <p className="text-muted-foreground text-xs">{t("common.loading")}</p>
        ) : items.length ? (
          items.map((item) => (
            <div
              key={item.workspace.id}
              className="space-y-2 border-t pt-3 first:border-t-0 first:pt-0"
            >
              {!compact && (
                <p className="text-muted-foreground text-[11px] font-medium tracking-wide uppercase">
                  {item.workspace.name}
                </p>
              )}
              <ul className="space-y-1.5">
                {item.folders.map((folder) => (
                  <FolderRow key={folder.path} folder={folder} compact={compact} />
                ))}
              </ul>
            </div>
          ))
        ) : (
          <p className="text-muted-foreground text-xs">{t("operations.noAllowedPaths")}</p>
        )}
      </div>
    </section>
  );
}

function FolderRow({
  folder,
  compact,
}: {
  folder: AllowedFolderItem;
  compact: boolean;
}) {
  const { t } = useTranslation();
  const Icon = folder.isProjectRoot ? FolderOpen : Folder;

  return (
    <li
      className={cn(
        "bg-muted/60 flex items-start gap-2 rounded-lg border px-2.5 py-2",
        folder.isProjectRoot && "border-primary/25 bg-primary/5",
      )}
      title={folder.path}
    >
      <Icon
        className={cn(
          "mt-0.5 h-4 w-4 shrink-0",
          folder.isProjectRoot ? "text-primary" : "text-muted-foreground",
        )}
        aria-hidden
      />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          <span className="truncate text-sm font-medium">{folder.name}</span>
          {folder.isProjectRoot ? (
            <Tag>{t("operations.folderTagRoot")}</Tag>
          ) : null}
          {folder.canRead ? <Tag tone="read">{t("operations.folderTagRead")}</Tag> : null}
          {folder.canWrite ? (
            <Tag tone="write">{t("operations.folderTagWrite")}</Tag>
          ) : null}
          {!folder.canRead && !folder.canWrite ? (
            <Tag>{t("operations.folderTagNone")}</Tag>
          ) : null}
        </div>
        {!compact ? (
          <p className="text-muted-foreground mt-0.5 truncate font-mono text-[10px] leading-4">
            {folder.path}
          </p>
        ) : folder.parentPath ? (
          <p className="text-muted-foreground mt-0.5 truncate text-[10px] leading-4">
            {folder.parentPath}
          </p>
        ) : (
          <p className="text-muted-foreground mt-0.5 truncate font-mono text-[10px] leading-4">
            {folder.path}
          </p>
        )}
      </div>
    </li>
  );
}

function Tag({
  children,
  tone = "default",
}: {
  children: React.ReactNode;
  tone?: "default" | "read" | "write";
}) {
  return (
    <span
      className={cn(
        "inline-flex shrink-0 rounded-full border px-1.5 py-0.5 text-[10px] font-medium",
        tone === "read" &&
          "border-sky-500/30 bg-sky-500/10 text-sky-800 dark:text-sky-300",
        tone === "write" &&
          "border-amber-500/30 bg-amber-500/10 text-amber-900 dark:text-amber-300",
        tone === "default" && "border-border bg-background text-muted-foreground",
      )}
    >
      {children}
    </span>
  );
}
