import { createFileRoute } from "@tanstack/react-router";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { typography } from "@/components/ui/typography";
import {
  browserUseAction,
  closeBrowserWindow,
  listBrowserWindows,
  listWorkspaces,
  openBrowserWindow,
} from "@/lib/tauri-api";
import {
  asBrowserWindowId,
  asWorkspaceId,
  type BrowserAction,
  type BrowserWindowId,
  type WorkspaceId,
} from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

export const Route = createFileRoute("/browser/")({
  component: BrowserPage,
});

const ACTION_KINDS: BrowserAction["kind"][] = [
  "Click",
  "Type",
  "ExtractVisibleText",
  "Wait",
  "Screenshot",
];

function BrowserPage() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [url, setUrl] = useState("https://example.com");
  const [title, setTitle] = useState("");
  const [selectedId, setSelectedId] = useState<BrowserWindowId | "">("");
  const [actionKind, setActionKind] = useState<BrowserAction["kind"]>("Click");
  const [selector, setSelector] = useState("");
  const [text, setText] = useState("");
  const [waitMs, setWaitMs] = useState(1000);
  const [actionResult, setActionResult] = useState("");
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

  const { data: windows, isLoading } = useQuery({
    queryKey: ["browser-windows"],
    queryFn: listBrowserWindows,
  });

  const open = useMutation({
    mutationFn: () => {
      if (!workspaceId) throw new Error("Select a workspace first");
      return openBrowserWindow(workspaceId, url, title || undefined);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["browser-windows"] });
      setTitle("");
    },
  });

  const close = useMutation({
    mutationFn: (id: BrowserWindowId) => {
      if (!workspaceId) throw new Error("Select a workspace first");
      return closeBrowserWindow(workspaceId, id);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["browser-windows"] });
      if (selectedId === close.variables) {
        setSelectedId("");
      }
    },
  });

  function buildAction(): BrowserAction {
    switch (actionKind) {
      case "Click":
        return { kind: "Click", selector: selector.trim() || "body" };
      case "Type":
        return { kind: "Type", selector: selector.trim() || "body", text };
      case "Wait":
        return { kind: "Wait", ms: waitMs };
      case "ExtractVisibleText":
      case "Screenshot":
        return { kind: actionKind };
    }
  }

  const sendAction = useMutation({
    mutationFn: async () => {
      if (!workspaceId) throw new Error("Select a workspace first");
      if (!selectedId) throw new Error("Select a browser window first");
      return browserUseAction(workspaceId, selectedId, buildAction());
    },
    onSuccess: (result) => setActionResult(result),
  });

  return (
    <main className="container mx-auto max-w-5xl space-y-6 p-6">
      <h1 className={typography.pageTitle}>{t("browser.title")}</h1>

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
          <CardTitle>{t("browser.openWindow")}</CardTitle>
        </CardHeader>
        <CardContent>
          <form
            className="flex flex-col gap-3 sm:flex-row"
            onSubmit={(e) => {
              e.preventDefault();
              open.mutate();
            }}
          >
            <Input
              placeholder="https://example.com"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              required
            />
            <Input
              placeholder={t("browser.windowTitleOptional")}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
            <Button type="submit" disabled={open.isPending || !workspaceId}>
              {t("common.open")}
            </Button>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("browser.openWindows")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {isLoading ? (
            <p className="text-muted-foreground">{t("common.loadingWindows")}</p>
          ) : windows?.length ? (
            <ul className="divide-y">
              {windows.map((window) => (
                <li key={window.id} className="flex items-center justify-between py-3">
                  <div className="min-w-0">
                    <p className={`truncate ${typography.itemTitle}`}>
                      {window.title || window.url}
                    </p>
                    <p className={`truncate ${typography.metadata}`}>{window.url}</p>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={close.isPending || !workspaceId}
                    onClick={() => close.mutate(window.id)}
                  >
                    {t("common.close")}
                  </Button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-muted-foreground">{t("browser.noWindows")}</p>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("browser.browserUse")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <select
              className="border-input bg-background h-10 rounded-md border px-3 text-sm"
              value={selectedId}
              onChange={(e) =>
                setSelectedId(e.target.value ? asBrowserWindowId(e.target.value) : "")
              }
            >
              <option value="">{t("browser.selectWindow")}</option>
              {windows?.map((window) => (
                <option key={window.id} value={window.id}>
                  {window.title || window.url}
                </option>
              ))}
            </select>

            <select
              className="border-input bg-background h-10 rounded-md border px-3 text-sm"
              value={actionKind}
              onChange={(e) => setActionKind(e.target.value as BrowserAction["kind"])}
            >
              {ACTION_KINDS.map((kind) => (
                <option key={kind} value={kind}>
                  {t(`browser.action.${kind === "ExtractVisibleText" ? "extractVisibleText" : kind.toLowerCase()}`)}
                </option>
              ))}
            </select>
          </div>

          {(actionKind === "Click" || actionKind === "Type") && (
            <Input
              placeholder={t("browser.cssSelector")}
              value={selector}
              onChange={(e) => setSelector(e.target.value)}
            />
          )}

          {actionKind === "Type" && (
            <Input
              placeholder={t("browser.textToType")}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          )}

          {actionKind === "Wait" && (
            <Input
              type="number"
              min={0}
              value={waitMs}
              onChange={(e) => setWaitMs(Number(e.target.value))}
            />
          )}

          <Button
            onClick={() => sendAction.mutate()}
            disabled={sendAction.isPending || !selectedId || !workspaceId}
          >
            {t("browser.sendAction")}
          </Button>

          {actionResult && (
            <div className="rounded-md border p-3">
              <p className="text-sm font-medium">{t("common.result")}</p>
              <p className="text-muted-foreground text-sm">{actionResult}</p>
            </div>
          )}
        </CardContent>
      </Card>
    </main>
  );
}
