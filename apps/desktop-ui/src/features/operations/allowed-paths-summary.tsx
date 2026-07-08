import { useQuery } from "@tanstack/react-query";
import { FolderLock } from "lucide-react";
import { listWorkspaces } from "@/lib/tauri-api";
import type { WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { buildAllowedPathsSummary } from "./allowed-paths-summary-model";

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
    queryKey: ["workspaces"],
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
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">{item.workspace.name}</p>
                {!compact && (
                  <p className="text-muted-foreground truncate font-mono text-xs">
                    {item.workspace.root_path}
                  </p>
                )}
              </div>
              <PathGroup
                label={t("operations.allowedRead")}
                paths={item.readPaths}
                compact={compact}
              />
              <PathGroup
                label={t("operations.allowedWrite")}
                paths={item.writePaths}
                compact={compact}
              />
            </div>
          ))
        ) : (
          <p className="text-muted-foreground text-xs">{t("operations.noAllowedPaths")}</p>
        )}
      </div>
    </section>
  );
}

function PathGroup({
  label,
  paths,
  compact,
}: {
  label: string;
  paths: string[];
  compact: boolean;
}) {
  const { t } = useTranslation();

  return (
    <div className="grid gap-1">
      <p className="text-muted-foreground text-xs font-medium">{label}</p>
      {paths.length ? (
        <ul className={compact ? "space-y-1" : "grid gap-1"}>
          {paths.map((path) => (
            <li
              key={path}
              className="bg-muted truncate rounded px-2 py-1 font-mono text-xs"
              title={path}
            >
              {path}
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-muted-foreground text-xs">{t("common.none")}</p>
      )}
    </div>
  );
}
