import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Brain, FolderKanban, Sparkles, UserRound } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ErrorAlert } from "@/components/ui/error-alert";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { typography } from "@/components/ui/typography";
import { useTranslation } from "@/lib/i18n-react";
import {
  createMemory,
  deleteMemory,
  listMemories,
  listWorkflowPatterns,
  listWorkspaces,
  muteWorkflowPattern,
} from "@/lib/tauri-api";
import type { MemoryItem, WorkflowPattern, Workspace } from "@/lib/schemas";

/**
 * Capabilities → Memory
 *
 * Surfaces two loosely coupled memory planes:
 * 1) Fact memory (key/value) — User / Workspace
 * 2) Workflow patterns — habits that condition multi-agent planning
 *
 * Project-scoped memory remains reachable from each project page; this center
 * is for cross-project habits + overview.
 */
export function MemoryCapabilitiesPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [factKey, setFactKey] = useState("");
  const [factValue, setFactValue] = useState("");

  const workspacesQuery = useQuery({
    queryKey: ["workspaces"],
    queryFn: listWorkspaces,
  });

  const userFactsQuery = useQuery({
    queryKey: ["memories", "User", null],
    queryFn: () => listMemories("User", null, null),
  });

  const userPatternsQuery = useQuery({
    queryKey: ["workflow-patterns", "User", null],
    queryFn: () => listWorkflowPatterns("User", null),
  });

  const createUserFact = useMutation({
    mutationFn: () => createMemory("User", null, null, factKey.trim(), factValue.trim(), false),
    onSuccess: async () => {
      setFactKey("");
      setFactValue("");
      await queryClient.invalidateQueries({ queryKey: ["memories", "User", null] });
    },
  });

  const removeFact = useMutation({
    mutationFn: (id: MemoryItem["id"]) => deleteMemory(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["memories"] });
    },
  });

  const mutePattern = useMutation({
    mutationFn: (id: WorkflowPattern["id"]) => muteWorkflowPattern(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["workflow-patterns"] });
    },
  });

  const workspaces = workspacesQuery.data ?? [];
  const userFacts = userFactsQuery.data ?? [];
  const userPatterns = userPatternsQuery.data ?? [];

  return (
    <div className="space-y-6">
      <section className="bg-muted/20 rounded-lg border p-4">
        <div className="mb-2 flex items-center gap-2 text-sm font-semibold">
          <Brain className="h-4 w-4" />
          {t("memory.capabilities.architectureTitle")}
        </div>
        <p className={typography.pageDescription}>{t("memory.capabilities.architectureBody")}</p>
        <div className="mt-4 grid gap-3 sm:grid-cols-3">
          <LayerCard
            icon={UserRound}
            title={t("memory.capabilities.layerUser")}
            body={t("memory.capabilities.layerUserBody")}
          />
          <LayerCard
            icon={FolderKanban}
            title={t("memory.capabilities.layerProject")}
            body={t("memory.capabilities.layerProjectBody")}
          />
          <LayerCard
            icon={Sparkles}
            title={t("memory.capabilities.layerPatterns")}
            body={t("memory.capabilities.layerPatternsBody")}
          />
        </div>
      </section>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("memory.capabilities.userFactsTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-muted-foreground text-sm">{t("memory.capabilities.userFactsBody")}</p>
          <form
            className="space-y-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (!factKey.trim() || !factValue.trim()) return;
              createUserFact.mutate();
            }}
          >
            <Input
              placeholder={t("memory.key")}
              value={factKey}
              onChange={(e) => setFactKey(e.target.value)}
            />
            <Textarea
              placeholder={t("memory.value")}
              value={factValue}
              onChange={(e) => setFactValue(e.target.value)}
              rows={2}
            />
            <Button type="submit" disabled={createUserFact.isPending || !factKey.trim()}>
              {t("memory.saveMemory")}
            </Button>
          </form>
          {createUserFact.error && (
            <ErrorAlert
              title={t("memory.capabilities.saveFailed")}
              message={
                createUserFact.error instanceof Error
                  ? createUserFact.error.message
                  : String(createUserFact.error)
              }
            />
          )}
          <FactList
            items={userFacts}
            empty={t("memory.capabilities.userFactsEmpty")}
            onDelete={(id) => removeFact.mutate(id)}
            deleting={removeFact.isPending}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("memory.capabilities.userPatternsTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <p className="text-muted-foreground text-sm">
            {t("memory.capabilities.userPatternsBody")}
          </p>
          <PatternList
            items={userPatterns}
            empty={t("memory.capabilities.patternsEmpty")}
            onMute={(id) => mutePattern.mutate(id)}
            muting={mutePattern.isPending}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("memory.capabilities.projectMemoryTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <p className="text-muted-foreground text-sm">
            {t("memory.capabilities.projectMemoryBody")}
          </p>
          {workspacesQuery.isLoading ? (
            <p className="text-muted-foreground text-sm">{t("common.loading")}</p>
          ) : workspaces.length === 0 ? (
            <p className="text-muted-foreground text-sm">{t("memory.capabilities.noProjects")}</p>
          ) : (
            <ul className="space-y-2">
              {workspaces.map((workspace) => (
                <ProjectMemoryRow key={workspace.id} workspace={workspace} />
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function LayerCard({
  icon: Icon,
  title,
  body,
}: {
  icon: typeof Brain;
  title: string;
  body: string;
}) {
  return (
    <div className="bg-background rounded-md border p-3">
      <div className="mb-1 flex items-center gap-1.5 text-sm font-medium">
        <Icon className="h-3.5 w-3.5" />
        {title}
      </div>
      <p className="text-muted-foreground text-xs leading-5">{body}</p>
    </div>
  );
}

function FactList({
  items,
  empty,
  onDelete,
  deleting,
}: {
  items: MemoryItem[];
  empty: string;
  onDelete: (id: MemoryItem["id"]) => void;
  deleting: boolean;
}) {
  const { t } = useTranslation();
  if (!items.length) {
    return <p className="text-muted-foreground text-sm">{empty}</p>;
  }
  return (
    <ul className="space-y-2">
      {items.map((item) => (
        <li key={item.id} className="flex items-start justify-between gap-3 rounded-md border p-3">
          <div className="min-w-0">
            <div className="font-medium">{item.key}</div>
            <p className="text-muted-foreground mt-1 text-sm whitespace-pre-wrap">{item.value}</p>
          </div>
          <Button size="sm" variant="outline" disabled={deleting} onClick={() => onDelete(item.id)}>
            {t("memory.delete")}
          </Button>
        </li>
      ))}
    </ul>
  );
}

function PatternList({
  items,
  empty,
  onMute,
  muting,
}: {
  items: WorkflowPattern[];
  empty: string;
  onMute: (id: WorkflowPattern["id"]) => void;
  muting: boolean;
}) {
  const { t } = useTranslation();
  if (!items.length) {
    return <p className="text-muted-foreground text-sm">{empty}</p>;
  }
  return (
    <ul className="space-y-2">
      {items.map((pattern) => (
        <li key={pattern.id} className="rounded-md border p-3 text-sm">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-medium">{pattern.name}</span>
                <span className="bg-muted text-muted-foreground rounded px-1.5 py-0.5 text-[10px] uppercase">
                  {pattern.status}
                </span>
                <span className="text-muted-foreground text-xs">
                  strength {pattern.strength.toFixed(1)} · ✓{pattern.success_count} · ✗
                  {pattern.failure_count}
                </span>
              </div>
              <p className="text-muted-foreground mt-1 text-xs">{pattern.summary}</p>
              {pattern.preferred_roles.length > 0 && (
                <p className="mt-1 text-xs">
                  {t("orchestration.suggestedRoles")}: {pattern.preferred_roles.join(" → ")}
                </p>
              )}
              {pattern.trigger_text && (
                <p className="text-muted-foreground mt-1 text-[11px]">
                  triggers: {pattern.trigger_text}
                </p>
              )}
            </div>
            {pattern.status !== "muted" && (
              <Button
                size="sm"
                variant="outline"
                disabled={muting}
                onClick={() => onMute(pattern.id)}
              >
                {t("memory.capabilities.mutePattern")}
              </Button>
            )}
          </div>
        </li>
      ))}
    </ul>
  );
}

function ProjectMemoryRow({ workspace }: { workspace: Workspace }) {
  const { t } = useTranslation();
  const patternsQuery = useQuery({
    queryKey: ["workflow-patterns", "Workspace", workspace.id],
    queryFn: () => listWorkflowPatterns("Workspace", workspace.id),
  });
  const factsQuery = useQuery({
    queryKey: ["memories", "Workspace", workspace.id],
    queryFn: () => listMemories("Workspace", workspace.id, null),
  });

  const patternCount = patternsQuery.data?.length ?? 0;
  const factCount = factsQuery.data?.length ?? 0;

  return (
    <li className="flex items-center justify-between gap-3 rounded-md border p-3">
      <div className="min-w-0">
        <div className="truncate font-medium">{workspace.name}</div>
        <p className="text-muted-foreground text-xs">
          {t("memory.capabilities.factsLabel")}: {factCount} ·{" "}
          {t("memory.capabilities.patternsLabel")}: {patternCount}
        </p>
      </div>
      <Link
        to="/workspaces/$workspaceId/memory"
        params={{ workspaceId: workspace.id }}
        className="text-primary shrink-0 text-sm hover:underline"
      >
        {t("memory.capabilities.openProjectMemory")}
      </Link>
    </li>
  );
}
