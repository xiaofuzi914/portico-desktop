import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import {
  createMemory,
  deleteMemory,
  getRagIndexStatus,
  listMemories,
  rebuildRagIndex,
  refreshChangedRagDocuments,
  updateMemory,
} from "@/lib/tauri-api";
import { asWorkspaceId, type MemoryItem, type MemoryScope } from "@/lib/schemas";
import { useState } from "react";
import { useTranslation } from "@/lib/i18n-react";
import { ragKeys, workspaceKeys } from "@/lib/query-keys";

export const Route = createFileRoute("/workspaces/$workspaceId/memory/")({
  component: MemoryPage,
});

/** Project memory form only offers long-lived scopes with valid workspace binding. */
const SCOPES: MemoryScope[] = ["Workspace", "User"];

function MemoryPage() {
  const { t } = useTranslation();
  const { workspaceId: workspaceIdParam } = Route.useParams();
  const workspaceId = asWorkspaceId(workspaceIdParam);
  const queryClient = useQueryClient();

  const [nlText, setNlText] = useState("");
  const [sensitive, setSensitive] = useState(false);
  const [scope, setScope] = useState<MemoryScope>("Workspace");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");

  const { data: memories, isLoading } = useQuery({
    queryKey: [...workspaceKeys.memories(workspaceId), scope],
    queryFn: () =>
      listMemories(scope, scope === "Workspace" ? workspaceId : null, null),
  });

  const ragQuery = useQuery({
    queryKey: ragKeys.workspaceStatus(workspaceId),
    queryFn: () => getRagIndexStatus(workspaceId),
  });

  const create = useMutation({
    mutationFn: () => {
      const value = nlText.trim();
      const key =
        value.length > 48
          ? value.slice(0, 48).replace(/\s+/g, "_")
          : value.replace(/\s+/g, "_") || "project_preference";
      return createMemory(
        scope,
        scope === "Workspace" ? workspaceId : null,
        null,
        key,
        value,
        sensitive,
      );
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.memories(workspaceId) });
      setNlText("");
      setSensitive(false);
    },
  });

  const update = useMutation({
    mutationFn: ({ id, value }: { id: MemoryItem["id"]; value: string }) => updateMemory(id, value),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.memories(workspaceId) });
      setEditingId(null);
      setEditValue("");
    },
  });

  const remove = useMutation({
    mutationFn: (id: MemoryItem["id"]) => deleteMemory(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.memories(workspaceId) });
    },
  });

  const rebuildIndex = useMutation({
    mutationFn: () => rebuildRagIndex(workspaceId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ragKeys.workspaceStatus(workspaceId) });
    },
  });

  const refreshIndex = useMutation({
    mutationFn: () => refreshChangedRagDocuments(workspaceId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ragKeys.workspaceStatus(workspaceId) });
    },
  });

  function startEdit(memory: MemoryItem) {
    setEditingId(memory.id);
    setEditValue(memory.value);
  }

  const rag = ragQuery.data;

  return (
    <main className="container mx-auto max-w-4xl space-y-6 p-6">
      <div className="mb-2">
        <Link
          to="/workspaces/$workspaceId"
          params={{ workspaceId }}
          className="text-muted-foreground text-sm hover:underline"
        >
          ← {t("common.workspace")}
        </Link>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{t("memory.center.projectPreferences")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              create.mutate();
            }}
          >
            <Textarea
              placeholder={t("memory.center.rememberPlaceholder")}
              value={nlText}
              onChange={(e) => setNlText(e.target.value)}
              rows={3}
              required
            />
            <div className="flex flex-wrap items-center gap-3 text-sm">
              {SCOPES.map((s) => (
                <label key={s} className="flex items-center gap-1.5">
                  <input
                    type="radio"
                    name="ws-scope"
                    checked={scope === s}
                    onChange={() => setScope(s)}
                  />
                  {s === "Workspace"
                    ? t("memory.center.scopeThisProject")
                    : t("memory.center.scopeAllProjects")}
                </label>
              ))}
              <label className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={sensitive}
                  onChange={(e) => setSensitive(e.target.checked)}
                  className="h-4 w-4"
                />
                {t("memory.sensitive")}
              </label>
            </div>
            <Button type="submit" disabled={create.isPending || !nlText.trim()}>
              {t("memory.saveMemory")}
            </Button>
          </form>

          {isLoading ? (
            <p className="text-muted-foreground text-sm">{t("common.loading")}</p>
          ) : (memories ?? []).length === 0 ? (
            <p className="text-muted-foreground text-sm">{t("memory.empty")}</p>
          ) : (
            <ul className="space-y-2">
              {(memories ?? []).map((memory) => (
                <li key={memory.id} className="rounded-md border p-3">
                  {editingId === memory.id ? (
                    <div className="space-y-2">
                      <Textarea
                        value={editValue}
                        onChange={(e) => setEditValue(e.target.value)}
                        rows={2}
                      />
                      <div className="flex gap-2">
                        <Button
                          size="sm"
                          onClick={() =>
                            update.mutate({ id: memory.id, value: editValue.trim() })
                          }
                        >
                          {t("common.save")}
                        </Button>
                        <Button size="sm" variant="outline" onClick={() => setEditingId(null)}>
                          {t("common.cancel")}
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <p className="text-sm whitespace-pre-wrap">{memory.value}</p>
                        <p className="text-muted-foreground mt-1 text-[11px]">
                          {memory.scope}
                          {memory.sensitive ? ` · ${t("memory.sensitiveBadge")}` : ""}
                          {memory.use_count ? ` · used ${memory.use_count}` : ""}
                        </p>
                      </div>
                      <div className="flex shrink-0 gap-1">
                        <Button size="sm" variant="outline" onClick={() => startEdit(memory)}>
                          {t("common.edit")}
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => remove.mutate(memory.id)}
                        >
                          {t("memory.delete")}
                        </Button>
                      </div>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("memory.center.projectKnowledge")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          {rag ? (
            <p className="text-muted-foreground text-sm">
              indexed {rag.indexed} · stale {rag.stale} · failed {rag.failed} · total{" "}
              {rag.total_documents}
              <br />
              <span className="text-[11px]">
                {rag.embedding_provider_id} · dim {rag.dimension}
              </span>
            </p>
          ) : (
            <p className="text-muted-foreground text-sm">{t("common.loading")}</p>
          )}
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              disabled={refreshIndex.isPending}
              onClick={() => refreshIndex.mutate()}
            >
              {t("inspector.refreshRag")}
            </Button>
            <Button
              size="sm"
              variant="outline"
              disabled={rebuildIndex.isPending}
              onClick={() => {
                if (window.confirm(t("memory.center.rebuildConfirm"))) {
                  rebuildIndex.mutate();
                }
              }}
            >
              {rebuildIndex.isPending ? t("memory.rebuildingIndex") : t("memory.rebuildIndex")}
            </Button>
          </div>
        </CardContent>
      </Card>
    </main>
  );
}
