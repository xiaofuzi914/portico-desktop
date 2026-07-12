import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { ArtifactPreview as ArtifactPreviewComponent } from "@/components/artifact/artifact-preview";
import { listRunEvents, parseRuntimeEvent, previewArtifact } from "@/lib/tauri-api";
import type { AgentRunId, Artifact, ArtifactPreview, ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { EmptyState, InlineError, PanelLoading } from "./panel-primitives";
import { runKeys } from "@/lib/query-keys";

interface ArtifactsPanelProps {
  workspaceId: WorkspaceId;
  threadId: ThreadId;
  runId?: AgentRunId;
}

export function ArtifactsPanel({ workspaceId, threadId, runId }: ArtifactsPanelProps) {
  const { t } = useTranslation();
  const {
    data: events,
    isLoading,
    error,
  } = useQuery({
    queryKey: runKeys.events(workspaceId, threadId, runId),
    queryFn: () => {
      if (!runId) throw new Error("No active run");
      return listRunEvents(runId);
    },
    enabled: !!runId,
  });

  const artifactEvents = useMemo(() => {
    if (!events) return [];
    return events
      .map((event) => {
        const runtime = parseRuntimeEvent(event.payload);
        if (runtime?.kind !== "ArtifactCreated") return null;
        return runtime.data.artifact;
      })
      .filter((artifact): artifact is Artifact => artifact !== null);
  }, [events]);

  if (!runId) return <EmptyState message={t("inspector.startRunArtifacts")} />;
  if (isLoading) return <PanelLoading />;
  if (error) {
    return <InlineError title={t("inspector.loadRunEventsFailed")} message={error.message} />;
  }

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-3">
      <p className="text-muted-foreground text-xs">{t("inspector.artifactsNote")}</p>
      {artifactEvents.length === 0 && (
        <p className="text-muted-foreground text-xs">{t("inspector.noArtifacts")}</p>
      )}
      {artifactEvents.map((artifact) => (
        <ArtifactItem key={artifact.id} workspaceId={workspaceId} artifact={artifact} />
      ))}
    </div>
  );
}

function ArtifactItem({ workspaceId, artifact }: { workspaceId: WorkspaceId; artifact: Artifact }) {
  const { t } = useTranslation();
  const [preview, setPreview] = useState<ArtifactPreview | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);

  function handlePreview() {
    if (!artifact.path) return;
    setPreviewError(null);
    void previewArtifact(workspaceId, artifact.path)
      .then(setPreview)
      .catch((err: unknown) => setPreviewError(err instanceof Error ? err.message : String(err)));
  }

  return (
    <div className="rounded border p-2">
      <div className="flex items-center justify-between">
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium">{artifact.name}</p>
          <p className="text-muted-foreground truncate text-[10px]">{artifact.mime_type}</p>
        </div>
        {artifact.path && (
          <Button size="sm" variant="outline" className="h-7 text-xs" onClick={handlePreview}>
            {t("inspector.preview")}
          </Button>
        )}
      </div>
      {artifact.content_preview && (
        <pre className="text-muted-foreground mt-2 max-h-32 overflow-auto text-[10px] whitespace-pre-wrap">
          {artifact.content_preview}
        </pre>
      )}
      {previewError && <p className="mt-2 text-xs text-red-600">{previewError}</p>}
      {preview && (
        <div className="mt-2">
          <ArtifactPreviewComponent preview={preview} />
        </div>
      )}
    </div>
  );
}
