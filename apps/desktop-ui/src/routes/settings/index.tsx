import { createFileRoute, Link } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { typography } from "@/components/ui/typography";
import {
  collectDiagnosticsBundle,
  listMigrations,
  rollbackLastMigration,
  uploadDiagnosticsBundle,
} from "@/lib/tauri-api";
import { formatDateTime } from "@/lib/formatters";
import type { DiagnosticsBundle } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { migrationKeys } from "@/lib/query-keys";

export const Route = createFileRoute("/settings/")({
  component: SettingsPage,
});

function SettingsPage() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [bundle, setBundle] = useState<DiagnosticsBundle | null>(null);

  const collectMutation = useMutation({
    mutationFn: collectDiagnosticsBundle,
    onSuccess: (data) => {
      setBundle(data);
    },
  });

  const uploadMutation = useMutation({
    mutationFn: () => {
      if (!bundle) {
        throw new Error("No diagnostics bundle collected");
      }
      return uploadDiagnosticsBundle(bundle.id);
    },
  });

  const migrationsQuery = useQuery({
    queryKey: migrationKeys.list(),
    queryFn: listMigrations,
    enabled: false,
  });

  const rollbackMutation = useMutation({
    mutationFn: rollbackLastMigration,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: migrationKeys.list() });
    },
  });

  return (
    <main className="container mx-auto max-w-5xl space-y-6 p-6">
      <div>
        <h1 className={typography.pageTitle}>{t("settings.title")}</h1>
        <p className={typography.pageDescription}>{t("settings.description")}</p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.capabilities")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-muted-foreground text-sm">
            {t("settings.capabilitiesBody")}
          </p>
          <Button asChild>
            <Link to="/models">{t("settings.openCapabilities")}</Link>
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.diagnostics")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-wrap gap-3">
            <Button onClick={() => collectMutation.mutate()} disabled={collectMutation.isPending}>
              {collectMutation.isPending
                ? t("settings.collecting")
                : t("settings.collectDiagnostics")}
            </Button>
            <Button
              onClick={() => uploadMutation.mutate()}
              disabled={!bundle || uploadMutation.isPending}
              variant="outline"
            >
              {uploadMutation.isPending
                ? t("settings.uploading")
                : t("settings.uploadDiagnostics")}
            </Button>
          </div>

          {collectMutation.isError && (
            <p className="text-destructive text-sm">
              {t("settings.collectFailed")}{" "}
              {collectMutation.error instanceof Error
                ? collectMutation.error.message
                : String(collectMutation.error)}
            </p>
          )}

          {uploadMutation.isError && (
            <p className="text-destructive text-sm">
              {t("settings.uploadFailed")}{" "}
              {uploadMutation.error instanceof Error
                ? uploadMutation.error.message
                : String(uploadMutation.error)}
            </p>
          )}

          {bundle && (
            <div className="space-y-1 text-sm">
              <p>
                <span className="font-medium">{t("settings.path")}</span> {bundle.log_path}
              </p>
              <p>
                <span className="font-medium">{t("settings.size")}</span>{" "}
                {bundle.size_bytes.toLocaleString()} {t("settings.bytes")}
              </p>
              <p>
                <span className="font-medium">{t("settings.redacted")}</span>{" "}
                {bundle.redacted ? t("common.yes") : t("common.no")}
              </p>
              <p>
                <span className="font-medium">{t("settings.created")}</span>{" "}
                {formatDateTime(bundle.created_at)}
              </p>
              <p>
                <span className="font-medium">{t("settings.version")}</span> {bundle.app_version}
              </p>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.migrations")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-wrap gap-3">
            <Button
              onClick={() => migrationsQuery.refetch()}
              disabled={migrationsQuery.isLoading || migrationsQuery.isFetching}
            >
              {migrationsQuery.isLoading || migrationsQuery.isFetching
                ? t("settings.loading")
                : t("settings.listMigrations")}
            </Button>
            <Button
              onClick={() => rollbackMutation.mutate()}
              disabled={rollbackMutation.isPending}
              variant="destructive"
            >
              {rollbackMutation.isPending
                ? t("settings.rollingBack")
                : t("settings.rollbackLastMigration")}
            </Button>
          </div>

          {migrationsQuery.isError && (
            <p className="text-destructive text-sm">
              {t("settings.listMigrationsFailed")}{" "}
              {migrationsQuery.error instanceof Error
                ? migrationsQuery.error.message
                : String(migrationsQuery.error)}
            </p>
          )}

          {rollbackMutation.isError && (
            <p className="text-destructive text-sm">
              {t("settings.rollbackFailed")}{" "}
              {rollbackMutation.error instanceof Error
                ? rollbackMutation.error.message
                : String(rollbackMutation.error)}
            </p>
          )}

          {rollbackMutation.isSuccess && (
            <p className="text-sm text-green-600">{t("settings.rollbackSuccess")}</p>
          )}

          {migrationsQuery.data && migrationsQuery.data.length > 0 ? (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead className="text-muted-foreground border-b">
                  <tr>
                    <th className="py-2 pr-4">{t("settings.migrationVersion")}</th>
                    <th className="py-2 pr-4">{t("settings.migrationName")}</th>
                    <th className="py-2 pr-4">{t("settings.migrationAppliedAt")}</th>
                    <th className="py-2">{t("settings.migrationChecksum")}</th>
                  </tr>
                </thead>
                <tbody className="divide-y">
                  {migrationsQuery.data.map((migration) => (
                    <tr key={migration.version}>
                      <td className="py-2 pr-4 font-medium">{migration.version}</td>
                      <td className="py-2 pr-4">{migration.name}</td>
                      <td className="py-2 pr-4 whitespace-nowrap">
                        {formatDateTime(migration.applied_at)}
                      </td>
                      <td className="text-muted-foreground py-2 font-mono">{migration.checksum}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : migrationsQuery.data && migrationsQuery.data.length === 0 ? (
            <p className="text-muted-foreground">{t("settings.noMigrations")}</p>
          ) : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("settings.about")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-1 text-sm">
          <p>
            <span className="font-medium">{t("settings.version")}</span> 0.1.0
          </p>
          <p>
            <span className="font-medium">{t("settings.updaterEndpoint")}</span>{" "}
            <span className="text-muted-foreground">{t("settings.configuredAtBuild")}</span>
          </p>
        </CardContent>
      </Card>
    </main>
  );
}
