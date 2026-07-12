import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ChevronLeft,
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

interface FilesPanelProps {
  workspaceId: WorkspaceId;
}

export function FilesPanel({ workspaceId }: FilesPanelProps) {
  const { t } = useTranslation();
  const [relativePath, setRelativePath] = useState("");
  const [preview, setPreview] = useState<ArtifactPreviewType | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewLoadingPath, setPreviewLoadingPath] = useState<string | null>(null);
  const [previewExpanded, setPreviewExpanded] = useState(false);
  const [openFolderBusy, setOpenFolderBusy] = useState(false);
  const [openFolderError, setOpenFolderError] = useState<string | null>(null);
  const previewRequest = useRef(0);
  useEffect(() => {
    previewRequest.current += 1;
    setRelativePath("");
    setPreview(null);
    setPreviewError(null);
    setPreviewLoadingPath(null);
    setPreviewExpanded(false);
    setOpenFolderError(null);
  }, [workspaceId]);
  const { data: workspaces = [] } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });
  const workspace = workspaces.find((candidate) => candidate.id === workspaceId);
  const rootName =
    workspace?.root_path.split("/").filter(Boolean).at(-1) ?? t("inspector.rootFolder");
  const currentFolderName = relativePath.split("/").filter(Boolean).at(-1) ?? rootName;
  const {
    data: entries = [],
    isLoading,
    error,
    refetch,
    isFetching,
  } = useQuery({
    queryKey: workspaceKeys.filesAt(workspaceId, relativePath),
    queryFn: () => listWorkspaceFiles(workspaceId, relativePath),
    // Agent writes land via runtime events → invalidateQueries (see workspace-files-sync).
    staleTime: 0,
  });

  if (isLoading) return <PanelLoading />;
  if (error) {
    return <InlineError title={t("inspector.filesLoadFailed")} message={error.message} />;
  }

  const parentPath = relativePath.split("/").slice(0, -1).join("/");

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
      <div className="flex shrink-0 items-center gap-2 border-b px-3 py-2">
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          disabled={!relativePath}
          onClick={() => setRelativePath(parentPath)}
          aria-label={t("inspector.parentFolder")}
        >
          <ChevronLeft className="h-3.5 w-3.5" />
        </Button>
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <FolderOpen className="h-4 w-4 shrink-0" />
          <div className="min-w-0">
            <p className="truncate text-xs font-medium">{currentFolderName}</p>
            <p className="text-muted-foreground truncate font-mono text-[10px]">
              {relativePath || "/"}
            </p>
          </div>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          disabled={openFolderBusy || !workspace}
          onClick={() => {
            setOpenFolderError(null);
            setOpenFolderBusy(true);
            void openWorkspaceFolder(workspaceId, relativePath)
              .catch((error: unknown) => {
                setOpenFolderError(
                  error instanceof Error ? error.message : t("inspector.openFolderFailed"),
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
          className="h-7 w-7"
          onClick={() => void refetch()}
          aria-label={t("common.refresh")}
          title={t("common.refresh")}
          disabled={isFetching}
        >
          <RefreshCw className={isFetching ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} />
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {openFolderError && <p className="px-2 py-2 text-xs text-red-600">{openFolderError}</p>}
        {previewError && <p className="px-2 py-2 text-xs text-red-600">{previewError}</p>}
        {entries.length === 0 ? (
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
                    void previewWorkspaceMarkdown(workspaceId, entry.relative_path)
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
                        if (request === previewRequest.current) setPreviewLoadingPath(null);
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
    </div>
  );
}
