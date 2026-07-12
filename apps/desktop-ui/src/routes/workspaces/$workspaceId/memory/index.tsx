import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  createMemory,
  deleteMemory,
  listMemories,
  rebuildRagIndex,
  updateMemory,
} from "@/lib/tauri-api";
import { asWorkspaceId, memoryScopeSchema, type MemoryItem, type MemoryScope } from "@/lib/schemas";
import { useMemo, useState } from "react";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";

export const Route = createFileRoute("/workspaces/$workspaceId/memory/")({
  component: MemoryPage,
});

const SCOPES: MemoryScope[] = ["Session", "Thread", "Workspace", "User"];

function MemoryPage() {
  const { t } = useTranslation();
  const { workspaceId: workspaceIdParam } = Route.useParams();
  const workspaceId = asWorkspaceId(workspaceIdParam);
  const queryClient = useQueryClient();

  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const [sensitive, setSensitive] = useState(false);
  const [scope, setScope] = useState<MemoryScope>("Workspace");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");

  const { data: memories, isLoading } = useQuery({
    queryKey: workspaceKeys.memories(workspaceId),
    queryFn: () => listMemories(scope, workspaceId, null),
  });

  const create = useMutation({
    mutationFn: () => createMemory(scope, workspaceId, null, key, value, sensitive),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.memories(workspaceId) });
      setKey("");
      setValue("");
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
  });

  const scopeOptions = useMemo(
    () =>
      SCOPES.map((s) => ({
        value: s,
        label:
          s === "Session"
            ? t("memory.scope.session")
            : s === "Thread"
              ? t("memory.scope.thread")
              : s === "Workspace"
                ? t("memory.scope.workspace")
                : t("memory.scope.user"),
      })),
    [t],
  );

  function startEdit(memory: MemoryItem) {
    setEditingId(memory.id);
    setEditValue(memory.value);
  }

  return (
    <main className="container mx-auto max-w-4xl p-6">
      <div className="mb-4">
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
          <CardTitle>{t("memory.title")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              create.mutate();
            }}
          >
            <div className="flex gap-2">
              <select
                value={scope}
                onChange={(e) => setScope(memoryScopeSchema.parse(e.target.value))}
                className="border-input bg-background h-10 rounded-md border px-3 text-sm"
              >
                {scopeOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
              <Input
                placeholder={t("memory.key")}
                value={key}
                onChange={(e) => setKey(e.target.value)}
                required
              />
            </div>
            <Textarea
              placeholder={t("memory.value")}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              rows={3}
              required
            />
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={sensitive}
                onChange={(e) => setSensitive(e.target.checked)}
                className="h-4 w-4"
              />
              {t("memory.sensitive")}
            </label>
            <Button type="submit" disabled={create.isPending || !key.trim()}>
              {t("memory.saveMemory")}
            </Button>
          </form>

          <div>
            <div className="mb-2 flex items-center justify-between gap-4">
              <h3 className="text-lg font-semibold">{t("memory.stored")}</h3>
              <Button
                size="sm"
                variant="outline"
                onClick={() => rebuildIndex.mutate()}
                disabled={rebuildIndex.isPending}
              >
                {rebuildIndex.isPending ? t("memory.rebuildingIndex") : t("memory.rebuildIndex")}
              </Button>
            </div>
            {rebuildIndex.error && (
              <p className="text-destructive mb-3 text-sm">
                {rebuildIndex.error instanceof Error
                  ? rebuildIndex.error.message
                  : String(rebuildIndex.error)}
              </p>
            )}
            {isLoading ? (
              <p className="text-muted-foreground">{t("memory.loading")}</p>
            ) : memories?.length ? (
              <ul className="space-y-3">
                {memories.map((memory) => (
                  <li key={memory.id} className="rounded-md border p-3">
                    <div className="flex items-start justify-between gap-2">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="font-medium">{memory.key}</span>
                          <span className="bg-muted text-muted-foreground rounded px-1.5 py-0.5 text-xs">
                            {memory.scope}
                          </span>
                          {memory.sensitive && (
                            <span className="bg-destructive/10 text-destructive rounded px-1.5 py-0.5 text-xs">
                              {t("memory.sensitiveBadge")}
                            </span>
                          )}
                        </div>
                        {editingId === memory.id ? (
                          <div className="mt-2 space-y-2">
                            <Textarea
                              value={editValue}
                              onChange={(e) => setEditValue(e.target.value)}
                              rows={3}
                            />
                            <div className="flex gap-2">
                              <Button
                                size="sm"
                                onClick={() => update.mutate({ id: memory.id, value: editValue })}
                                disabled={update.isPending}
                              >
                                {t("memory.save")}
                              </Button>
                              <Button
                                size="sm"
                                variant="outline"
                                onClick={() => {
                                  setEditingId(null);
                                  setEditValue("");
                                }}
                              >
                                {t("memory.cancel")}
                              </Button>
                            </div>
                          </div>
                        ) : (
                          <p className="text-muted-foreground mt-1 text-sm whitespace-pre-wrap">
                            {memory.value}
                          </p>
                        )}
                      </div>
                      {editingId !== memory.id && (
                        <div className="flex gap-2">
                          <Button size="sm" variant="outline" onClick={() => startEdit(memory)}>
                            {t("memory.edit")}
                          </Button>
                          <Button
                            size="sm"
                            variant="destructive"
                            onClick={() => remove.mutate(memory.id)}
                            disabled={remove.isPending}
                          >
                            {t("memory.delete")}
                          </Button>
                        </div>
                      )}
                    </div>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-muted-foreground">{t("memory.empty")}</p>
            )}
          </div>
        </CardContent>
      </Card>
    </main>
  );
}
