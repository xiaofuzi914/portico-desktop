import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  browserUseAction,
  closeBrowserWindow,
  listBrowserWindows,
  openBrowserWindow,
} from "@/lib/tauri-api";
import type { BrowserAction, BrowserWindowId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

interface BrowserPanelProps {
  workspaceId: WorkspaceId;
}

export function BrowserPanel({ workspaceId }: BrowserPanelProps) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [url, setUrl] = useState("https://example.com");
  const [title, setTitle] = useState("");
  const [selectedId, setSelectedId] = useState<BrowserWindowId | null>(null);
  const [actionKind, setActionKind] = useState<BrowserAction["kind"]>("Click");
  const [selector, setSelector] = useState("");
  const [typeText, setTypeText] = useState("");
  const [waitMs, setWaitMs] = useState(1000);

  const { data: windows, isLoading } = useQuery({
    queryKey: ["browser-windows"],
    queryFn: listBrowserWindows,
    refetchInterval: 2000,
  });

  const open = useMutation({
    mutationFn: () => openBrowserWindow(workspaceId, url, title || undefined),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["browser-windows"] }),
  });

  const close = useMutation({
    mutationFn: (id: BrowserWindowId) => closeBrowserWindow(workspaceId, id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["browser-windows"] });
      setSelectedId(null);
    },
  });

  const action = useMutation({
    mutationFn: async () => {
      if (!selectedId) throw new Error(t("browser.selectWindow"));
      let browserAction: BrowserAction;
      switch (actionKind) {
        case "Click":
          browserAction = { kind: "Click", selector };
          break;
        case "Type":
          browserAction = { kind: "Type", selector, text: typeText };
          break;
        case "Wait":
          browserAction = { kind: "Wait", ms: waitMs };
          break;
        case "ExtractVisibleText":
          browserAction = { kind: "ExtractVisibleText" };
          break;
        case "Screenshot":
          browserAction = { kind: "Screenshot" };
          break;
        default:
          throw new Error(t("inspector.actionFailed"));
      }
      return browserUseAction(workspaceId, selectedId, browserAction);
    },
  });

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-3">
      <div className="flex gap-2">
        <Input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder={t("capabilities.url")}
          className="h-8 flex-1 text-xs"
        />
        <Input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={t("thread.threadTitle")}
          className="h-8 w-24 text-xs"
        />
        <Button
          size="sm"
          className="h-8 text-xs"
          onClick={() => open.mutate()}
          disabled={open.isPending}
        >
          <ExternalLink className="mr-1 h-3 w-3" />
          {t("inspector.open")}
        </Button>
      </div>
      {open.error && <InlineError title={t("inspector.openFailed")} message={open.error.message} />}

      <div className="space-y-1">
        <h4 className="text-muted-foreground text-xs font-semibold">{t("inspector.windows")}</h4>
        {isLoading && <PanelLoading />}
        {!isLoading && !windows?.length && (
          <p className="text-muted-foreground text-xs">{t("inspector.noBrowserWindows")}</p>
        )}
        {windows?.map((window) => (
          <div
            key={window.id}
            onClick={() => setSelectedId(window.id)}
            className={`flex items-center justify-between rounded border p-2 text-xs ${
              selectedId === window.id ? "bg-muted" : ""
            }`}
          >
            <div className="min-w-0 flex-1">
              <p className="truncate font-medium">{window.title}</p>
              <p className="text-muted-foreground truncate text-[10px]">{window.url}</p>
            </div>
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6"
              onClick={(event) => {
                event.stopPropagation();
                close.mutate(window.id);
              }}
              disabled={close.isPending}
            >
              <X className="h-3 w-3" />
            </Button>
          </div>
        ))}
      </div>

      <div className="space-y-2">
        <h4 className="text-muted-foreground text-xs font-semibold">{t("inspector.action")}</h4>
        <select
          value={actionKind}
          onChange={(e) => setActionKind(e.target.value as BrowserAction["kind"])}
          className="bg-background h-8 w-full rounded-md border px-2 text-xs"
        >
          <option value="Click">{t("browser.action.click")}</option>
          <option value="Type">{t("browser.action.type")}</option>
          <option value="Wait">{t("browser.action.wait")}</option>
          <option value="ExtractVisibleText">{t("browser.action.extractVisibleText")}</option>
          <option value="Screenshot">{t("browser.action.screenshot")}</option>
        </select>
        {actionKind === "Click" && (
          <Input
            value={selector}
            onChange={(e) => setSelector(e.target.value)}
            placeholder={t("inspector.selector")}
            className="h-8 text-xs"
          />
        )}
        {actionKind === "Type" && (
          <>
            <Input
              value={selector}
              onChange={(e) => setSelector(e.target.value)}
              placeholder={t("inspector.selector")}
              className="h-8 text-xs"
            />
            <Input
              value={typeText}
              onChange={(e) => setTypeText(e.target.value)}
              placeholder={t("inspector.text")}
              className="h-8 text-xs"
            />
          </>
        )}
        {actionKind === "Wait" && (
          <Input
            type="number"
            value={waitMs}
            onChange={(e) => setWaitMs(Number(e.target.value))}
            className="h-8 text-xs"
          />
        )}
        <Button
          size="sm"
          className="h-8 w-full text-xs"
          disabled={!selectedId || action.isPending}
          onClick={() => action.mutate()}
        >
          {t("inspector.sendAction")}
        </Button>
        {action.isSuccess && <p className="text-muted-foreground text-xs">{action.data}</p>}
        {action.error && (
          <InlineError title={t("inspector.actionFailed")} message={action.error.message} />
        )}
      </div>
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

function PanelLoading() {
  const { t } = useTranslation();
  return <p className="text-muted-foreground p-3 text-xs">{t("inspector.loading")}</p>;
}
