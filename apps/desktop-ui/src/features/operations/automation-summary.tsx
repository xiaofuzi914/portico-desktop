import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  AutomationForm,
  type AutomationFormData,
} from "@/features/automations/automation-form";
import { AutomationList } from "@/features/automations/automation-list";
import {
  createAutomation,
  deleteAutomation,
  listAutomations,
  listWorkspaces,
  runAutomationNow,
  updateAutomation,
} from "@/lib/tauri-api";
import { asAutomationId, asWorkspaceId, type Automation } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

export function AutomationSummary() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [workspaceFilter, setWorkspaceFilter] = useState("");
  const [editing, setEditing] = useState<Automation | null>(null);

  const { data: workspaces } = useQuery({
    queryKey: ["workspaces"],
    queryFn: listWorkspaces,
  });

  const listWorkspaceId = workspaceFilter.trim()
    ? asWorkspaceId(workspaceFilter.trim())
    : null;
  const selectedWorkspaceId =
    listWorkspaceId ??
    (workspaces?.[0] ? asWorkspaceId(workspaces[0].id) : null);

  const { data: automations, isLoading } = useQuery({
    queryKey: ["automations", listWorkspaceId],
    queryFn: () => listAutomations(listWorkspaceId),
  });

  const create = useMutation({
    mutationFn: (data: AutomationFormData) => {
      if (!selectedWorkspaceId) throw new Error("Select a workspace first");
      return createAutomation(
        selectedWorkspaceId,
        data.name,
        data.description,
        "Scheduled",
        data.cronExpr.trim() || null,
        data.enabled,
      );
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["automations", listWorkspaceId],
      });
      setEditing(null);
    },
  });

  const update = useMutation({
    mutationFn: (data: AutomationFormData) => {
      if (!editing) throw new Error("No automation selected");
      return updateAutomation({
        ...editing,
        name: data.name,
        description: data.description,
        cron_expr: data.cronExpr.trim() || null,
        enabled: data.enabled,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["automations", listWorkspaceId],
      });
      setEditing(null);
    },
  });

  const remove = useMutation({
    mutationFn: (automation: Automation) =>
      deleteAutomation(asAutomationId(automation.id)),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["automations", listWorkspaceId],
      });
    },
  });

  const runNow = useMutation({
    mutationFn: (automation: Automation) =>
      runAutomationNow(asAutomationId(automation.id)),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["automations", listWorkspaceId],
      });
    },
  });

  const handleSubmit = (data: AutomationFormData) => {
    if (editing) {
      update.mutate(data);
    } else {
      create.mutate(data);
    }
  };

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("operations.filterByWorkspace")}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid gap-3 sm:grid-cols-2">
            <select
              className="border-input bg-background h-9 rounded-md border px-3 text-sm"
              value={workspaceFilter}
              onChange={(e) => setWorkspaceFilter(e.target.value)}
            >
              <option value="">{t("operations.allWorkspaces")}</option>
              {workspaces?.map((workspace) => (
                <option key={workspace.id} value={workspace.id}>
                  {workspace.name}
                </option>
              ))}
            </select>
            <Button
              variant="outline"
              onClick={() =>
                void queryClient.invalidateQueries({
                  queryKey: ["automations", listWorkspaceId],
                })
              }
            >
              {t("common.refresh")}
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>
            {editing
              ? t("operations.editAutomation")
              : t("operations.createScheduledAutomation")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <AutomationForm
            initial={editing}
            onSubmit={handleSubmit}
            onCancel={editing ? () => setEditing(null) : undefined}
            isPending={create.isPending || update.isPending}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("operations.automations")}</CardTitle>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <p className="text-muted-foreground">{t("operations.loadingAutomations")}</p>
          ) : automations?.length ? (
            <AutomationList
              automations={automations}
              onEdit={setEditing}
              onDelete={(automation) => remove.mutate(automation)}
              onRun={(automation) => runNow.mutate(automation)}
              isPending={remove.isPending || runNow.isPending}
            />
          ) : (
            <p className="text-muted-foreground">{t("operations.noAutomations")}</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
