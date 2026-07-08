import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { listAuditLog } from "@/lib/tauri-api";
import { formatDateTime } from "@/lib/formatters";
import { asAgentRunId, asThreadId, asWorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { AllowedPathsSummary } from "./allowed-paths-summary";

export function AuditLogPanel() {
  const { t } = useTranslation();
  const [workspaceId, setWorkspaceId] = useState("");
  const [threadId, setThreadId] = useState("");
  const [runId, setRunId] = useState("");

  const filters = {
    workspaceId: workspaceId.trim(),
    threadId: threadId.trim(),
    runId: runId.trim(),
  };

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: ["audit-log", filters],
    queryFn: () =>
      listAuditLog(
        filters.workspaceId ? asWorkspaceId(filters.workspaceId) : null,
        filters.threadId ? asThreadId(filters.threadId) : null,
        filters.runId ? asAgentRunId(filters.runId) : null,
      ),
    enabled: false,
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("operations.auditLog")}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <form
          className="grid gap-3 sm:grid-cols-4"
          onSubmit={(e) => {
            e.preventDefault();
            void refetch();
          }}
        >
          <Input
            placeholder={t("operations.workspaceProjectId")}
            value={workspaceId}
            onChange={(e) => setWorkspaceId(e.target.value)}
          />
          <Input
            placeholder={t("operations.threadId")}
            value={threadId}
            onChange={(e) => setThreadId(e.target.value)}
          />
          <Input
            placeholder={t("operations.runId")}
            value={runId}
            onChange={(e) => setRunId(e.target.value)}
          />
          <Button type="submit" disabled={isLoading}>
            {isLoading ? t("settings.loading") : t("common.filter")}
          </Button>
        </form>

        <p className="text-muted-foreground text-xs">
          {t("operations.auditHint")}
        </p>

        <AllowedPathsSummary
          workspaceId={filters.workspaceId ? asWorkspaceId(filters.workspaceId) : null}
        />

        {isError && (
          <p className="text-destructive text-sm">
            {t("operations.auditLoadFailed")}{" "}
            {error instanceof Error ? error.message : String(error)}
          </p>
        )}

        {data && data.length > 0 ? (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead className="text-muted-foreground border-b">
                <tr>
                  <th className="py-2 pr-4">{t("operations.time")}</th>
                  <th className="py-2 pr-4">{t("operations.action")}</th>
                  <th className="py-2 pr-4">{t("operations.resource")}</th>
                  <th className="py-2 pr-4">{t("operations.decision")}</th>
                  <th className="py-2 pr-4">{t("operations.reason")}</th>
                  <th className="py-2 pr-4">{t("operations.run")}</th>
                  <th className="py-2 pr-4">{t("operations.thread")}</th>
                  <th className="py-2">{t("operations.workspace")}</th>
                </tr>
              </thead>
              <tbody className="divide-y">
                {data.map((entry) => (
                  <tr key={entry.id}>
                    <td className="py-2 pr-4 whitespace-nowrap">
                      {formatDateTime(entry.created_at)}
                    </td>
                    <td className="py-2 pr-4 font-medium">{entry.action}</td>
                    <td className="py-2 pr-4">{entry.resource}</td>
                    <td className="py-2 pr-4">
                      <DecisionBadge decision={entry.decision} />
                    </td>
                    <td className="text-muted-foreground py-2 pr-4">
                      {entry.reason ?? "—"}
                    </td>
                    <td className="text-muted-foreground py-2 pr-4">
                      {entry.run_id ?? "—"}
                    </td>
                    <td className="text-muted-foreground py-2 pr-4">
                      {entry.thread_id ?? "—"}
                    </td>
                    <td className="text-muted-foreground py-2">
                      {entry.workspace_id ?? "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : data && data.length === 0 ? (
          <p className="text-muted-foreground">
            {t("operations.noAuditMatches")}
          </p>
        ) : null}
      </CardContent>
    </Card>
  );
}

function DecisionBadge({ decision }: { decision: string }) {
  const normalized = decision.toLowerCase();
  const classes =
    normalized === "allow"
      ? "bg-green-100 text-green-800"
      : normalized === "ask"
        ? "bg-amber-100 text-amber-800"
        : normalized === "deny"
          ? "bg-red-100 text-red-800"
          : "bg-gray-100 text-gray-800";
  return (
    <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${classes}`}>
      {decision}
    </span>
  );
}
