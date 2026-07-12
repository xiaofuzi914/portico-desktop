import { useQuery } from "@tanstack/react-query";
import { listAuditLog } from "@/lib/tauri-api";
import type { AgentRunId, ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { auditKeys } from "@/lib/query-keys";
import { EmptyState, InlineError, PanelLoading } from "./panel-primitives";
import { AllowedPathsSummary } from "@/features/operations/allowed-paths-summary";

interface AuditPanelProps {
  workspaceId?: WorkspaceId;
  threadId?: ThreadId;
  runId?: AgentRunId;
}

export function AuditPanel({ workspaceId, threadId, runId }: AuditPanelProps) {
  const { t } = useTranslation();
  const hasIds = !!workspaceId || !!threadId || !!runId;

  const { data, isLoading, error } = useQuery({
    queryKey: auditKeys.log(workspaceId ?? null, threadId ?? null, runId ?? null),
    queryFn: () => listAuditLog(workspaceId ?? null, threadId ?? null, runId ?? null),
    enabled: hasIds,
  });

  if (!hasIds) return <EmptyState message={t("inspector.noSelection")} />;
  if (isLoading) return <PanelLoading />;
  if (error) return <InlineError title={t("inspector.loadAuditFailed")} message={error.message} />;

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-3">
      {workspaceId && <AllowedPathsSummary workspaceId={workspaceId} compact />}
      <table className="w-full border-collapse text-xs">
        <thead>
          <tr className="text-muted-foreground border-b text-left">
            <th className="py-1 pr-2">{t("operations.action")}</th>
            <th className="py-1 pr-2">{t("operations.resource")}</th>
            <th className="py-1 pr-2">{t("operations.decision")}</th>
            <th className="py-1 pr-2">{t("inspector.when")}</th>
          </tr>
        </thead>
        <tbody>
          {data?.map((entry) => (
            <tr key={entry.id} className="border-b last:border-b-0">
              <td className="py-1 pr-2">{entry.action}</td>
              <td className="max-w-[120px] truncate py-1 pr-2" title={entry.resource}>
                {entry.resource}
              </td>
              <td className="py-1 pr-2">{entry.decision}</td>
              <td className="text-muted-foreground py-1 pr-2 text-[10px]">
                {new Date(entry.created_at).toLocaleString()}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {!data?.length && (
        <p className="text-muted-foreground mt-2 text-xs">{t("inspector.noAuditEntries")}</p>
      )}
    </div>
  );
}
