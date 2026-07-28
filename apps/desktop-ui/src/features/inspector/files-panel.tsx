import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  File,
  Folder,
  FolderOpen,
  Maximize2,
  RefreshCw,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { MarkdownWorkspacePreview } from "@/features/markdown-provider/markdown-workspace-preview";
import { MarkdownPreviewDialog } from "@/features/markdown-provider/markdown-preview-dialog";
import { ProjectFoldersPanel } from "@/features/workspaces/project-folders-panel";
import {
  listWorkspaceFiles,
  listWorkspaces,
  openWorkspaceFolder,
  previewWorkspaceMarkdown,
} from "@/lib/tauri-api";
import type { ArtifactPreview as ArtifactPreviewType, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { InlineError, PanelLoading } from "./panel-primitives";
import { workspaceKeys } from "@/lib/query-keys";
import { cn } from "@/lib/utils";

const LIST_COLLAPSED_KEY = "portico.inspector.filesListCollapsed";

function readListCollapsed(): boolean {
  try {
    return localStorage.getItem(LIST_COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

function writeListCollapsed(value: boolean) {
  try {
    localStorage.setItem(LIST_COLLAPSED_KEY, value ? "1" : "0");
  } catch {
    /* ignore */
  }
}

interface FilesPanelProps {
  workspaceId: WorkspaceId;
}

export function FilesPanel({ workspaceId }: FilesPanelProps) {
  const { t } = useTranslation();
  const [relativePath, setRelativePath] = useState("");
  const [browseRoot, setBrowseRoot] = useState<string | null>(null);
  const [preview, setPreview] = useState<ArtifactPreviewType | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewLoadingPath, setPreviewLoadingPath] = useState<string | null>(null);
  const [previewExpanded, setPreviewExpanded] = useState(false);
  const [openFolderBusy, setOpenFolderBusy] = useState(false);
  const [openFolderError, setOpenFolderError] = useState<string | null>(null);
  const [listCollapsed, setListCollapsed] = useState(readListCollapsed);
  const previewRequest = useRef(0);
  useEffect(() => {
    previewRequest.current += 1;
    setRelativePath("");
    setBrowseRoot(null);
    setPreview(null);
    setPreviewError(null);
    setPreviewLoadingPath(null);
    setPreviewExpanded(false);
    setOpenFolderError(null);
  }, [workspaceId]);
  const { data: workspaces = [], isLoading: workspacesLoading } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });
  const workspace = workspaces.find((candidate) => candidate.id === workspaceId);
  const mainRoot = workspace?.root_path ?? "";
  const activeRoot = (browseRoot ?? mainRoot).replace(/\/+$/, "");
  const rootName =
    activeRoot.split("/").filter(Boolean).at(-1) ?? t("inspector.rootFolder");
  const currentFolderName = relativePath.split("/").filter(Boolean).at(-1) ?? rootName;
  const {
    data: entries = [],
    isLoading,
    error,
    refetch,
    isFetching,
  } = useQuery({
    queryKey: workspaceKeys.filesAt(workspaceId, `${activeRoot}::${relativePath}`),
    queryFn: () => listWorkspaceFiles(workspaceId, relativePath, activeRoot || null),
    enabled: Boolean(workspace && activeRoot),
    // Agent writes land via runtime events → invalidateQueries (see workspace-files-sync).
    staleTime: 0,
  });

  if (workspacesLoading && !workspace) return <PanelLoading />;
  if (!workspace) {
    return (
      <InlineError
        title={t("inspector.filesLoadFailed")}
        message={t("inspector.workspaceMissing")}
      />
    );
  }

  const parentPath = relativePath.split("/").slice(0, -1).join("/");
  const pathParts = relativePath.split("/").filter(Boolean);

  const toggleListCollapsed = () => {
    setListCollapsed((prev) => {
      const next = !prev;
      writeListCollapsed(next);
      return next;
    });
  };

  if (preview) {
    return (
      <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden">
        <div className="flex shrink-0 items-center gap-2 border-b px-3 py-2">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => setPreview(null)}
            aria-label={t("inspector.backToFiles")}
          >
            <ChevronLeft className="h-3.5 w-3.5" />
          </Button>
          <p className="min-w-0 flex-1 truncate text-xs font-medium">
            {preview.path.split(/[\\/]/).at(-1)}
          </p>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => setPreviewExpanded(true)}
            aria-label={t("inspector.expandPreview")}
            title={t("inspector.expandPreview")}
          >
            <Maximize2 className="h-3.5 w-3.5" />
          </Button>
        </div>
        <MarkdownWorkspacePreview preview={preview} />
        {previewExpanded && (
          <MarkdownPreviewDialog preview={preview} onClose={() => setPreviewExpanded(false)} />
        )}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <ProjectFoldersPanel
        workspace={workspace}
        variant="compact"
        activePath={activeRoot}
        onSelectPath={(path) => {
          setBrowseRoot(path);
          setRelativePath("");
          setPreview(null);
          setOpenFolderError(null);
        }}
      />

      <div className="flex shrink-0 items-center gap-1 border-b px-2 py-1.5">
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0"
          onClick={toggleListCollapsed}
          aria-expanded={!listCollapsed}
          aria-label={
            listCollapsed ? t("inspector.expandFileList") : t("inspector.collapseFileList")
          }
          title={
            listCollapsed ? t("inspector.expandFileList") : t("inspector.collapseFileList")
          }
        >
          {listCollapsed ? (
            <ChevronRight className="h-3.5 w-3.5" />
          ) : (
            <ChevronDown className="h-3.5 w-3.5" />
          )}
        </Button>
        <div className="flex min-w-0 flex-1 items-center gap-1.5">
          <FolderOpen className="text-muted-foreground h-3.5 w-3.5 shrink-0" />
          <div className="min-w-0 flex-1">
            {/* Breadcrumb: click segment to navigate (replaces the old back-arrow). */}
            <nav
              className="flex min-w-0 flex-wrap items-center gap-0.5 text-xs"
              aria-label={t("inspector.breadcrumb")}
            >
              <button
                type="button"
                className={cn(
                  "hover:text-foreground max-w-[7rem] truncate rounded px-0.5 font-medium",
                  pathParts.length === 0
                    ? "text-foreground"
                    : "text-muted-foreground hover:underline",
                )}
                onClick={() => setRelativePath("")}
                title={rootName}
              >
                {rootName}
              </button>
              {pathParts.map((part, index) => {
                const target = pathParts.slice(0, index + 1).join("/");
                const isLast = index === pathParts.length - 1;
                return (
                  <span key={target} className="flex min-w-0 items-center gap-0.5">
                    <span className="text-muted-foreground">/</span>
                    <button
                      type="button"
                      className={cn(
                        "max-w-[7rem] truncate rounded px-0.5",
                        isLast
                          ? "text-foreground font-medium"
                          : "text-muted-foreground hover:text-foreground hover:underline",
                      )}
                      onClick={() => setRelativePath(target)}
                      title={part}
                    >
                      {part}
                    </button>
                  </span>
                );
              })}
            </nav>
            {relativePath ? (
              <button
                type="button"
                className="text-muted-foreground hover:text-foreground mt-0.5 text-[10px] hover:underline"
                onClick={() => setRelativePath(parentPath)}
              >
                {t("inspector.parentFolder")}
              </button>
            ) : (
              <p className="text-muted-foreground mt-0.5 truncate font-mono text-[10px]">/</p>
            )}
          </div>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0"
          disabled={openFolderBusy}
          onClick={() => {
            setOpenFolderError(null);
            setOpenFolderBusy(true);
            void openWorkspaceFolder(workspaceId, relativePath, activeRoot || null)
              .catch((openError: unknown) => {
                setOpenFolderError(
                  openError instanceof Error
                    ? openError.message
                    : t("inspector.openFolderFailed"),
                );
              })
              .finally(() => setOpenFolderBusy(false));
          }}
          aria-label={t("inspector.openFolder")}
          title={t("inspector.openFolder")}
        >
          {openFolderBusy ? (
            <RefreshCw className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <ExternalLink className="h-3.5 w-3.5" />
          )}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0"
          onClick={() => void refetch()}
          aria-label={t("common.refresh")}
          title={t("common.refresh")}
          disabled={isFetching || listCollapsed}
        >
          <RefreshCw className={isFetching ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} />
        </Button>
      </div>

      {!listCollapsed ? (
        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          {openFolderError && (
            <p className="text-destructive px-2 py-2 text-xs">{openFolderError}</p>
          )}
          {previewError && (
            <p className="text-destructive px-2 py-2 text-xs">{previewError}</p>
          )}
          {isLoading ? (
            <PanelLoading />
          ) : error ? (
            <InlineError title={t("inspector.filesLoadFailed")} message={error.message} />
          ) : entries.length === 0 ? (
            <p className="text-muted-foreground px-2 py-4 text-center text-xs">
              {t("inspector.emptyFolder")}
            </p>
          ) : (
            <ul className="space-y-0.5">
              {entries.map((entry) => (
                <li key={entry.relative_path}>
                  <button
                    type="button"
                    className="hover:bg-muted flex h-8 w-full min-w-0 items-center gap-2 rounded-md px-2 text-left text-xs disabled:cursor-default"
                    disabled={!entry.is_directory && !entry.name.toLowerCase().endsWith(".md")}
                    onClick={() => {
                      if (entry.is_directory) {
                        setRelativePath(entry.relative_path);
                        return;
                      }
                      setPreviewError(null);
                      const request = ++previewRequest.current;
                      setPreviewLoadingPath(entry.relative_path);
                      void previewWorkspaceMarkdown(
                        workspaceId,
                        entry.relative_path,
                        activeRoot || null,
                      )
                        .then((nextPreview) => {
                          if (request === previewRequest.current) setPreview(nextPreview);
                        })
                        .catch((previewFailure: unknown) => {
                          if (request !== previewRequest.current) return;
                          setPreviewError(
                            previewFailure instanceof Error
                              ? previewFailure.message
                              : t("inspector.previewFailed"),
                          );
                        })
                        .finally(() => {
                          if (request === previewRequest.current) {
                            setPreviewLoadingPath(null);
                          }
                        });
                    }}
                  >
                    {entry.is_directory ? (
                      <Folder className="h-3.5 w-3.5 shrink-0" />
                    ) : (
                      <File className="text-muted-foreground h-3.5 w-3.5 shrink-0" />
                    )}
                    <span className="min-w-0 flex-1 truncate">{entry.name}</span>
                    {previewLoadingPath === entry.relative_path && (
                      <RefreshCw className="h-3 w-3 animate-spin" />
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : (
        <button
          type="button"
          className="text-muted-foreground hover:bg-muted/50 hover:text-foreground shrink-0 px-3 py-2 text-left text-[11px]"
          onClick={toggleListCollapsed}
        >
          {t("inspector.fileListCollapsedHint")}
          {entries.length > 0
            ? ` · ${currentFolderName} (${entries.length})`
            : ` · ${currentFolderName}`}
        </button>
      )}
    </div>
  );
}
