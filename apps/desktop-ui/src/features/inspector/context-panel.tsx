import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Input } from "@/components/ui/input";
import { inspectContext, listWorkspaces } from "@/lib/tauri-api";
import type { AgentRunId, ThreadId, WorkspaceId } from "@/lib/schemas";
import type { ReactNode } from "react";
import { useTranslation } from "@/lib/i18n-react";

interface ContextPanelProps {
  workspaceId: WorkspaceId;
  threadId: ThreadId;
  runId?: AgentRunId;
}

export function ContextPanel({ workspaceId, threadId, runId }: ContextPanelProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");

  const {
    data: workspaces,
    isLoading: loadingWorkspaces,
    error: workspacesError,
  } = useQuery({
    queryKey: ["workspaces"],
    queryFn: listWorkspaces,
  });

  const workspace = useMemo(
    () => workspaces?.find((w) => w.id === workspaceId),
    [workspaces, workspaceId],
  );

  const {
    data: context,
    isLoading: loadingContext,
    error: contextError,
  } = useQuery({
    queryKey: [
      "workspaces",
      workspaceId,
      "threads",
      threadId,
      "runs",
      runId ?? "none",
      "context",
      query,
    ],
    queryFn: () => {
      if (!workspace || !runId) {
        throw new Error("Missing workspace root or run");
      }
      return inspectContext(runId, threadId, workspaceId, workspace.root_path, query);
    },
    enabled: !!workspace && !!runId,
  });

  if (workspacesError) {
    return <InlineError title={t("inspector.loadWorkspaceFailed")} message={workspacesError.message} />;
  }
  if (loadingWorkspaces) return <PanelLoading />;
  if (!runId) return <EmptyState message={t("inspector.startRunContext")} />;
  if (contextError) {
    return <InlineError title={t("inspector.inspectContextFailed")} message={contextError.message} />;
  }

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-3">
      <Input
        placeholder={t("inspector.contextQuery")}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        className="h-8 text-xs"
      />
      {loadingContext && <PanelLoading />}
      {context && (
        <>
          <MetaCard title={t("inspector.estimatedTokens")} value={String(context.estimated_tokens)} />
          {context.privacy_flags.length > 0 && (
            <div className="flex flex-wrap gap-1">
              {context.privacy_flags.map((flag) => (
                <span
                  key={flag}
                  className="bg-muted text-muted-foreground rounded px-1.5 py-0.5 text-[10px]"
                >
                  {flag}
                </span>
              ))}
            </div>
          )}
          <Section title={t("inspector.instructions")}>
            {context.instructions.length === 0 ? (
              <p className="text-muted-foreground text-xs">{t("inspector.noInstructions")}</p>
            ) : (
              context.instructions.map((file) => (
                <div key={file.path} className="rounded border p-2">
                  <p className="text-muted-foreground text-[10px] font-medium">{file.path}</p>
                  <pre className="max-h-32 overflow-auto text-xs whitespace-pre-wrap">
                    {file.content}
                  </pre>
                </div>
              ))
            )}
          </Section>
          <Section title={t("inspector.memory")}>
            {context.memories.length === 0 ? (
              <p className="text-muted-foreground text-xs">{t("inspector.noMemory")}</p>
            ) : (
              context.memories.map((memory) => (
                <div key={memory.id} className="rounded border p-2">
                  <p className="text-[10px] font-medium">{memory.key}</p>
                  <p className="text-xs">{memory.value}</p>
                </div>
              ))
            )}
          </Section>
          <Section title={t("inspector.ragChunks")}>
            {context.rag_chunks.length === 0 ? (
              <p className="text-muted-foreground text-xs">{t("inspector.noRag")}</p>
            ) : (
              context.rag_chunks.map((chunk) => (
                <div key={chunk.id} className="rounded border p-2">
                  <p className="text-muted-foreground text-[10px]">
                    {chunk.document_path} #{chunk.chunk_index} · score {chunk.score.toFixed(3)}
                  </p>
                  <p className="text-xs">{chunk.content}</p>
                </div>
              ))
            )}
          </Section>
        </>
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="space-y-2">
      <h4 className="text-muted-foreground text-xs font-semibold">{title}</h4>
      <div className="space-y-2">{children}</div>
    </div>
  );
}

function MetaCard({ title, value }: { title: string; value: string }) {
  return (
    <div className="rounded border p-2">
      <p className="text-muted-foreground text-[10px]">{title}</p>
      <p className="text-sm font-medium">{value}</p>
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

function EmptyState({ message }: { message: string }) {
  return <p className="text-muted-foreground p-3 text-xs">{message}</p>;
}
