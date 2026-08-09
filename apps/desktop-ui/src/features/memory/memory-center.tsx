/**
 * 智能与记忆中心 — 六 Tab 产品闭环入口（概览 / 收件箱 / 偏好 / 模式 / 项目知识 / 隐私）。
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  Brain,
  FolderKanban,
  Inbox,
  Lock,
  Shield,
  Sparkles,
  UserRound,
} from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ErrorAlert } from "@/components/ui/error-alert";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { typography } from "@/components/ui/typography";
import { useTranslation } from "@/lib/i18n-react";
import { learningKeys, ragKeys, workspaceKeys } from "@/lib/query-keys";
import type {
  LearningOverview,
  MemoryCandidate,
  MemoryItem,
  MemoryScope,
  PrivacySettings,
  WorkflowPattern,
  Workspace,
} from "@/lib/schemas";
import {
  acceptMemoryCandidate,
  acceptWorkflowPattern,
  clearLearningData,
  createMemory,
  deleteMemory,
  exportLearningData,
  getLearningOverview,
  getPrivacySettings,
  getRagIndexStatus,
  listMemories,
  listMemoryCandidates,
  listWorkflowPatterns,
  listWorkspaces,
  muteWorkflowPattern,
  refreshChangedRagDocuments,
  rebuildRagIndex,
  rejectMemoryCandidate,
  rejectWorkflowPattern,
  updatePrivacySettings,
} from "@/lib/tauri-api";
import { cn } from "@/lib/utils";
import { WorkflowPatternDetail } from "./workflow-pattern-detail";

const REJECT_REASONS = [
  "incorrect",
  "onceOnly",
  "duplicate",
  "dontRemember",
] as const;

type CenterTab =
  | "overview"
  | "inbox"
  | "preferences"
  | "patterns"
  | "knowledge"
  | "privacy";

export function MemoryCenter() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<CenterTab>("overview");
  const overviewQuery = useQuery({
    queryKey: learningKeys.overview(),
    queryFn: getLearningOverview,
  });
  const pending = overviewQuery.data?.pending_candidates ?? 0;

  const tabs: { id: CenterTab; label: string; badge?: number }[] = [
    { id: "overview", label: t("memory.center.tab.overview") },
    { id: "inbox", label: t("memory.center.tab.inbox"), badge: pending || undefined },
    { id: "preferences", label: t("memory.center.tab.preferences") },
    { id: "patterns", label: t("memory.center.tab.patterns") },
    { id: "knowledge", label: t("memory.center.tab.knowledge") },
    { id: "privacy", label: t("memory.center.tab.privacy") },
  ];

  return (
    <div className="space-y-5">
      <header>
        <h2 className={typography.pageTitle}>{t("memory.center.title")}</h2>
        <p className={typography.pageDescription}>{t("memory.center.description")}</p>
      </header>

      <div
        role="tablist"
        aria-label={t("memory.center.title")}
        className="flex flex-wrap gap-1 border-b pb-2"
      >
        {tabs.map((item) => (
          <button
            key={item.id}
            type="button"
            role="tab"
            aria-selected={tab === item.id}
            className={cn(
              "rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
              tab === item.id
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:bg-muted hover:text-foreground",
            )}
            onClick={() => setTab(item.id)}
          >
            {item.label}
            {item.badge != null && item.badge > 0 ? (
              <span className="bg-background/20 ml-1.5 rounded-full px-1.5 py-0.5 text-[10px]">
                {item.badge > 99 ? "99+" : item.badge}
              </span>
            ) : null}
          </button>
        ))}
      </div>

      <div role="tabpanel">
        {tab === "overview" && (
          <OverviewTab overview={overviewQuery.data} loading={overviewQuery.isLoading} />
        )}
        {tab === "inbox" && <InboxTab />}
        {tab === "preferences" && <PreferencesTab />}
        {tab === "patterns" && <PatternsTab />}
        {tab === "knowledge" && <KnowledgeTab />}
        {tab === "privacy" && <PrivacyTab />}
      </div>
    </div>
  );
}

function OverviewTab({
  overview,
  loading,
}: {
  overview?: LearningOverview;
  loading: boolean;
}) {
  const { t } = useTranslation();
  if (loading) {
    return <p className="text-muted-foreground text-sm">{t("common.loading")}</p>;
  }
  if (!overview) {
    return (
      <p className="text-muted-foreground text-sm">{t("memory.center.overviewEmpty")}</p>
    );
  }

  const stats = [
    { label: t("memory.center.stat.pending"), value: overview.pending_candidates },
    { label: t("memory.center.stat.confirmed"), value: overview.confirmed_preferences },
    {
      label: t("memory.center.stat.patterns"),
      value: overview.active_patterns + overview.suggested_patterns,
    },
    {
      label: t("memory.center.stat.queue"),
      value: overview.learning_queue.queued + overview.learning_queue.running,
    },
  ];

  return (
    <div className="space-y-4">
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {stats.map((s) => (
          <Card key={s.label}>
            <CardContent className="p-4">
              <p className="text-muted-foreground text-xs">{s.label}</p>
              <p className="mt-1 text-2xl font-semibold">{s.value}</p>
            </CardContent>
          </Card>
        ))}
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("memory.center.needsAttention")}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            {overview.pending_candidates === 0 ? (
              <p className="text-muted-foreground">{t("memory.center.noAttention")}</p>
            ) : (
              <>
                <p>
                  {overview.pending_candidates} {t("memory.center.pendingCandidates")}
                </p>
                {overview.recent_candidate_summaries.map((s) => (
                  <p key={s} className="text-muted-foreground truncate text-xs">
                    · {s}
                  </p>
                ))}
              </>
            )}
            {overview.suggested_patterns > 0 && (
              <p>
                {overview.suggested_patterns} {t("memory.center.suggestedPatterns")}
              </p>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("memory.center.dataStatus")}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 text-sm">
            <p>✓ {t("memory.center.localStorage")}</p>
            <p>
              {overview.sensitive_encryption_enabled
                ? `✓ ${t("memory.center.encryptionOn")}`
                : `○ ${t("memory.center.encryptionOff")}`}
            </p>
            {overview.learning_queue.failed > 0 && (
              <p className="text-warning">
                {t("memory.center.queueDelayed")}
              </p>
            )}
          </CardContent>
        </Card>
      </div>

      {overview.recent_memory_keys.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("memory.center.recentMemory")}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-1 text-sm">
            {overview.recent_memory_keys.map((k) => (
              <p key={k} className="text-muted-foreground">
                · {k}
              </p>
            ))}
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function InboxTab() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [rejectMenuId, setRejectMenuId] = useState<string | null>(null);
  const [lastRejected, setLastRejected] = useState<MemoryCandidate | null>(null);
  const [filter, setFilter] = useState<"all" | "pref" | "pattern" | "sensitive">("all");

  const candidatesQuery = useQuery({
    queryKey: learningKeys.candidates("Proposed"),
    queryFn: () => listMemoryCandidates("Proposed", null),
  });
  const patternsQuery = useQuery({
    queryKey: learningKeys.patterns("Workspace"),
    queryFn: async () => {
      const user = await listWorkflowPatterns("User", null);
      const workspaces = await listWorkspaces();
      const wsPatterns = await Promise.all(
        workspaces.map((w) => listWorkflowPatterns("Workspace", w.id)),
      );
      return [...user, ...wsPatterns.flat()].filter(
        (p) => String(p.status).toLowerCase() === "suggested",
      );
    },
  });

  const acceptCandidate = useMutation({
    mutationFn: ({
      id,
      value,
      sensitive,
    }: {
      id: MemoryCandidate["id"];
      value?: string;
      sensitive?: boolean;
    }) => acceptMemoryCandidate(id, value ?? null, null, sensitive ?? null),
    onSuccess: async () => {
      setEditingId(null);
      await queryClient.invalidateQueries({ queryKey: learningKeys.all });
      await queryClient.invalidateQueries({ queryKey: ["memories"] });
    },
  });

  const rejectCandidate = useMutation({
    mutationFn: async ({
      id,
      reason,
      snapshot,
    }: {
      id: MemoryCandidate["id"];
      reason?: string;
      snapshot?: MemoryCandidate;
    }) => {
      await rejectMemoryCandidate(id);
      return { id, reason, snapshot };
    },
    onSuccess: async (data) => {
      if (data.snapshot) setLastRejected(data.snapshot);
      setRejectMenuId(null);
      setSelected((prev) => {
        const next = new Set(prev);
        next.delete(data.id);
        return next;
      });
      await queryClient.invalidateQueries({ queryKey: learningKeys.all });
    },
  });

  const batchReject = useMutation({
    mutationFn: async (ids: string[]) => {
      for (const id of ids) {
        await rejectMemoryCandidate(id as MemoryCandidate["id"]);
      }
    },
    onSuccess: async () => {
      setSelected(new Set());
      await queryClient.invalidateQueries({ queryKey: learningKeys.all });
    },
  });

  // Soft undo after reject: re-create as long-term memory (fingerprint is blocked for Proposed).
  const undoByRecreate = useMutation({
    mutationFn: async (c: MemoryCandidate) => {
      return createMemory(
        c.scope,
        c.workspace_id ?? null,
        c.thread_id ?? null,
        c.key,
        c.value,
        c.sensitive,
      );
    },
    onSuccess: async () => {
      setLastRejected(null);
      await queryClient.invalidateQueries({ queryKey: ["memories"] });
      await queryClient.invalidateQueries({ queryKey: learningKeys.overview() });
    },
  });

  const acceptPattern = useMutation({
    mutationFn: (id: WorkflowPattern["id"]) => acceptWorkflowPattern(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: learningKeys.all });
      await queryClient.invalidateQueries({ queryKey: ["workflow-patterns"] });
    },
  });

  const rejectPattern = useMutation({
    mutationFn: (id: WorkflowPattern["id"]) => rejectWorkflowPattern(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: learningKeys.all });
      await queryClient.invalidateQueries({ queryKey: ["workflow-patterns"] });
    },
  });

  const candidates = (candidatesQuery.data ?? []).filter((c) => {
    if (filter === "sensitive") return c.sensitive;
    if (filter === "pref") return !c.sensitive;
    if (filter === "pattern") return false;
    return true;
  });
  const suggestedPatterns =
    filter === "pref" || filter === "sensitive" ? [] : (patternsQuery.data ?? []);

  if (candidatesQuery.isLoading) {
    return <p className="text-muted-foreground text-sm">{t("common.loading")}</p>;
  }

  if (
    (candidatesQuery.data?.length ?? 0) === 0 &&
    (patternsQuery.data?.length ?? 0) === 0
  ) {
    return (
      <Card>
        <CardContent className="space-y-2 p-6 text-sm">
          <p className="font-medium">{t("memory.capabilities.inboxEmpty")}</p>
          <p className="text-muted-foreground">{t("memory.center.inboxHint")}</p>
        </CardContent>
      </Card>
    );
  }

  const toggleSelect = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        {(["all", "pref", "pattern", "sensitive"] as const).map((f) => (
          <Button
            key={f}
            size="sm"
            variant={filter === f ? "default" : "outline"}
            onClick={() => setFilter(f)}
          >
            {t(`memory.center.filter.${f}`)}
          </Button>
        ))}
        {selected.size > 0 && (
          <Button
            size="sm"
            variant="outline"
            disabled={batchReject.isPending}
            onClick={() => batchReject.mutate([...selected])}
          >
            {t("memory.center.batchReject")} ({selected.size})
          </Button>
        )}
      </div>

      {lastRejected && (
        <div className="bg-muted flex flex-wrap items-center gap-2 rounded-md px-3 py-2 text-sm">
          <span>{t("memory.center.rejectedToast")}</span>
          <Button
            size="sm"
            variant="outline"
            disabled={undoByRecreate.isPending}
            onClick={() => undoByRecreate.mutate(lastRejected)}
          >
            {t("memory.center.undoReject")}
          </Button>
        </div>
      )}

      {candidates.map((c) => {
        const confLabel =
          c.confidence >= 0.85
            ? t("memory.center.confidenceHigh")
            : c.confidence >= 0.6
              ? t("memory.center.confidenceMed")
              : t("memory.center.confidenceLow");
        const isEditing = editingId === c.id;
        return (
          <Card key={c.id}>
            <CardContent className="space-y-3 p-4">
              <div className="flex flex-wrap items-center gap-2 text-xs">
                <input
                  type="checkbox"
                  checked={selected.has(c.id)}
                  onChange={() => toggleSelect(c.id)}
                  aria-label="select candidate"
                />
                <span className="font-semibold">{t("memory.center.suggestRemember")}</span>
                <span className="bg-muted rounded px-1.5 py-0.5">{confLabel}</span>
                {c.sensitive && (
                  <span className="bg-warning/15 text-warning rounded px-1.5 py-0.5">
                    {t("memory.center.sensitive")}
                  </span>
                )}
                <span className="text-muted-foreground">{c.scope}</span>
                <span className="text-muted-foreground">{c.kind}</span>
              </div>
              {isEditing ? (
                <Textarea
                  value={editValue}
                  onChange={(e) => setEditValue(e.target.value)}
                  rows={3}
                />
              ) : (
                <p className="text-sm whitespace-pre-wrap">{c.value}</p>
              )}
              {c.evidence[0] && (
                <p className="text-muted-foreground text-xs">
                  {t("memory.capabilities.evidence")}: {c.evidence[0]}
                </p>
              )}
              <div className="flex flex-wrap gap-2">
                {isEditing ? (
                  <>
                    <Button
                      size="sm"
                      disabled={acceptCandidate.isPending}
                      onClick={() =>
                        acceptCandidate.mutate({ id: c.id, value: editValue.trim() })
                      }
                    >
                      {t("memory.center.confirmSave")}
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => setEditingId(null)}>
                      {t("common.cancel")}
                    </Button>
                  </>
                ) : (
                  <>
                    <Button
                      size="sm"
                      disabled={acceptCandidate.isPending}
                      onClick={() => acceptCandidate.mutate({ id: c.id })}
                    >
                      {t("memory.capabilities.acceptCandidate")}
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => {
                        setEditingId(c.id);
                        setEditValue(c.value);
                      }}
                    >
                      {t("memory.center.editThenAccept")}
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={rejectCandidate.isPending}
                      onClick={() => setRejectMenuId(rejectMenuId === c.id ? null : c.id)}
                    >
                      {t("memory.capabilities.rejectCandidate")}
                    </Button>
                  </>
                )}
              </div>
              {rejectMenuId === c.id && (
                <div className="bg-muted/40 flex flex-wrap gap-1.5 rounded-md border p-2">
                  {REJECT_REASONS.map((reason) => (
                    <Button
                      key={reason}
                      size="sm"
                      variant="outline"
                      className="h-7 text-[11px]"
                      onClick={() =>
                        rejectCandidate.mutate({
                          id: c.id,
                          reason,
                          snapshot: c,
                        })
                      }
                    >
                      {t(`memory.center.rejectReason.${reason}`)}
                    </Button>
                  ))}
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-7 text-[11px]"
                    onClick={() =>
                      rejectCandidate.mutate({ id: c.id, snapshot: c })
                    }
                  >
                    {t("memory.center.rejectNoReason")}
                  </Button>
                </div>
              )}
            </CardContent>
          </Card>
        );
      })}

      {suggestedPatterns.map((p) => (
        <Card key={p.id}>
          <CardContent className="space-y-2 p-4">
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-medium">{p.name}</span>
              <span className="bg-muted rounded px-1.5 py-0.5 text-[10px] uppercase">
                Suggested
              </span>
            </div>
            <p className="text-muted-foreground text-sm">{p.summary}</p>
            {p.preferred_roles.length > 0 && (
              <p className="text-xs">{p.preferred_roles.join(" → ")}</p>
            )}
            <div className="flex flex-wrap gap-2">
              <Button
                size="sm"
                disabled={acceptPattern.isPending}
                onClick={() => acceptPattern.mutate(p.id)}
              >
                {t("memory.center.acceptEnable")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={rejectPattern.isPending}
                onClick={() => rejectPattern.mutate(p.id)}
              >
                {t("memory.capabilities.rejectPattern")}
              </Button>
            </div>
          </CardContent>
        </Card>
      ))}

      {(acceptCandidate.error || rejectCandidate.error || batchReject.error) && (
        <ErrorAlert
          title={t("memory.capabilities.inboxActionFailed")}
          message={String(
            acceptCandidate.error ?? rejectCandidate.error ?? batchReject.error,
          )}
        />
      )}
    </div>
  );
}

function PreferencesTab() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [nlText, setNlText] = useState("");
  const [scope, setScope] = useState<MemoryScope>("User");
  const [sensitive, setSensitive] = useState(false);

  const userFactsQuery = useQuery({
    queryKey: ["memories", "User", null],
    queryFn: () => listMemories("User", null, null),
  });

  const create = useMutation({
    mutationFn: () => {
      const value = nlText.trim();
      const key =
        value.length > 40
          ? value.slice(0, 40).replace(/\s+/g, "_")
          : value.replace(/\s+/g, "_") || "preference";
      return createMemory(scope, null, null, key, value, sensitive);
    },
    onSuccess: async () => {
      setNlText("");
      setSensitive(false);
      await queryClient.invalidateQueries({ queryKey: ["memories"] });
      await queryClient.invalidateQueries({ queryKey: learningKeys.overview() });
    },
  });

  const remove = useMutation({
    mutationFn: (id: MemoryItem["id"]) => deleteMemory(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["memories"] });
      await queryClient.invalidateQueries({ queryKey: learningKeys.overview() });
    },
  });

  const items = userFactsQuery.data ?? [];

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("memory.center.rememberWhat")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <Textarea
            placeholder={t("memory.center.rememberPlaceholder")}
            value={nlText}
            onChange={(e) => setNlText(e.target.value)}
            rows={3}
          />
          <div className="flex flex-wrap items-center gap-3 text-sm">
            <label className="flex items-center gap-1.5">
              <input
                type="radio"
                name="pref-scope"
                checked={scope === "User"}
                onChange={() => setScope("User")}
              />
              {t("memory.center.scopeAllProjects")}
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={sensitive}
                onChange={(e) => setSensitive(e.target.checked)}
              />
              {t("memory.sensitive")}
            </label>
          </div>
          <Button
            disabled={!nlText.trim() || create.isPending}
            onClick={() => create.mutate()}
          >
            {t("memory.saveMemory")}
          </Button>
          {create.error && (
            <ErrorAlert
              title={t("memory.capabilities.saveFailed")}
              message={String(create.error)}
            />
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("memory.capabilities.userFactsTitle")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2">
          {items.length === 0 ? (
            <p className="text-muted-foreground text-sm">
              {t("memory.capabilities.userFactsEmpty")}
            </p>
          ) : (
            items.map((item) => (
              <div
                key={item.id}
                className="flex items-start justify-between gap-3 rounded-md border p-3"
              >
                <div className="min-w-0">
                  <p className="text-sm whitespace-pre-wrap">{item.value}</p>
                  <p className="text-muted-foreground mt-1 text-[11px]">
                    {item.scope}
                    {item.use_count ? ` · used ${item.use_count}` : ""}
                    {item.sensitive ? ` · ${t("memory.sensitive")}` : ""}
                  </p>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={remove.isPending}
                  onClick={() => remove.mutate(item.id)}
                >
                  {t("memory.delete")}
                </Button>
              </div>
            ))
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function PatternsTab() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [detail, setDetail] = useState<WorkflowPattern | null>(null);
  const patternsQuery = useQuery({
    queryKey: learningKeys.patterns("all"),
    queryFn: async () => {
      const user = await listWorkflowPatterns("User", null);
      const workspaces = await listWorkspaces();
      const rest = await Promise.all(
        workspaces.map((w) => listWorkflowPatterns("Workspace", w.id)),
      );
      return [...user, ...rest.flat()];
    },
  });

  const mute = useMutation({
    mutationFn: (id: WorkflowPattern["id"]) => muteWorkflowPattern(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: learningKeys.patterns("all") });
    },
  });
  const accept = useMutation({
    mutationFn: (id: WorkflowPattern["id"]) => acceptWorkflowPattern(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: learningKeys.patterns("all") });
    },
  });
  const reject = useMutation({
    mutationFn: (id: WorkflowPattern["id"]) => rejectWorkflowPattern(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: learningKeys.patterns("all") });
    },
  });

  const items = patternsQuery.data ?? [];

  return (
    <div className="space-y-3">
      <p className="text-muted-foreground text-sm">{t("memory.capabilities.userPatternsBody")}</p>
      {items.length === 0 ? (
        <p className="text-muted-foreground text-sm">{t("memory.capabilities.patternsEmpty")}</p>
      ) : (
        items.map((p) => {
          const status = String(p.status).toLowerCase();
          return (
            <Card key={p.id}>
              <CardContent className="space-y-2 p-4">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{p.name}</span>
                  <span className="bg-muted rounded px-1.5 py-0.5 text-[10px] uppercase">
                    {p.status}
                  </span>
                  <span className="text-muted-foreground text-xs">
                    ✓{p.success_count} · ✗{p.failure_count}
                    {p.evidence_count != null ? ` · evidence ${p.evidence_count}` : ""}
                  </span>
                </div>
                <p className="text-muted-foreground text-sm">{p.summary}</p>
                {p.preferred_roles.length > 0 && (
                  <p className="text-xs">{p.preferred_roles.join(" → ")}</p>
                )}
                <div className="flex flex-wrap gap-2">
                  <Button size="sm" variant="outline" onClick={() => setDetail(p)}>
                    {t("memory.center.viewDetail")}
                  </Button>
                  {status === "suggested" && (
                    <>
                      <Button size="sm" onClick={() => accept.mutate(p.id)}>
                        {t("memory.center.acceptEnable")}
                      </Button>
                      <Button size="sm" variant="outline" onClick={() => reject.mutate(p.id)}>
                        {t("memory.capabilities.rejectPattern")}
                      </Button>
                    </>
                  )}
                  {status !== "muted" && status !== "rejected" && (
                    <Button size="sm" variant="outline" onClick={() => mute.mutate(p.id)}>
                      {t("memory.capabilities.mutePattern")}
                    </Button>
                  )}
                </div>
              </CardContent>
            </Card>
          );
        })
      )}
      <WorkflowPatternDetail
        pattern={detail}
        open={!!detail}
        onClose={() => setDetail(null)}
      />
    </div>
  );
}

function KnowledgeTab() {
  const { t } = useTranslation();
  const workspacesQuery = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });
  const workspaces = workspacesQuery.data ?? [];

  return (
    <div className="space-y-3">
      <p className="text-muted-foreground text-sm">{t("memory.center.knowledgeBody")}</p>
      {workspaces.length === 0 ? (
        <p className="text-muted-foreground text-sm">{t("memory.capabilities.noProjects")}</p>
      ) : (
        workspaces.map((ws) => <ProjectKnowledgeCard key={ws.id} workspace={ws} />)
      )}
    </div>
  );
}

function ProjectKnowledgeCard({ workspace }: { workspace: Workspace }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const statusQuery = useQuery({
    queryKey: ragKeys.workspaceStatus(workspace.id),
    queryFn: () => getRagIndexStatus(workspace.id),
  });
  const refresh = useMutation({
    mutationFn: () => refreshChangedRagDocuments(workspace.id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ragKeys.workspaceStatus(workspace.id) });
    },
  });
  const rebuild = useMutation({
    mutationFn: () => rebuildRagIndex(workspace.id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ragKeys.workspaceStatus(workspace.id) });
    },
  });

  const s = statusQuery.data;

  return (
    <Card>
      <CardContent className="space-y-2 p-4">
        <div className="flex items-start justify-between gap-3">
          <div>
            <p className="font-medium">{workspace.name}</p>
            {s ? (
              <p className="text-muted-foreground text-xs">
                indexed {s.indexed} · stale {s.stale} · failed {s.failed} · total{" "}
                {s.total_documents}
              </p>
            ) : (
              <p className="text-muted-foreground text-xs">{t("common.loading")}</p>
            )}
            {s && (
              <p className="text-muted-foreground text-[11px]">
                {s.embedding_provider_id} · dim {s.dimension}
              </p>
            )}
          </div>
          <Link
            to="/workspaces/$workspaceId/memory"
            params={{ workspaceId: workspace.id }}
            className="text-primary text-xs hover:underline"
          >
            {t("memory.capabilities.openProjectMemory")}
          </Link>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="outline"
            disabled={refresh.isPending}
            onClick={() => refresh.mutate()}
          >
            {t("inspector.refreshRag")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={rebuild.isPending}
            onClick={() => {
              if (window.confirm(t("memory.center.rebuildConfirm"))) {
                rebuild.mutate();
              }
            }}
          >
            {t("memory.rebuildIndex")}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function PrivacyTab() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const settingsQuery = useQuery({
    queryKey: learningKeys.privacy(),
    queryFn: getPrivacySettings,
  });
  const [draft, setDraft] = useState<PrivacySettings | null>(null);
  const settings = draft ?? settingsQuery.data;

  const save = useMutation({
    mutationFn: (s: PrivacySettings) => updatePrivacySettings(s),
    onSuccess: async (s) => {
      setDraft(s);
      await queryClient.invalidateQueries({ queryKey: learningKeys.privacy() });
    },
  });

  const clear = useMutation({
    mutationFn: (opts: {
      clearCandidates?: boolean;
      clearMemories?: boolean;
      clearPatterns?: boolean;
      clearRag?: boolean;
    }) => clearLearningData(opts),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: learningKeys.all });
      await queryClient.invalidateQueries({ queryKey: ["memories"] });
      await queryClient.invalidateQueries({ queryKey: ["workflow-patterns"] });
    },
  });

  const exportData = useMutation({
    mutationFn: exportLearningData,
    onSuccess: (data) => {
      const blob = new Blob([JSON.stringify(data, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `portico-learning-export-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
    },
  });

  if (!settings) {
    return <p className="text-muted-foreground text-sm">{t("common.loading")}</p>;
  }

  const modes: PrivacySettings["privacy_mode"][] = [
    "FullyLocal",
    "LocalStorageCloudInference",
    "CloudInferenceAndEmbedding",
  ];

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Shield className="h-4 w-4" />
            {t("memory.center.privacyMode")}
          </CardTitle>
        </CardHeader>
        <CardContent className="grid gap-2 sm:grid-cols-3">
          {modes.map((mode) => (
            <button
              key={mode}
              type="button"
              className={cn(
                "rounded-lg border p-3 text-left text-sm transition-colors",
                settings.privacy_mode === mode
                  ? "border-primary bg-primary/5"
                  : "hover:bg-muted/50",
              )}
              onClick={() => setDraft({ ...settings, privacy_mode: mode })}
            >
              <p className="font-medium">{t(`memory.center.mode.${mode}`)}</p>
              <p className="text-muted-foreground mt-1 text-xs">
                {t(`memory.center.modeBody.${mode}`)}
              </p>
            </button>
          ))}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("memory.center.learningSettings")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <label className="flex items-center justify-between gap-3">
            <span>{t("memory.center.autoDiscover")}</span>
            <input
              type="checkbox"
              checked={settings.auto_discover_candidates}
              onChange={(e) =>
                setDraft({ ...settings, auto_discover_candidates: e.target.checked })
              }
            />
          </label>
          <p className="text-muted-foreground text-xs">{t("memory.center.autoDiscoverHint")}</p>
          <label className="flex items-center justify-between gap-3">
            <span>{t("memory.center.autoPromote")}</span>
            <input
              type="checkbox"
              checked={settings.auto_promote_patterns}
              onChange={(e) =>
                setDraft({ ...settings, auto_promote_patterns: e.target.checked })
              }
            />
          </label>
          <label className="flex items-center justify-between gap-3">
            <span>{t("memory.center.promoteThreshold")}</span>
            <Input
              type="number"
              className="w-20"
              min={1}
              max={20}
              value={settings.auto_promote_threshold}
              onChange={(e) =>
                setDraft({
                  ...settings,
                  auto_promote_threshold: Math.max(1, Number(e.target.value) || 3),
                })
              }
            />
          </label>
          <div className="space-y-1">
            <p className="font-medium">{t("memory.center.traceRetention")}</p>
            {(["RedactedTrace", "FullLocalTrace", "MetadataOnly"] as const).map((mode) => (
              <label key={mode} className="flex items-center gap-2">
                <input
                  type="radio"
                  name="trace"
                  checked={settings.trace_retention === mode}
                  onChange={() => setDraft({ ...settings, trace_retention: mode })}
                />
                {t(`memory.center.trace.${mode}`)}
              </label>
            ))}
          </div>
          <Button
            disabled={save.isPending}
            onClick={() => save.mutate(settings)}
          >
            {t("common.save")}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Lock className="h-4 w-4" />
            {t("memory.center.dataManagement")}
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="outline"
            disabled={exportData.isPending}
            onClick={() => exportData.mutate()}
          >
            {t("memory.center.exportAll")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              if (window.confirm(t("memory.center.clearCandidatesConfirm"))) {
                clear.mutate({ clearCandidates: true });
              }
            }}
          >
            {t("memory.center.clearCandidates")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              if (window.confirm(t("memory.center.clearMemoriesConfirm"))) {
                clear.mutate({ clearMemories: true });
              }
            }}
          >
            {t("memory.center.clearMemories")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              if (window.confirm(t("memory.center.clearPatternsConfirm"))) {
                clear.mutate({ clearPatterns: true });
              }
            }}
          >
            {t("memory.center.clearPatterns")}
          </Button>
          <Button
            size="sm"
            variant="destructive"
            onClick={() => {
              if (window.confirm(t("memory.center.resetAllConfirm"))) {
                clear.mutate({
                  clearCandidates: true,
                  clearMemories: true,
                  clearPatterns: true,
                  clearRag: true,
                });
              }
            }}
          >
            {t("memory.center.resetAll")}
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}

/** Pending candidate count for sidebar badge. */
export function usePendingCandidateCount(): number {
  const q = useQuery({
    queryKey: learningKeys.overview(),
    queryFn: getLearningOverview,
    staleTime: 30_000,
  });
  return q.data?.pending_candidates ?? 0;
}

// Keep icons referenced so tree-shaking doesn't confuse audits.
void [Brain, FolderKanban, Inbox, Sparkles, UserRound];
