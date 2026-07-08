import { useState } from "react";
import { useParams, useSearch } from "@tanstack/react-router";
import { asAgentRunId, asThreadId, asWorkspaceId } from "@/lib/schemas";
import { InspectorTabs } from "@/features/inspector/inspector-tabs";
import { isInspectorTab, type InspectorTab } from "@/features/inspector/inspector-state";
import { ContextPanel } from "@/features/inspector/context-panel";
import { FilesPanel } from "@/features/inspector/files-panel";
import { TerminalPanel } from "@/features/inspector/terminal-panel";
import { BrowserPanel } from "@/features/inspector/browser-panel";
import { DesktopPanel } from "@/features/inspector/desktop-panel";
import { ArtifactsPanel } from "@/features/inspector/artifacts-panel";
import { AuditPanel } from "@/features/inspector/audit-panel";
import { useTranslation } from "@/lib/i18n-react";

export function InspectorShell() {
  const [activeTab, setActiveTab] = useState<InspectorTab>("files");
  const { t } = useTranslation();

  const params = useParams({ strict: false }) as {
    workspaceId?: string;
    threadId?: string;
  };
  const search = useSearch({ strict: false }) as { runId?: string };
  const workspaceId = params.workspaceId ? asWorkspaceId(params.workspaceId) : undefined;
  const threadId = params.threadId ? asThreadId(params.threadId) : undefined;
  const runId = search.runId ? asAgentRunId(search.runId) : undefined;

  function handleChange(tab: string) {
    if (isInspectorTab(tab)) setActiveTab(tab);
  }

  return (
    <aside className="bg-inspector hidden h-full w-[var(--inspector-width)] shrink-0 flex-col border-l xl:flex">
      <div className="flex h-10 shrink-0 items-center justify-between border-b px-3">
        <div>
          <h2 className="text-sm font-semibold">{t("inspector.title")}</h2>
        </div>
        <span className="text-muted-foreground text-[11px]">
          {workspaceId ? t("common.projectScoped") : t("common.noProject")}
        </span>
      </div>
      <InspectorTabs activeTab={activeTab} onChange={handleChange} />
      <div className="flex flex-1 flex-col overflow-hidden">
        {!workspaceId ? (
          <TabPlaceholder message={t("inspector.openWorkspace")} />
        ) : (
          <>
            {activeTab === "context" &&
              (threadId ? (
                <ContextPanel workspaceId={workspaceId} threadId={threadId} runId={runId} />
              ) : (
                <TabPlaceholder message={t("inspector.openThreadContext")} />
              ))}
            {activeTab === "files" && <FilesPanel workspaceId={workspaceId} />}
            {activeTab === "terminal" &&
              (threadId ? (
                <TerminalPanel threadId={threadId} />
              ) : (
                <TabPlaceholder message={t("inspector.openThreadTerminal")} />
              ))}
            {activeTab === "browser" && <BrowserPanel workspaceId={workspaceId} />}
            {activeTab === "desktop" && <DesktopPanel workspaceId={workspaceId} />}
            {activeTab === "artifacts" && (
              <ArtifactsPanel workspaceId={workspaceId} runId={runId} />
            )}
            {activeTab === "audit" && (
              <AuditPanel workspaceId={workspaceId} threadId={threadId} runId={runId} />
            )}
          </>
        )}
      </div>
    </aside>
  );
}

function TabPlaceholder({ message }: { message: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
      <p className="text-muted-foreground max-w-48 text-xs leading-5">{message}</p>
    </div>
  );
}
