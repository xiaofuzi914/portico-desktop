import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { captureScreen, clickMouse, focusApp, moveMouse, typeText } from "@/lib/tauri-api";
import type { WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

interface DesktopPanelProps {
  workspaceId: WorkspaceId;
}

export function DesktopPanel({ workspaceId }: DesktopPanelProps) {
  const { t } = useTranslation();
  const [x, setX] = useState(0);
  const [y, setY] = useState(0);
  const [text, setText] = useState("");
  const [appName, setAppName] = useState("");

  const capture = useQuery({
    queryKey: ["desktop-capture", workspaceId],
    queryFn: () => captureScreen(workspaceId),
    enabled: false,
  });

  const click = useMutation({
    mutationFn: () => clickMouse(workspaceId),
  });
  const move = useMutation({
    mutationFn: () => moveMouse(workspaceId, x, y),
  });
  const type = useMutation({
    mutationFn: () => typeText(workspaceId, text),
  });
  const focus = useMutation({
    mutationFn: () => focusApp(workspaceId, appName),
  });

  const firstError = move.error ?? click.error ?? type.error ?? focus.error;

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-3">
      <div className="flex items-center gap-2">
        <Button
          size="sm"
          className="h-8 text-xs"
          onClick={() => capture.refetch()}
          disabled={capture.isFetching}
        >
          {t("inspector.capture")}
        </Button>
        <span className="text-muted-foreground text-xs">
          {capture.data ? `${capture.data.width}x${capture.data.height}` : ""}
        </span>
      </div>
      {capture.error && (
        <InlineError title={t("inspector.captureFailed")} message={capture.error.message} />
      )}
      {capture.data && (
        <img
          src={`data:image/png;base64,${capture.data.image_base64}`}
          alt="Desktop"
          className="rounded border"
        />
      )}

      <div className="grid grid-cols-2 gap-2">
        <Input
          type="number"
          value={x}
          onChange={(e) => setX(Number(e.target.value))}
          placeholder="x"
          className="h-8 text-xs"
        />
        <Input
          type="number"
          value={y}
          onChange={(e) => setY(Number(e.target.value))}
          placeholder="y"
          className="h-8 text-xs"
        />
      </div>
      <div className="grid grid-cols-2 gap-2">
        <Button
          size="sm"
          variant="outline"
          className="h-8 text-xs"
          onClick={() => move.mutate()}
          disabled={move.isPending}
        >
          {t("inspector.move")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          className="h-8 text-xs"
          onClick={() => click.mutate()}
          disabled={click.isPending}
        >
          {t("inspector.click")}
        </Button>
      </div>

      <div className="flex gap-2">
        <Input
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={t("desktop.textToType")}
          className="h-8 flex-1 text-xs"
        />
        <Button
          size="sm"
          className="h-8 text-xs"
          onClick={() => type.mutate()}
          disabled={!text || type.isPending}
        >
          {t("inspector.type")}
        </Button>
      </div>

      <div className="flex gap-2">
        <Input
          value={appName}
          onChange={(e) => setAppName(e.target.value)}
          placeholder={t("inspector.appName")}
          className="h-8 flex-1 text-xs"
        />
        <Button
          size="sm"
          className="h-8 text-xs"
          onClick={() => focus.mutate()}
          disabled={!appName || focus.isPending}
        >
          {t("inspector.focus")}
        </Button>
      </div>

      {firstError && <InlineError title={t("inspector.actionFailed")} message={firstError.message} />}
    </div>
  );
}

function InlineError({ title, message }: { title: string; message: string }) {
  return (
    <div className="p-3">
      <div className="rounded border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950">
        <p className="font-semibold">{title}</p>
        <p>{message}</p>
      </div>
    </div>
  );
}
