import { useParams, useSearch, useNavigate } from "@tanstack/react-router";
import { useCallback, useRef, useState } from "react";
import { PanelRightClose, PanelRightOpen } from "lucide-react";
import { asAgentRunId, asThreadId, asWorkspaceId } from "@/lib/schemas";
import { InspectorTabs } from "@/features/inspector/inspector-tabs";
import {
  isInspectorTab,
  readInspectorCollapsed,
  writeInspectorCollapsed,
  type InspectorTab,
} from "@/features/inspector/inspector-state";
import { AuditPanel } from "@/features/inspector/audit-panel";
import { FilesPanel } from "@/features/inspector/files-panel";
import { TimelinePanel } from "@/features/inspector/timeline-panel";
import { useTranslation } from "@/lib/i18n-react";
import { PanelResizeHandle } from "./panel-resize-handle";
import {
  DEFAULT_INSPECTOR_WIDTH,
  clampInspectorWidth,
  readInspectorWidth,
  writeInspectorWidth,
} from "./shell-layout-state";

export function InspectorShell() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const rowRef = useRef<HTMLDivElement>(null);
  const [collapsed, setCollapsed] = useState(readInspectorCollapsed);
  const [width, setWidth] = useState(readInspectorWidth);

  const params = useParams({ strict: false }) as {
    workspaceId?: string;
    threadId?: string;
  };
  const search = useSearch({ strict: false }) as {
    runId?: string;
    inspector?: string;
  };
  const workspaceId = params.workspaceId ? asWorkspaceId(params.workspaceId) : undefined;
  const threadId = params.threadId ? asThreadId(params.threadId) : undefined;
  const runId = search.runId ? asAgentRunId(search.runId) : undefined;

  const inspectorParam = search.inspector;
  const activeTab: InspectorTab =
    typeof inspectorParam === "string" && isInspectorTab(inspectorParam)
      ? inspectorParam
      : threadId
        ? "timeline"
        : "files";

  function handleChange(tab: InspectorTab) {
    void navigate({
      search: (prev: { inspector?: string; runId?: string }) => ({
        ...prev,
        inspector: tab,
      }),
      replace: true,
    } as never);
  }

  function setInspectorCollapsed(nextCollapsed: boolean) {
    setCollapsed(nextCollapsed);
    writeInspectorCollapsed(nextCollapsed);
  }

  const resolveContainerWidth = useCallback(() => {
    const parent = rowRef.current?.parentElement;
    return parent?.clientWidth ?? window.innerWidth;
  }, []);

  const applyWidth = useCallback(
    (next: number, persist: boolean) => {
      const clamped = clampInspectorWidth(next, resolveContainerWidth());
      setWidth(clamped);
      if (persist) writeInspectorWidth(clamped);
    },
    [resolveContainerWidth],
  );

  if (collapsed) {
    return (
      <aside className="bg-inspector hidden h-full w-11 shrink-0 flex-col items-center border-l py-2 transition-[width] xl:flex">
        <button
          type="button"
          className="text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:ring-ring flex h-8 w-8 items-center justify-center rounded-md focus-visible:ring-2 focus-visible:outline-none"
          onClick={() => setInspectorCollapsed(false)}
          aria-label={t("inspector.expand")}
          aria-expanded={false}
          aria-controls="inspector-content"
          title={t("inspector.expand")}
        >
          <PanelRightOpen className="h-4 w-4" />
        </button>
      </aside>
    );
  }

  return (
    <div ref={rowRef} className="hidden h-full shrink-0 xl:flex">
      <PanelResizeHandle
        value={width}
        label={t("inspector.resize")}
        onChange={(next) => applyWidth(next, false)}
        onCommit={(next) => applyWidth(next, true)}
        onReset={() => applyWidth(DEFAULT_INSPECTOR_WIDTH, true)}
      />
      <aside
        id="inspector-content"
        className="bg-inspector flex h-full shrink-0 flex-col border-l"
        style={{ width }}
      >
        <InspectorTabs
          activeTab={activeTab}
          onChange={handleChange}
          trailing={
            <button
              type="button"
              className="text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:ring-ring flex h-8 w-8 shrink-0 items-center justify-center rounded-md focus-visible:ring-2 focus-visible:outline-none"
              onClick={() => setInspectorCollapsed(true)}
              aria-label={t("inspector.collapse")}
              aria-expanded={true}
              aria-controls="inspector-content"
              title={t("inspector.collapse")}
            >
              <PanelRightClose className="h-4 w-4" />
            </button>
          }
        />
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          {!workspaceId ? (
            <TabPlaceholder message={t("inspector.openWorkspace")} />
          ) : (
            <>
              {activeTab === "timeline" && (
                <TimelinePanel threadId={threadId} activeRunId={runId} />
              )}
              {activeTab === "audit" && (
                <AuditPanel workspaceId={workspaceId} threadId={threadId} runId={runId} />
              )}
              {activeTab === "files" && <FilesPanel key={workspaceId} workspaceId={workspaceId} />}
            </>
          )}
        </div>
      </aside>
    </div>
  );
}

function TabPlaceholder({ message }: { message: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
      <p className="text-muted-foreground max-w-48 text-xs leading-5">{message}</p>
    </div>
  );
}
