import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { FileCode2, FileText, Loader2, Pencil, Plus } from "lucide-react";
import { MarkdownPreviewDialog } from "@/features/markdown-provider/markdown-preview-dialog";
import { ArtifactPreview as ArtifactPreviewView } from "@/components/artifact/artifact-preview";
import { Modal } from "@/components/ui/modal";
import { Button } from "@/components/ui/button";
import { listRunEvents, previewArtifact, previewWorkspaceMarkdown } from "@/lib/tauri-api";
import type { AgentRunId, ArtifactPreview, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { cn } from "@/lib/utils";
import {
  collectFileRefsFromRunEvents,
  extractPathsFromText,
  isMarkdownPath,
  isPreviewablePath,
  mergeFileRefs,
  type FileChangeKind,
  type MessageFileRef,
} from "./message-file-refs";

type Props = {
  workspaceId: WorkspaceId;
  runId?: AgentRunId | null;
  /** Assistant message text — used to surface mentioned deliverables. */
  messageText: string;
  className?: string;
};

function kindIcon(kind: FileChangeKind) {
  if (kind === "edited") return Pencil;
  if (kind === "written" || kind === "artifact") return Plus;
  return FileText;
}

function kindLabelKey(kind: FileChangeKind): string {
  switch (kind) {
    case "written":
      return "agent.fileChange.written";
    case "edited":
      return "agent.fileChange.edited";
    case "artifact":
      return "agent.fileChange.artifact";
    default:
      return "agent.fileChange.mentioned";
  }
}

/**
 * Surfaces files produced/changed in this turn and lets the user open a preview.
 */
export function MessageFileChanges({
  workspaceId,
  runId,
  messageText,
  className,
}: Props) {
  const { t } = useTranslation();
  const [preview, setPreview] = useState<ArtifactPreview | null>(null);
  const [previewExpanded, setPreviewExpanded] = useState(false);
  const [loadingPath, setLoadingPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const eventsQuery = useQuery({
    queryKey: ["run-events-files", runId],
    queryFn: () => listRunEvents(runId!),
    enabled: Boolean(runId) && runId !== ("unknown" as AgentRunId),
    staleTime: 15_000,
  });

  const files = useMemo(() => {
    const fromTools = collectFileRefsFromRunEvents(eventsQuery.data);
    const fromText = extractPathsFromText(messageText);
    return mergeFileRefs(fromTools, fromText);
  }, [eventsQuery.data, messageText]);

  if (files.length === 0 && !eventsQuery.isLoading) {
    return null;
  }

  async function openFile(ref: MessageFileRef) {
    if (!isPreviewablePath(ref.path)) {
      setError(t("agent.fileChange.notPreviewable"));
      return;
    }
    setError(null);
    setLoadingPath(ref.path);
    try {
      // Workspace-relative paths (preferred). Falls back to absolute artifact read.
      let next: ArtifactPreview;
      try {
        next = await previewWorkspaceMarkdown(workspaceId, ref.path, null);
      } catch {
        next = await previewArtifact(workspaceId, ref.path);
      }
      setPreview(next);
      setPreviewExpanded(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("agent.fileChange.previewFailed"));
    } finally {
      setLoadingPath(null);
    }
  }

  return (
    <div
      className={cn(
        "border-border/70 bg-background/60 mt-3 rounded-lg border px-3 py-2.5",
        className,
      )}
    >
      <div className="mb-2 flex items-center gap-2">
        <FileCode2 className="text-muted-foreground h-3.5 w-3.5 shrink-0" />
        <p className="text-muted-foreground text-[11px] font-semibold tracking-wide uppercase">
          {t("agent.fileChange.title")}
          {files.length > 0 ? (
            <span className="text-foreground/80 ml-1 font-medium normal-case">
              ({files.length})
            </span>
          ) : null}
        </p>
        {eventsQuery.isLoading ? (
          <Loader2 className="text-muted-foreground h-3 w-3 animate-spin" />
        ) : null}
      </div>
      <ul className="flex flex-col gap-1">
        {files.map((ref) => {
          const Icon = kindIcon(ref.kind);
          const loading = loadingPath === ref.path;
          return (
            <li key={ref.path}>
              <button
                type="button"
                className={cn(
                  "hover:bg-muted/80 flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors",
                  "focus-visible:ring-ring focus-visible:ring-2 focus-visible:outline-none",
                )}
                onClick={() => void openFile(ref)}
                disabled={loading}
                title={t("agent.fileChange.openHint")}
              >
                <Icon className="text-muted-foreground h-3.5 w-3.5 shrink-0" />
                <span className="text-primary min-w-0 flex-1 truncate font-medium underline-offset-2 hover:underline">
                  {ref.path}
                </span>
                <span className="text-muted-foreground shrink-0 text-[10px]">
                  {t(kindLabelKey(ref.kind))}
                </span>
                {loading ? (
                  <Loader2 className="h-3 w-3 shrink-0 animate-spin" />
                ) : null}
              </button>
            </li>
          );
        })}
      </ul>
      {error ? (
        <p className="text-destructive mt-2 text-[11px] leading-snug">{error}</p>
      ) : null}

      {preview && previewExpanded ? (
        preview.mime_type === "text/markdown" || isMarkdownPath(preview.path) ? (
          <MarkdownPreviewDialog
            preview={preview}
            onClose={() => {
              setPreviewExpanded(false);
              setPreview(null);
            }}
          />
        ) : (
          <Modal
            open
            onClose={() => {
              setPreviewExpanded(false);
              setPreview(null);
            }}
            labelledBy="file-preview-title"
            className="flex max-h-[min(90vh,900px)] max-w-4xl flex-col overflow-hidden p-0"
          >
            <div className="flex shrink-0 items-center justify-between gap-3 border-b px-4 py-3">
              <p id="file-preview-title" className="min-w-0 truncate text-sm font-semibold">
                {preview.path.split(/[\\/]/).at(-1) ?? preview.path}
              </p>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => {
                  setPreviewExpanded(false);
                  setPreview(null);
                }}
              >
                {t("common.close")}
              </Button>
            </div>
            <div className="min-h-0 flex-1 overflow-auto p-4">
              <ArtifactPreviewView preview={preview} />
            </div>
          </Modal>
        )
      ) : null}
    </div>
  );
}

/** Lightweight clickable path chip for inline use (markdown code spans). */
export function FilePathButton({
  path,
  workspaceId,
  className,
}: {
  path: string;
  workspaceId: WorkspaceId;
  className?: string;
}) {
  const { t } = useTranslation();
  const [preview, setPreview] = useState<ArtifactPreview | null>(null);
  const [loading, setLoading] = useState(false);

  return (
    <>
      <button
        type="button"
        className={cn(
          "text-primary inline font-mono text-[0.9em] underline decoration-primary/40 underline-offset-2 hover:decoration-primary",
          className,
        )}
        disabled={loading}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          if (!isPreviewablePath(path)) return;
          setLoading(true);
          const job = isMarkdownPath(path)
            ? previewWorkspaceMarkdown(workspaceId, path, null)
            : previewArtifact(workspaceId, path).catch(() =>
                previewWorkspaceMarkdown(workspaceId, path, null),
              );
          void job
            .then(setPreview)
            .catch(() => undefined)
            .finally(() => setLoading(false));
        }}
        title={t("agent.fileChange.openHint")}
      >
        {loading ? "…" : path}
      </button>
      {preview && isMarkdownPath(preview.path) ? (
        <MarkdownPreviewDialog preview={preview} onClose={() => setPreview(null)} />
      ) : null}
      {preview && !isMarkdownPath(preview.path) ? (
        <Modal
          open
          onClose={() => setPreview(null)}
          labelledBy="inline-file-preview-title"
          className="flex max-h-[min(90vh,900px)] max-w-4xl flex-col overflow-hidden p-0"
        >
          <div className="flex shrink-0 items-center justify-between gap-3 border-b px-4 py-3">
            <p id="inline-file-preview-title" className="truncate text-sm font-semibold">
              {preview.path.split(/[\\/]/).at(-1)}
            </p>
            <Button type="button" size="sm" variant="ghost" onClick={() => setPreview(null)}>
              {t("common.close")}
            </Button>
          </div>
          <div className="min-h-0 flex-1 overflow-auto p-4">
            <ArtifactPreviewView preview={preview} />
          </div>
        </Modal>
      ) : null}
    </>
  );
}
