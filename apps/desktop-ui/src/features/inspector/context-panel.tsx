import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  getRagIndexStatus,
  getRunContextSnapshot,
  inspectContext,
  refreshChangedRagDocuments,
} from "@/lib/tauri-api";
import type { AgentRunId, ContextSnapshotItem, ThreadId, WorkspaceId } from "@/lib/schemas";
import type { ReactNode } from "react";
import { useTranslation } from "@/lib/i18n-react";
import { EmptyState, InlineError, PanelLoading } from "./panel-primitives";
import { learningKeys, runKeys } from "@/lib/query-keys";
import { cn } from "@/lib/utils";

interface ContextPanelProps {
  workspaceId: WorkspaceId;
  threadId: ThreadId;
  runId?: AgentRunId;
}

type InspectorTab = "snapshot" | "sent" | "blocked" | "memory" | "rag" | "policy" | "budget" | "preview";

export function ContextPanel({ workspaceId, threadId, runId }: ContextPanelProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<InspectorTab>("snapshot");
  const [previewQuery, setPreviewQuery] = useState("");
  const [revealSensitive, setRevealSensitive] = useState(false);

  const snapshotQuery = useQuery({
    queryKey: learningKeys.runSummary(runId!),
    queryFn: () => getRunContextSnapshot(runId!),
    enabled: !!runId,
  });

  const previewQueryResult = useQuery({
    queryKey: runKeys.context(workspaceId, threadId, runId, previewQuery),
    queryFn: () => inspectContext(runId!, threadId, workspaceId, previewQuery),
    enabled: !!runId && tab === "preview",
  });

  const ragStatusQuery = useQuery({
    queryKey: ["rag-index-status", workspaceId],
    queryFn: () => getRagIndexStatus(workspaceId),
  });

  const [refreshing, setRefreshing] = useState(false);

  if (!runId) return <EmptyState message={t("inspector.startRunContext")} />;

  if (snapshotQuery.isLoading) return <PanelLoading />;
  if (snapshotQuery.error && tab !== "preview") {
    // Fall through to preview-only mode when no snapshot yet.
  }

  const snap = snapshotQuery.data;
  const items = snap?.items ?? [];
  const sent = items.filter((i) => i.disposition === "Sent");
  const blocked = items.filter(
    (i) =>
      i.disposition === "BlockedSensitive" ||
      i.disposition === "TrimmedByBudget" ||
      i.disposition === "NotRelevant" ||
      i.disposition === "DisabledForRun" ||
      i.disposition === "LocalOnly",
  );
  const memoryItems = items.filter((i) => i.kind === "memory");
  const ragItems = items.filter((i) => i.kind === "rag");
  const learning = snap?.learning;
  const policy = snap?.behavior_policy ?? learning?.behavior_policy;
  const outbound = snap?.outbound_manifest ?? learning?.outbound_manifest;

  const tabs: { id: InspectorTab; label: string }[] = [
    { id: "snapshot", label: t("inspector.tab.snapshot") },
    { id: "sent", label: t("inspector.tab.sent") },
    { id: "blocked", label: t("inspector.tab.blocked") },
    { id: "memory", label: t("inspector.tab.memory") },
    { id: "rag", label: t("inspector.tab.rag") },
    { id: "policy", label: t("inspector.tab.policy") },
    { id: "budget", label: t("inspector.tab.budget") },
    { id: "preview", label: t("inspector.tab.preview") },
  ];

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-3">
      {/* Top summary from frozen snapshot */}
      <div className="grid grid-cols-2 gap-2">
        <MetaCard
          title={t("inspector.outboundManifest")}
          value={
            outbound
              ? `${outbound.provider_kind}${outbound.local_provider ? " · local" : " · remote"}`
              : t("inspector.noSnapshotYet")
          }
        />
        <MetaCard
          title={t("inspector.privacy")}
          value={
            outbound?.sensitive_content_blocked
              ? t("inspector.sensitiveBlocked")
              : t("inspector.sensitiveOk")
          }
        />
      </div>

      {learning?.feedback && (
        <p className="text-muted-foreground text-xs">
          {t("inspector.userFeedback")}: {learning.feedback.rating}
        </p>
      )}
      {learning?.experience && (
        <p className="text-muted-foreground text-xs">
          {t("inspector.outcome")}: {learning.experience.outcome}
          {learning.candidates.length > 0
            ? ` · ${learning.candidates.length} ${t("inspector.candidatesFromRun")}`
            : ""}
        </p>
      )}

      <TabsList className="flex flex-wrap gap-1">
        {tabs.map((item) => (
          <TabsTrigger
            key={item.id}
            variant="compact"
            active={tab === item.id}
            onClick={() => setTab(item.id)}
          >
            {item.label}
          </TabsTrigger>
        ))}
      </TabsList>

      {tab === "snapshot" && (
        <TabsContent className="space-y-2">
          {!snap ? (
            <p className="text-muted-foreground text-xs">{t("inspector.noSnapshotYet")}</p>
          ) : (
            <>
              <MetaCard
                title={t("inspector.snapshotStats")}
                value={`mem ${snap.memory_ids.length} · pattern ${snap.pattern_ids.length} · items ${items.length}`}
              />
              {policy && (
                <Section title={t("inspector.behaviorPolicy")}>
                  <PolicyView policy={policy} />
                </Section>
              )}
              {outbound && (
                <Section title={t("inspector.outboundManifest")}>
                  <div className="rounded border p-2 text-xs space-y-1">
                    <p>bytes ~{outbound.total_bytes}</p>
                    <p>rag paths: {outbound.rag_paths.length}</p>
                  </div>
                </Section>
              )}
            </>
          )}
        </TabsContent>
      )}

      {tab === "sent" && (
        <TabsContent>
          <ItemList
            items={sent}
            empty={t("inspector.noSent")}
            revealSensitive={revealSensitive}
            onReveal={() => {
              if (window.confirm(t("inspector.revealSensitiveConfirm"))) {
                setRevealSensitive(true);
              }
            }}
          />
        </TabsContent>
      )}

      {tab === "blocked" && (
        <TabsContent>
          <ItemList
            items={blocked}
            empty={t("inspector.noBlocked")}
            revealSensitive={revealSensitive}
            onReveal={() => {
              if (window.confirm(t("inspector.revealSensitiveConfirm"))) {
                setRevealSensitive(true);
              }
            }}
          />
        </TabsContent>
      )}

      {tab === "memory" && (
        <TabsContent>
          <ItemList
            items={memoryItems}
            empty={t("inspector.noMemory")}
            revealSensitive={revealSensitive}
            onReveal={() => {
              if (window.confirm(t("inspector.revealSensitiveConfirm"))) {
                setRevealSensitive(true);
              }
            }}
          />
        </TabsContent>
      )}

      {tab === "rag" && (
        <TabsContent className="space-y-2">
          <ItemList items={ragItems} empty={t("inspector.noRag")} revealSensitive={false} />
          {ragStatusQuery.data && (
            <div className="rounded border p-2 text-xs space-y-1">
              <p>
                indexed {ragStatusQuery.data.indexed} · stale {ragStatusQuery.data.stale} · failed{" "}
                {ragStatusQuery.data.failed}
              </p>
              <Button
                size="sm"
                variant="outline"
                disabled={refreshing}
                onClick={async () => {
                  setRefreshing(true);
                  try {
                    await refreshChangedRagDocuments(workspaceId);
                    await ragStatusQuery.refetch();
                  } finally {
                    setRefreshing(false);
                  }
                }}
              >
                {refreshing ? t("common.loading") : t("inspector.refreshRag")}
              </Button>
            </div>
          )}
        </TabsContent>
      )}

      {tab === "policy" && (
        <TabsContent>
          {policy ? (
            <PolicyView policy={policy} />
          ) : (
            <p className="text-muted-foreground text-xs">{t("inspector.noPolicy")}</p>
          )}
        </TabsContent>
      )}

      {tab === "budget" && (
        <TabsContent className="space-y-2">
          <p className="text-muted-foreground text-xs">{t("inspector.budgetFromSnapshot")}</p>
          {outbound && (
            <MetaCard title="outbound bytes" value={String(outbound.total_bytes)} />
          )}
          <MetaCard
            title="memories recalled"
            value={String(snap?.memory_ids.length ?? 0)}
          />
          <MetaCard
            title="patterns used"
            value={String(snap?.pattern_ids.length ?? 0)}
          />
        </TabsContent>
      )}

      {tab === "preview" && (
        <TabsContent className="space-y-2">
          <p className="text-warning text-xs font-medium">{t("inspector.previewWarning")}</p>
          <Input
            placeholder={t("inspector.contextQuery")}
            value={previewQuery}
            onChange={(e) => setPreviewQuery(e.target.value)}
            className="h-8 text-xs"
          />
          {previewQueryResult.isLoading && <PanelLoading />}
          {previewQueryResult.error && (
            <InlineError
              title={t("inspector.inspectContextFailed")}
              message={previewQueryResult.error.message}
            />
          )}
          {previewQueryResult.data && (
            <>
              <MetaCard
                title={t("inspector.estimatedTokens")}
                value={String(previewQueryResult.data.estimated_tokens)}
              />
              <Section title={t("inspector.memory")}>
                {previewQueryResult.data.memories.length === 0 ? (
                  <p className="text-muted-foreground text-xs">{t("inspector.noMemory")}</p>
                ) : (
                  previewQueryResult.data.memories.map((m) => (
                    <div key={m.id} className="rounded border p-2 text-xs">
                      <p className="font-medium">{m.key}</p>
                      <p>{m.sensitive ? t("inspector.sensitiveHidden") : m.value}</p>
                    </div>
                  ))
                )}
              </Section>
            </>
          )}
        </TabsContent>
      )}
    </div>
  );
}

function PolicyView({
  policy,
}: {
  policy: {
    response_language?: string | null;
    response_style?: string | null;
    explore_before_edit?: boolean;
    run_tests_after_edit?: boolean;
    negative_constraints?: string[];
    memory_ids?: string[];
    pattern_ids?: string[];
  };
}) {
  return (
    <div className="rounded border p-2 text-xs space-y-1">
      {policy.response_language && <p>language: {policy.response_language}</p>}
      {policy.response_style && <p>style: {policy.response_style}</p>}
      {policy.explore_before_edit && <p>explore-before-edit: yes</p>}
      {policy.run_tests_after_edit && <p>run-tests-after-edit: yes</p>}
      {(policy.negative_constraints?.length ?? 0) > 0 && (
        <p>constraints: {policy.negative_constraints?.join("; ")}</p>
      )}
      <p className="text-muted-foreground">
        memories: {policy.memory_ids?.length ?? 0} · patterns: {policy.pattern_ids?.length ?? 0}
      </p>
    </div>
  );
}

function ItemList({
  items,
  empty,
  revealSensitive,
  onReveal,
}: {
  items: ContextSnapshotItem[];
  empty: string;
  revealSensitive: boolean;
  onReveal?: () => void;
}) {
  const { t } = useTranslation();
  if (items.length === 0) {
    return <p className="text-muted-foreground text-xs">{empty}</p>;
  }
  return (
    <div className="space-y-2">
      {items.map((item, idx) => {
        const sensitive = item.disposition === "BlockedSensitive";
        const body =
          sensitive && !revealSensitive ? t("inspector.sensitiveHidden") : item.summary;
        return (
          <div key={`${item.kind}-${item.title}-${idx}`} className="rounded border p-2 text-xs">
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="font-medium">{item.title}</span>
              <DispositionBadge disposition={item.disposition} />
              {item.score != null && (
                <span className="text-muted-foreground">score {item.score.toFixed(2)}</span>
              )}
            </div>
            <p className="mt-1 whitespace-pre-wrap">{body}</p>
            {item.reason && (
              <p className="text-muted-foreground mt-0.5">{item.reason}</p>
            )}
            {sensitive && !revealSensitive && onReveal && (
              <button
                type="button"
                className="text-primary mt-1 text-[11px] underline-offset-2 hover:underline"
                onClick={onReveal}
              >
                {t("inspector.revealSensitive")}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}

function DispositionBadge({ disposition }: { disposition: string }) {
  const tone =
    disposition === "Sent"
      ? "bg-emerald-500/15 text-emerald-700"
      : disposition === "BlockedSensitive"
        ? "bg-destructive/15 text-destructive"
        : "bg-muted text-muted-foreground";
  return (
    <span className={cn("rounded px-1.5 py-0.5 text-[10px]", tone)}>{disposition}</span>
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
      <p className="text-sm font-medium break-all">{value}</p>
    </div>
  );
}
