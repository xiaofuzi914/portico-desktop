import { createFileRoute } from "@tanstack/react-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { typography } from "@/components/ui/typography";
import {
  captureScreen,
  clickMouse,
  focusApp,
  listWorkspaces,
  moveMouse,
  typeText,
} from "@/lib/tauri-api";
import { asWorkspaceId, type DesktopCapture, type WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

export const Route = createFileRoute("/desktop/")({
  component: DesktopPage,
});

function DesktopPage() {
  const { t } = useTranslation();
  const [capture, setCapture] = useState<DesktopCapture | null>(null);
  const [x, setX] = useState(0);
  const [y, setY] = useState(0);
  const [text, setText] = useState("");
  const [appName, setAppName] = useState("");
  const [workspaceFilter, setWorkspaceFilter] = useState("");

  const { data: workspaces } = useQuery({
    queryKey: ["workspaces"],
    queryFn: listWorkspaces,
  });

  const workspaceId: WorkspaceId | null = workspaceFilter.trim()
    ? asWorkspaceId(workspaceFilter.trim())
    : workspaces?.[0]
      ? asWorkspaceId(workspaces[0].id)
      : null;

  const screenCapture = useMutation({
    mutationFn: () => {
      if (!workspaceId) throw new Error("Select a workspace first");
      return captureScreen(workspaceId);
    },
    onSuccess: setCapture,
  });

  const move = useMutation({
    mutationFn: () => {
      if (!workspaceId) throw new Error("Select a workspace first");
      return moveMouse(workspaceId, x, y);
    },
  });

  const click = useMutation({
    mutationFn: () => {
      if (!workspaceId) throw new Error("Select a workspace first");
      return clickMouse(workspaceId);
    },
  });

  const type = useMutation({
    mutationFn: () => {
      if (!workspaceId) throw new Error("Select a workspace first");
      return typeText(workspaceId, text);
    },
  });

  const focus = useMutation({
    mutationFn: () => {
      if (!workspaceId) throw new Error("Select a workspace first");
      return focusApp(workspaceId, appName);
    },
  });

  return (
    <main className="container mx-auto max-w-5xl space-y-6 p-6">
      <h1 className={typography.pageTitle}>{t("desktop.title")}</h1>

      <Card>
        <CardHeader>
          <CardTitle>{t("common.workspace")}</CardTitle>
        </CardHeader>
        <CardContent>
          <select
            className="border-input bg-background h-10 w-full rounded-md border px-3 text-sm"
            value={workspaceFilter}
            onChange={(e) => setWorkspaceFilter(e.target.value)}
          >
            <option value="">
              {workspaces?.[0] ? workspaces[0].name : t("browser.selectWorkspace")}
            </option>
            {workspaces?.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.name}
              </option>
            ))}
          </select>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("desktop.screenCapture")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <Button
            onClick={() => screenCapture.mutate()}
            disabled={screenCapture.isPending || !workspaceId}
          >
            {t("desktop.capture")}
          </Button>

          {capture && (
            <div className="space-y-2">
              <p className="text-muted-foreground text-sm">
                {capture.width} × {capture.height}
              </p>
              <img
                src={`data:image/png;base64,${capture.image_base64}`}
                alt={t("desktop.screenCapture")}
                className="h-auto max-w-full rounded-md border"
              />
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("desktop.mouseControl")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-3 sm:flex-row">
            <Input
              type="number"
              placeholder="X"
              value={x}
              onChange={(e) => setX(Number(e.target.value))}
            />
            <Input
              type="number"
              placeholder="Y"
              value={y}
              onChange={(e) => setY(Number(e.target.value))}
            />
            <Button onClick={() => move.mutate()} disabled={move.isPending || !workspaceId}>
              {t("desktop.moveMouse")}
            </Button>
            <Button onClick={() => click.mutate()} disabled={click.isPending || !workspaceId}>
              {t("desktop.clickMouse")}
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("desktop.keyboardInput")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-3 sm:flex-row">
            <Input
              placeholder={t("desktop.textToType")}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
            <Button
              onClick={() => type.mutate()}
              disabled={type.isPending || !text || !workspaceId}
            >
              {t("desktop.typeText")}
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("desktop.applicationFocus")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-3 sm:flex-row">
            <Input
              placeholder={t("desktop.applicationName")}
              value={appName}
              onChange={(e) => setAppName(e.target.value)}
            />
            <Button
              onClick={() => focus.mutate()}
              disabled={focus.isPending || !appName || !workspaceId}
            >
              {t("desktop.focusApplication")}
            </Button>
          </div>
        </CardContent>
      </Card>
    </main>
  );
}
