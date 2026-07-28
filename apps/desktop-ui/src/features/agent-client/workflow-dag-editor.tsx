import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { GitBranch, Loader2, Plus, Save, Trash2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ErrorAlert } from "@/components/ui/error-alert";
import { useTranslation } from "@/lib/i18n-react";
import type {
  OrchestrationStageKind,
  WorkflowTemplate,
  WorkspaceId,
} from "@/lib/schemas";
import {
  deleteWorkflowTemplate,
  listWorkflowTemplates,
  saveWorkflowTemplate,
} from "@/lib/tauri-api";
import { cn } from "@/lib/utils";
import {
  applyStageEdit,
  newBlankStage,
  removeStage,
  stageToDraft,
  type StageEditDraft,
} from "./workflow-dag-model";
import {
  catalogDisplayOrder,
  isKnownCatalogKey,
  resolveCatalogCopy,
} from "./workflow-catalog-copy";

const KINDS: OrchestrationStageKind[] = ["single", "foreach", "reduce", "loop"];

interface WorkflowDagEditorProps {
  workspaceId?: WorkspaceId;
  open: boolean;
  onClose: () => void;
  /** Start multi-agent with catalog key or template UUID. */
  onStart: (workflowId: string, title: string) => void;
  /** Optional pre-selected template id / catalog key. */
  initialKey?: string | null;
}

function emptyUserTemplate(
  workspaceId: WorkspaceId | undefined,
  title: string,
  summary: string,
): WorkflowTemplate {
  const now = new Date().toISOString();
  return {
    id: crypto.randomUUID() as WorkflowTemplate["id"],
    catalog_key: null,
    title,
    summary,
    stages: [
      {
        id: "plan",
        kind: "single",
        title: "Plan",
        agent_name: "planner",
        status: "Pending",
        prompt_template: "Task:\n{task}\n\nProduce a short plan.",
        depends_on: [],
        body_stage_ids: [],
        tasks: [],
      },
      {
        id: "execute",
        kind: "single",
        title: "Execute",
        agent_name: "worker",
        status: "Pending",
        prompt_template: "Task:\n{task}\n\nPlan:\n{upstream}\n\nExecute.",
        depends_on: ["plan"],
        body_stage_ids: [],
        tasks: [],
      },
    ],
    builtin: false,
    workspace_id: workspaceId ?? null,
    created_at: now,
    updated_at: now,
  };
}

function kindLabel(kind: OrchestrationStageKind, t: (k: string) => string): string {
  return t(`orchestration.stageKind.${kind}`);
}

/** Product-facing title/summary for list + form (built-ins localized). */
function displayMeta(
  catalogKey: string | null | undefined,
  serverTitle: string,
  serverSummary: string,
  t: (k: string) => string,
): { title: string; summary: string; when: string; difference: string } {
  const id = catalogKey ?? "";
  const copy = resolveCatalogCopy(id, t, serverTitle, serverSummary);
  // Summary field = "when to use"; prefer when for known keys.
  const summary = isKnownCatalogKey(id)
    ? `${copy.when} · ${copy.difference}`
    : serverSummary || copy.when;
  return {
    title: copy.title,
    summary,
    when: copy.when,
    difference: copy.difference,
  };
}

export function WorkflowDagEditor({
  workspaceId,
  open,
  onClose,
  onStart,
  initialKey = null,
}: WorkflowDagEditorProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [selectedKey, setSelectedKey] = useState<string | null>(initialKey);
  const [draftTitle, setDraftTitle] = useState("");
  const [draftSummary, setDraftSummary] = useState("");
  const [stages, setStages] = useState<StageEditDraft[]>([]);
  const [templateId, setTemplateId] = useState<string | null>(null);
  const [isBuiltin, setIsBuiltin] = useState(false);
  const [catalogKey, setCatalogKey] = useState<string | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const [hydrated, setHydrated] = useState(false);
  const [showTechIds, setShowTechIds] = useState(false);

  const templatesQuery = useQuery({
    queryKey: ["workflow-templates", workspaceId],
    queryFn: () => listWorkflowTemplates(workspaceId),
    enabled: open,
  });

  const templates = useMemo(() => {
    const list = templatesQuery.data ?? [];
    return [...list].sort((a, b) =>
      catalogDisplayOrder(a.catalog_key ?? a.id, b.catalog_key ?? b.id),
    );
  }, [templatesQuery.data]);

  const loadTemplateIntoEditor = (picked: WorkflowTemplate) => {
    const key = picked.catalog_key ?? picked.id;
    const meta = displayMeta(picked.catalog_key, picked.title, picked.summary, t);
    setTemplateId(picked.id);
    setCatalogKey(picked.catalog_key ?? null);
    setIsBuiltin(picked.builtin);
    setDraftTitle(meta.title);
    setDraftSummary(meta.summary);
    setStages((picked.stages ?? []).map(stageToDraft));
    setSelectedKey(key);
    setHydrated(true);
    setLocalError(null);
  };

  // Initial hydrate when dialog opens and templates arrive.
  useEffect(() => {
    if (!open || hydrated || !templatesQuery.data) return;
    const list = templatesQuery.data;
    const preferred = initialKey
      ? list.find(
          (x) =>
            x.id === initialKey ||
            x.catalog_key === initialKey ||
            x.title === initialKey,
        )
      : null;
    const picked =
      preferred ??
      list[0] ??
      emptyUserTemplate(
        workspaceId,
        t("orchestration.customDefaultTitle"),
        t("orchestration.customDefaultSummary"),
      );
    loadTemplateIntoEditor(picked);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- hydrate once per open
  }, [open, hydrated, templatesQuery.data, initialKey, workspaceId, t]);

  useEffect(() => {
    if (!open) setHydrated(false);
  }, [open]);

  const saveMut = useMutation({
    mutationFn: async () => {
      const applied = applyStageEdit(stages);
      if (!applied.ok) throw new Error(applied.error);
      const now = new Date().toISOString();
      // Builtin edits are saved as a user copy (new id, not builtin).
      const id =
        isBuiltin || !templateId
          ? (crypto.randomUUID() as WorkflowTemplate["id"])
          : (templateId as WorkflowTemplate["id"]);
      const template: WorkflowTemplate = {
        id,
        catalog_key: isBuiltin ? null : catalogKey,
        title: draftTitle.trim() || t("orchestration.customDefaultTitle"),
        summary: draftSummary.trim() || t("orchestration.customDefaultSummary"),
        stages: applied.stages,
        builtin: false,
        workspace_id: workspaceId ?? null,
        created_at: now,
        updated_at: now,
      };
      return saveWorkflowTemplate(template);
    },
    onSuccess: async (saved) => {
      setTemplateId(saved.id);
      setCatalogKey(saved.catalog_key ?? null);
      setIsBuiltin(false);
      setSelectedKey(saved.id);
      setDraftTitle(saved.title);
      setDraftSummary(saved.summary);
      setHydrated(true);
      setLocalError(null);
      await queryClient.invalidateQueries({ queryKey: ["workflow-templates"] });
    },
    onError: (e: Error) => setLocalError(e.message),
  });

  const deleteMut = useMutation({
    mutationFn: async () => {
      if (!templateId || isBuiltin) throw new Error("cannot delete builtin");
      await deleteWorkflowTemplate(templateId as WorkflowTemplate["id"]);
    },
    onSuccess: async () => {
      setHydrated(false);
      setSelectedKey(null);
      setTemplateId(null);
      setStages([]);
      await queryClient.invalidateQueries({ queryKey: ["workflow-templates"] });
    },
    onError: (e: Error) => setLocalError(e.message),
  });

  if (!open) return null;

  const validation = applyStageEdit(stages);
  const canStart = validation.ok;

  const startWith = () => {
    if (!validation.ok) {
      setLocalError(validation.error);
      return;
    }
    const wfId = !isBuiltin && templateId ? templateId : catalogKey ?? templateId;
    if (!wfId) {
      setLocalError(t("orchestration.saveBeforeStart"));
      return;
    }
    onStart(wfId, draftTitle || "workflow");
  };

  const updateStage = (index: number, patch: Partial<StageEditDraft>) => {
    setStages((prev) =>
      prev.map((s, i) => (i === index ? { ...s, ...patch } : s)),
    );
    setLocalError(null);
  };

  return (
    <div className="bg-background/95 fixed inset-0 z-50 flex items-end justify-center sm:items-center">
      <button
        type="button"
        className="absolute inset-0 bg-black/40"
        aria-label={t("common.close")}
        onClick={onClose}
      />
      <div
        className="bg-background relative z-10 flex max-h-[90vh] w-full max-w-3xl flex-col overflow-hidden rounded-t-xl border shadow-xl sm:rounded-xl"
        role="dialog"
        aria-modal="true"
        aria-label={t("orchestration.templateLibrary")}
      >
        <header className="flex items-start justify-between gap-3 border-b px-4 py-3">
          <div>
            <h2 className="flex items-center gap-2 text-sm font-semibold">
              <GitBranch className="h-4 w-4" />
              {t("orchestration.templateLibrary")}
            </h2>
            <p className="text-muted-foreground mt-1 text-xs leading-5">
              {t("orchestration.templateLibraryHelp")}
            </p>
          </div>
          <Button type="button" size="sm" variant="ghost" onClick={onClose}>
            <X className="h-4 w-4" />
          </Button>
        </header>

        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4 sm:flex-row">
          {/* Catalog list — same product copy as outer menu */}
          <div className="sm:w-56 sm:shrink-0">
            <p className="text-muted-foreground mb-1.5 text-[10px] font-semibold tracking-wide">
              {t("orchestration.catalog")}
            </p>
            {templatesQuery.isLoading ? (
              <div className="text-muted-foreground flex items-center gap-2 text-xs">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {t("common.loading")}
              </div>
            ) : (
              <ul className="space-y-1.5">
                {templates.map((tpl) => {
                  const key = tpl.catalog_key ?? tpl.id;
                  const meta = displayMeta(tpl.catalog_key, tpl.title, tpl.summary, t);
                  const active =
                    (selectedKey === key || selectedKey === tpl.id) && hydrated;
                  return (
                    <li key={tpl.id}>
                      <button
                        type="button"
                        className={cn(
                          "hover:bg-muted/50 w-full rounded-lg border px-2.5 py-2 text-left text-xs transition-colors",
                          active && "border-primary bg-primary/5",
                        )}
                        onClick={() => loadTemplateIntoEditor(tpl)}
                      >
                        <span className="font-semibold">{meta.title}</span>
                        <span className="text-foreground/80 mt-0.5 block text-[10px] leading-4">
                          {meta.when}
                        </span>
                        <span className="text-muted-foreground mt-0.5 block line-clamp-2 text-[10px] leading-4">
                          {meta.difference}
                        </span>
                        {tpl.builtin ? (
                          <span className="text-muted-foreground mt-1 inline-block text-[9px]">
                            {t("orchestration.builtin")}
                          </span>
                        ) : null}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="mt-2 w-full gap-1"
              onClick={() => {
                const blank = emptyUserTemplate(
                  workspaceId,
                  t("orchestration.customDefaultTitle"),
                  t("orchestration.customDefaultSummary"),
                );
                loadTemplateIntoEditor(blank);
              }}
            >
              <Plus className="h-3.5 w-3.5" />
              {t("orchestration.newTemplate")}
            </Button>
          </div>

          {/* Step editor */}
          <div className="min-w-0 flex-1 space-y-3">
            <div className="grid gap-2 sm:grid-cols-2">
              <label className="flex flex-col gap-1 text-xs">
                <span className="text-muted-foreground font-medium">
                  {t("orchestration.templateTitle")}
                </span>
                <input
                  className="bg-background rounded-md border px-2 py-1.5 text-sm"
                  value={draftTitle}
                  onChange={(e) => setDraftTitle(e.target.value)}
                  disabled={isBuiltin}
                />
              </label>
              <label className="flex flex-col gap-1 text-xs">
                <span className="text-muted-foreground font-medium">
                  {t("orchestration.templateSummary")}
                </span>
                <input
                  className="bg-background rounded-md border px-2 py-1.5 text-sm"
                  value={draftSummary}
                  onChange={(e) => setDraftSummary(e.target.value)}
                  disabled={isBuiltin}
                />
              </label>
            </div>

            {isBuiltin ? (
              <p className="bg-muted/40 text-muted-foreground rounded-md border px-2.5 py-1.5 text-[11px] leading-4">
                {t("orchestration.builtinEditHint")}
              </p>
            ) : null}

            <div className="space-y-2">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="text-muted-foreground text-[10px] font-semibold tracking-wide">
                  {t("orchestration.stages")} ({stages.length})
                </p>
                <div className="flex flex-wrap items-center gap-1">
                  <button
                    type="button"
                    className="text-muted-foreground hover:text-foreground mr-1 text-[10px] underline-offset-2 hover:underline"
                    onClick={() => setShowTechIds((v) => !v)}
                  >
                    {showTechIds
                      ? t("orchestration.hideAdvanced")
                      : t("orchestration.showAdvanced")}
                  </button>
                  {KINDS.map((k) => (
                    <Button
                      key={k}
                      type="button"
                      size="sm"
                      variant="outline"
                      className="h-7 px-2 text-[10px]"
                      title={kindLabel(k, t)}
                      onClick={() =>
                        setStages((prev) => [...prev, newBlankStage(k, prev.length + 1)])
                      }
                    >
                      +{kindLabel(k, t)}
                    </Button>
                  ))}
                </div>
              </div>

              {stages.map((stage, index) => (
                <div key={`${stage.id}-${index}`} className="rounded-lg border p-2.5">
                  <div className="flex flex-wrap items-center gap-2">
                    {showTechIds ? (
                      <input
                        className="bg-background w-28 rounded border px-1.5 py-1 font-mono text-xs"
                        value={stage.id}
                        onChange={(e) => updateStage(index, { id: e.target.value })}
                        title={t("orchestration.field.stageIdHint")}
                        aria-label={t("orchestration.field.stageId")}
                      />
                    ) : (
                      <span className="text-muted-foreground w-6 shrink-0 text-center text-[11px] font-medium tabular-nums">
                        {index + 1}
                      </span>
                    )}
                    <select
                      className="bg-background rounded border px-1.5 py-1 text-xs"
                      value={stage.kind}
                      onChange={(e) =>
                        updateStage(index, {
                          kind: e.target.value as OrchestrationStageKind,
                        })
                      }
                      aria-label={t("orchestration.field.kind")}
                    >
                      {KINDS.map((k) => (
                        <option key={k} value={k}>
                          {kindLabel(k, t)}
                        </option>
                      ))}
                    </select>
                    <input
                      className="bg-background min-w-0 flex-1 rounded border px-1.5 py-1 text-xs"
                      value={stage.title}
                      onChange={(e) => updateStage(index, { title: e.target.value })}
                      placeholder={t("orchestration.field.stageTitle")}
                      aria-label={t("orchestration.field.stageTitle")}
                    />
                    <input
                      className="bg-background w-28 rounded border px-1.5 py-1 text-xs"
                      value={stage.agent_name}
                      onChange={(e) => updateStage(index, { agent_name: e.target.value })}
                      placeholder={t("orchestration.field.agent")}
                      title={t("orchestration.field.agentHint")}
                      aria-label={t("orchestration.field.agent")}
                    />
                    <button
                      type="button"
                      className="text-muted-foreground hover:text-destructive rounded p-1"
                      title={t("orchestration.removeStage")}
                      aria-label={t("orchestration.removeStage")}
                      onClick={() => setStages((prev) => removeStage(prev, stage.id))}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                  <div className="mt-2 grid gap-2 sm:grid-cols-2">
                    <label className="flex flex-col gap-0.5 text-[10px]">
                      <span className="text-muted-foreground">
                        {t("orchestration.field.dependsOn")}
                      </span>
                      <input
                        className="bg-background rounded border px-1.5 py-1 text-xs"
                        value={stage.depends_on.join(", ")}
                        onChange={(e) =>
                          updateStage(index, {
                            depends_on: e.target.value
                              .split(",")
                              .map((x) => x.trim())
                              .filter(Boolean),
                          })
                        }
                        placeholder={t("orchestration.field.dependsOnHint")}
                      />
                    </label>
                    {stage.kind === "foreach" ? (
                      <label className="flex flex-col gap-0.5 text-[10px]">
                        <span className="text-muted-foreground">
                          {t("orchestration.field.foreachPath")}
                        </span>
                        <input
                          className="bg-background rounded border px-1.5 py-1 text-xs"
                          value={stage.foreach_path ?? ""}
                          onChange={(e) =>
                            updateStage(index, { foreach_path: e.target.value || null })
                          }
                          placeholder={t("orchestration.field.foreachPathHint")}
                        />
                      </label>
                    ) : null}
                    {stage.kind === "loop" ? (
                      <>
                        <label className="flex flex-col gap-0.5 text-[10px]">
                          <span className="text-muted-foreground">
                            {t("orchestration.field.bodyStages")}
                          </span>
                          <input
                            className="bg-background rounded border px-1.5 py-1 text-xs"
                            value={stage.body_stage_ids.join(", ")}
                            onChange={(e) =>
                              updateStage(index, {
                                body_stage_ids: e.target.value
                                  .split(",")
                                  .map((x) => x.trim())
                                  .filter(Boolean),
                              })
                            }
                          />
                        </label>
                        <label className="flex flex-col gap-0.5 text-[10px]">
                          <span className="text-muted-foreground">
                            {t("orchestration.field.maxRounds")}
                          </span>
                          <input
                            type="number"
                            min={1}
                            max={32}
                            className="bg-background rounded border px-1.5 py-1 text-xs"
                            value={stage.max_iterations ?? 3}
                            onChange={(e) =>
                              updateStage(index, {
                                max_iterations: Number(e.target.value) || 3,
                              })
                            }
                          />
                        </label>
                        <label className="flex flex-col gap-0.5 text-[10px]">
                          <span className="text-muted-foreground">
                            {t("orchestration.field.stopFlag")}
                          </span>
                          <input
                            className="bg-background rounded border px-1.5 py-1 text-xs"
                            value={stage.stop_flag_path ?? "pass"}
                            onChange={(e) =>
                              updateStage(index, {
                                stop_flag_path: e.target.value || "pass",
                              })
                            }
                          />
                        </label>
                      </>
                    ) : null}
                  </div>
                  <label className="mt-2 flex flex-col gap-0.5 text-[10px]">
                    <span className="text-muted-foreground">
                      {t("orchestration.field.prompt")}
                    </span>
                    <textarea
                      className="bg-background min-h-[52px] rounded border px-1.5 py-1 text-[11px] leading-4"
                      value={stage.prompt_template}
                      onChange={(e) =>
                        updateStage(index, { prompt_template: e.target.value })
                      }
                      placeholder={t("orchestration.field.promptHint")}
                    />
                  </label>
                </div>
              ))}
            </div>

            {!validation.ok ? (
              <p className="text-destructive text-xs">{validation.error}</p>
            ) : null}
            {(localError || saveMut.error || deleteMut.error) && (
              <ErrorAlert
                title={t("orchestration.failed")}
                message={localError || String(saveMut.error ?? deleteMut.error)}
              />
            )}
          </div>
        </div>

        <footer className="flex flex-wrap items-center justify-end gap-2 border-t px-4 py-3">
          {!isBuiltin && templateId ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={deleteMut.isPending}
              onClick={() => deleteMut.mutate()}
            >
              {t("orchestration.deleteTemplate")}
            </Button>
          ) : null}
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="gap-1.5"
            disabled={saveMut.isPending || !validation.ok}
            onClick={() => saveMut.mutate()}
          >
            {saveMut.isPending ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Save className="h-3.5 w-3.5" />
            )}
            {t("common.save")}
          </Button>
          <Button type="button" size="sm" disabled={!canStart} onClick={startWith}>
            {t("orchestration.startFromTemplate")}
          </Button>
        </footer>
      </div>
    </div>
  );
}
