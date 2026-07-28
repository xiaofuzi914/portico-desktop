import { useEffect, useState } from "react";
import { Link } from "@tanstack/react-router";
import {
  CheckCircle2,
  ExternalLink,
  Loader2,
  MessageSquareText,
  Play,
  Save,
  Trash2,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Modal } from "@/components/ui/modal";
import { Textarea } from "@/components/ui/textarea";
import type { CanvasLink, CanvasNode, CanvasNodeStatus, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import {
  USER_EDITABLE_ROLES,
  applyRoleToNode,
  roleFromNode,
  roleLabelKey,
  type CanvasNodeRole,
} from "./canvas-node-role";
import { kindStyle } from "./canvas-view-model";
import { cn } from "@/lib/utils";

export type StagePayload = {
  acceptance?: string;
  suggested_prompt?: string;
  launch_mode?: "single" | "multi-role" | string;
  last_run_id?: string | null;
  last_thread_id?: string | null;
  related_insight_ids?: string[];
  message_ids?: string[];
  thread_id?: string;
};

export function parseStagePayload(payloadJson: string): StagePayload {
  try {
    return JSON.parse(payloadJson) as StagePayload;
  } catch {
    return {};
  }
}

/** Build a chat prompt from a canvas node for send / launch. */
export function buildNodeChatPrompt(node: CanvasNode): string {
  const payload = parseStagePayload(node.payload_json);
  if (node.kind === "Stage") {
    return (
      payload.suggested_prompt?.trim() ||
      `【任务】${node.title}\n【说明】${node.summary}\n【验收】${payload.acceptance ?? ""}\n【要求】中文结论先行；标注依据路径。`
    );
  }
  if (node.kind === "Goal") {
    return `【目标】${node.title}\n【说明】${node.summary || "（无补充）"}\n请围绕该目标推进，给出可执行下一步与验收标准。`;
  }
  if (node.kind === "Insight" || node.kind === "ThreadCluster") {
    return `【基于脑图节点】${node.title}\n${node.summary || ""}\n请基于以上内容继续分析或给出下一步建议。`;
  }
  const body = [node.title.trim(), node.summary.trim()].filter(Boolean).join("\n\n");
  return body || node.title || "（空便签）";
}

const EDITABLE_STATUSES: CanvasNodeStatus[] = [
  "Todo",
  "InProgress",
  "Done",
  "Blocked",
  "Stale",
];

interface CanvasNodeInspectorProps {
  open: boolean;
  onClose: () => void;
  workspaceId: WorkspaceId;
  /** When set (session mind map), actions prefer the current thread. */
  threadId?: string;
  node: CanvasNode | null;
  links: CanvasLink[];
  onDelete: (nodeId: string) => void;
  onSave: (node: CanvasNode) => void;
  onLaunchStage?: (
    node: CanvasNode,
    mode: "single" | "multi-role",
    target: "new-thread" | "current-thread",
  ) => void;
  /** Send node content into chat (current thread or a new one). */
  onSendToChat?: (node: CanvasNode, target: "new-thread" | "current-thread") => void;
  onMarkDone?: (node: CanvasNode) => void;
  onDecomposeGoal?: (node: CanvasNode) => void;
  deleting?: boolean;
  launching?: boolean;
  saving?: boolean;
  statusPending?: boolean;
}

/**
 * Node detail + edit dialog. Opened by double-clicking a canvas node so the
 * mind-map keeps full width (no permanent side panel).
 */
export function CanvasNodeInspector({
  open,
  onClose,
  workspaceId,
  threadId,
  node,
  links,
  onDelete,
  onSave,
  onLaunchStage,
  onSendToChat,
  onMarkDone,
  onDecomposeGoal,
  deleting = false,
  launching = false,
  saving = false,
  statusPending = false,
}: CanvasNodeInspectorProps) {
  const { t } = useTranslation();
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [status, setStatus] = useState<CanvasNodeStatus>("Todo");
  const [role, setRole] = useState<CanvasNodeRole>("note");

  useEffect(() => {
    if (!node) {
      setTitle("");
      setSummary("");
      setStatus("Todo");
      setRole("note");
      return;
    }
    setTitle(node.title);
    setSummary(node.summary);
    setStatus(node.status);
    setRole(roleFromNode(node));
  }, [node?.id, node?.title, node?.summary, node?.status, node?.updated_at, node?.kind, node?.payload_json]);

  if (!node) {
    return null;
  }

  const applied = applyRoleToNode(node, role);
  const style = kindStyle(applied.kind);
  const threadLinks = links.filter((l) => l.ref_type === "Thread");
  const messageLinks = links.filter((l) => l.ref_type === "Message");
  const payload = parseStagePayload(node.payload_json);
  const isStage = role === "stage";
  const isGoal = role === "goal";
  const isNote = role === "note";
  const isInsight =
    role === "insight" ||
    role === "intent" ||
    role === "progress" ||
    role === "conclusion" ||
    role === "session";
  const defaultMode: "single" | "multi-role" =
    payload.launch_mode === "multi-role" ? "multi-role" : "single";
  const openThreadId =
    threadLinks[0]?.ref_id ??
    payload.last_thread_id ??
    payload.thread_id ??
    null;
  const openMessageId =
    messageLinks[0]?.ref_id ??
    (Array.isArray(payload.message_ids) ? payload.message_ids[0] : null) ??
    null;

  const originalRole = roleFromNode(node);
  const dirty =
    title.trim() !== node.title ||
    summary !== node.summary ||
    status !== node.status ||
    role !== originalRole;

  // All user-facing nodes are editable; session roots from extract can still retitle.
  const canEdit = true;
  const roleOptions: CanvasNodeRole[] =
    originalRole === "session"
      ? (["session", ...USER_EDITABLE_ROLES] as CanvasNodeRole[])
      : USER_EDITABLE_ROLES;

  const draftNode = (): CanvasNode => {
    const roleFields = applyRoleToNode(node, role);
    return {
      ...node,
      ...roleFields,
      title: title.trim() || node.title,
      summary,
      status,
      source: node.source === "Auto" ? "User" : node.source,
      updated_at: new Date().toISOString(),
    };
  };

  const handleSave = () => {
    if (!dirty) return;
    onSave(draftNode());
  };

  const withDraft = (action: (n: CanvasNode) => void) => {
    action(draftNode());
  };

  const handleDelete = () => {
    onDelete(node.id);
    onClose();
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      labelledBy="canvas-node-dialog-title"
      className="flex max-h-[min(88vh,720px)] max-w-lg flex-col overflow-hidden p-0"
    >
      <div className="flex shrink-0 items-start justify-between gap-3 border-b px-5 py-4">
        <div className="min-w-0">
          <h2 id="canvas-node-dialog-title" className="text-sm font-semibold">
            {t("canvas.inspector")}
          </h2>
          <p className="text-muted-foreground mt-0.5 text-xs">{t("canvas.inspectorHint")}</p>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 w-8 shrink-0 p-0"
          onClick={onClose}
          aria-label={t("common.close")}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      <div className="min-h-0 flex-1 space-y-4 overflow-auto px-5 py-4">
        {canEdit ? (
          <div className="space-y-3">
            <div>
              <label className="text-muted-foreground mb-1 block text-[10px] font-medium tracking-wide uppercase">
                {t("canvas.fieldRole")}
              </label>
              <select
                className="border-border bg-background w-full rounded-md border px-2 py-1.5 text-sm"
                value={role}
                onChange={(e) => setRole(e.target.value as CanvasNodeRole)}
              >
                {roleOptions.map((r) => (
                  <option key={r} value={r}>
                    {t(roleLabelKey(r))}
                  </option>
                ))}
              </select>
              <p className="text-muted-foreground mt-1 text-[11px] leading-snug">
                {t("canvas.fieldRoleHint")}
              </p>
            </div>
            <div>
              <label className="text-muted-foreground mb-1 block text-[10px] font-medium tracking-wide uppercase">
                {t("canvas.fieldTitle")}
              </label>
              <Input
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                className="text-sm font-medium"
                maxLength={200}
                autoFocus
              />
            </div>
            <div>
              <label className="text-muted-foreground mb-1 block text-[10px] font-medium tracking-wide uppercase">
                {t("canvas.fieldSummary")}
              </label>
              <Textarea
                value={summary}
                onChange={(e) => setSummary(e.target.value)}
                rows={5}
                className="min-h-[100px] resize-y text-sm"
                placeholder={t("canvas.summaryPlaceholder")}
              />
            </div>
            <div>
              <label className="text-muted-foreground mb-1 block text-[10px] font-medium tracking-wide uppercase">
                {t("canvas.status")}
              </label>
              <select
                className="border-border bg-background w-full rounded-md border px-2 py-1.5 text-xs"
                value={status}
                onChange={(e) => setStatus(e.target.value as CanvasNodeStatus)}
              >
                {EDITABLE_STATUSES.map((s) => (
                  <option key={s} value={s}>
                    {s}
                  </option>
                ))}
              </select>
            </div>
            <div className="flex items-center gap-2">
              <span
                className={cn(
                  "inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold tracking-wide uppercase",
                  style.bg,
                  style.badge,
                )}
              >
                {t(roleLabelKey(role))}
              </span>
              <span className="text-muted-foreground text-[11px]">
                {t("canvas.rolePreview")}
              </span>
            </div>
            <Button
              type="button"
              size="sm"
              className="w-full"
              disabled={!dirty || saving}
              onClick={handleSave}
            >
              {saving ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Save className="h-4 w-4" />
              )}
              {saving ? t("canvas.saving") : t("canvas.saveNode")}
            </Button>
          </div>
        ) : null}

        <dl className="text-muted-foreground grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
          <dt>{t("canvas.source")}</dt>
          <dd className="text-foreground">{node.source}</dd>
          {payload.launch_mode ? (
            <>
              <dt>{t("canvas.launchMode")}</dt>
              <dd className="text-foreground">{payload.launch_mode}</dd>
            </>
          ) : null}
        </dl>

        {payload.acceptance ? (
          <div>
            <p className="mb-1 text-xs font-medium">{t("canvas.acceptance")}</p>
            <p className="text-muted-foreground text-xs whitespace-pre-wrap">
              {payload.acceptance}
            </p>
          </div>
        ) : null}

        {threadLinks.length > 0 ? (
          <div>
            <p className="mb-2 text-xs font-medium">{t("canvas.linkedThreads")}</p>
            <ul className="space-y-1">
              {threadLinks.map((link) => (
                <li key={link.id}>
                  <Link
                    to="/workspaces/$workspaceId/threads/$threadId"
                    params={{ workspaceId, threadId: link.ref_id as never }}
                    search={{ view: "chat" }}
                    className="text-primary inline-flex items-center gap-1 text-sm hover:underline"
                  >
                    <ExternalLink className="h-3.5 w-3.5" />
                    {link.snippet?.trim() || link.ref_id.slice(0, 8)}
                  </Link>
                </li>
              ))}
            </ul>
          </div>
        ) : null}

        {links.length === 0 && !isNote ? (
          <p className="text-muted-foreground text-xs">{t("canvas.noLinks")}</p>
        ) : null}
      </div>

      <div className="space-y-2 border-t px-5 py-3">
        {onSendToChat && (isNote || isGoal || isInsight) ? (
          <>
            {threadId ? (
              <Button
                type="button"
                className="w-full"
                disabled={launching || saving}
                onClick={() => withDraft((n) => onSendToChat(n, "current-thread"))}
              >
                {launching ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <MessageSquareText className="h-4 w-4" />
                )}
                {launching ? t("canvas.sending") : t("canvas.sendToCurrentChat")}
              </Button>
            ) : null}
            <Button
              type="button"
              variant={threadId ? "outline" : "default"}
              className="w-full"
              disabled={launching || saving}
              onClick={() => withDraft((n) => onSendToChat(n, "new-thread"))}
            >
              {launching ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <MessageSquareText className="h-4 w-4" />
              )}
              {t("canvas.startChatFromNode")}
            </Button>
          </>
        ) : null}

        {isInsight && openThreadId ? (
          <Button type="button" variant="outline" className="w-full" asChild>
            <Link
              to="/workspaces/$workspaceId/threads/$threadId"
              params={{ workspaceId, threadId: openThreadId as never }}
              search={{
                view: "chat",
                messageId: openMessageId ?? undefined,
              }}
            >
              <MessageSquareText className="h-4 w-4" />
              {openMessageId ? t("canvas.openMessage") : t("canvas.openThread")}
            </Link>
          </Button>
        ) : null}

        {isStage && payload.last_thread_id ? (
          <Button type="button" variant="outline" className="w-full" asChild>
            <Link
              to="/workspaces/$workspaceId/threads/$threadId"
              params={{ workspaceId, threadId: payload.last_thread_id as never }}
              search={{ view: "chat", runId: payload.last_run_id ?? undefined }}
            >
              <ExternalLink className="h-4 w-4" />
              {t("canvas.openLaunchThread")}
            </Link>
          </Button>
        ) : null}

        {isStage && onLaunchStage ? (
          <>
            {threadId ? (
              <Button
                type="button"
                className="w-full"
                disabled={launching || saving}
                onClick={() =>
                  withDraft((n) => onLaunchStage(n, defaultMode, "current-thread"))
                }
              >
                <Play className="h-4 w-4" />
                {launching ? t("canvas.launching") : t("canvas.launchInCurrentChat")}
              </Button>
            ) : null}
            <Button
              type="button"
              variant={threadId ? "outline" : "default"}
              className="w-full"
              disabled={launching || saving}
              onClick={() =>
                withDraft((n) => onLaunchStage(n, defaultMode, "new-thread"))
              }
            >
              <Play className="h-4 w-4" />
              {launching
                ? t("canvas.launching")
                : threadId
                  ? t("canvas.launchNewThread")
                  : t("canvas.launchStage")}
            </Button>
            {defaultMode === "single" ? (
              <Button
                type="button"
                variant="outline"
                className="w-full"
                disabled={launching || saving}
                onClick={() =>
                  withDraft((n) =>
                    onLaunchStage(
                      n,
                      "multi-role",
                      threadId ? "current-thread" : "new-thread",
                    ),
                  )
                }
              >
                {t("canvas.launchMultiRole")}
              </Button>
            ) : (
              <Button
                type="button"
                variant="outline"
                className="w-full"
                disabled={launching || saving}
                onClick={() =>
                  withDraft((n) =>
                    onLaunchStage(
                      n,
                      "single",
                      threadId ? "current-thread" : "new-thread",
                    ),
                  )
                }
              >
                {t("canvas.launchSingle")}
              </Button>
            )}
          </>
        ) : null}

        {isGoal && onDecomposeGoal ? (
          <Button
            type="button"
            variant="outline"
            className="w-full"
            onClick={() => onDecomposeGoal(node)}
          >
            {t("canvas.decomposeAgain")}
          </Button>
        ) : null}

        {(isStage || isGoal) && onMarkDone && node.status !== "Done" ? (
          <Button
            type="button"
            variant="outline"
            className="w-full"
            disabled={statusPending}
            onClick={() => onMarkDone(node)}
          >
            <CheckCircle2 className="h-4 w-4" />
            {t("canvas.markDone")}
          </Button>
        ) : null}

        <Button
          type="button"
          variant="outline"
          className="w-full"
          disabled={deleting}
          onClick={handleDelete}
        >
          <Trash2 className="h-4 w-4" />
          {t("canvas.deleteNode")}
        </Button>
      </div>
    </Modal>
  );
}
