import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Modal } from "@/components/ui/modal";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/lib/i18n-react";
import { learningKeys } from "@/lib/query-keys";
import type { WorkflowPattern } from "@/lib/schemas";
import {
  acceptWorkflowPattern,
  editWorkflowPattern,
  listWorkflowPatternEvidence,
  muteWorkflowPattern,
  rejectWorkflowPattern,
} from "@/lib/tauri-api";

interface WorkflowPatternDetailProps {
  pattern: WorkflowPattern | null;
  open: boolean;
  onClose: () => void;
}

export function WorkflowPatternDetail({ pattern, open, onClose }: WorkflowPatternDetailProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [advanced, setAdvanced] = useState(false);
  const [name, setName] = useState("");
  const [summary, setSummary] = useState("");
  const [trigger, setTrigger] = useState("");
  const [roles, setRoles] = useState("");
  const [collab, setCollab] = useState("");
  const [toolStrategy, setToolStrategy] = useState("");
  const [outputKind, setOutputKind] = useState("");

  useEffect(() => {
    if (!open || !pattern) return;
    setName(pattern.name);
    setSummary(pattern.summary);
    setTrigger(pattern.trigger_text);
    setRoles(pattern.preferred_roles.join(", "));
    setCollab(pattern.collaboration_style);
    setToolStrategy(pattern.tool_strategy ?? "");
    setOutputKind(pattern.output_kind ?? "");
    setAdvanced(false);
  }, [open, pattern]);

  const evidenceQuery = useQuery({
    queryKey: ["pattern-evidence", pattern?.id],
    enabled: open && !!pattern,
    queryFn: () => listWorkflowPatternEvidence(pattern!.id),
  });

  const invalidate = async () => {
    await queryClient.invalidateQueries({ queryKey: learningKeys.patterns("all") });
    await queryClient.invalidateQueries({ queryKey: learningKeys.overview() });
    await queryClient.invalidateQueries({ queryKey: ["workflow-patterns"] });
  };

  const save = useMutation({
    mutationFn: () =>
      editWorkflowPattern(pattern!.id, {
        name: name.trim() || null,
        summary: summary.trim() || null,
        trigger_text: trigger.trim() || null,
        preferred_roles: roles
          .split(",")
          .map((r) => r.trim())
          .filter(Boolean),
        collaboration_style: collab.trim() || null,
        tool_strategy: toolStrategy.trim() || null,
        output_kind: outputKind.trim() || null,
      }),
    onSuccess: async () => {
      await invalidate();
      onClose();
    },
  });

  const accept = useMutation({
    mutationFn: () => acceptWorkflowPattern(pattern!.id),
    onSuccess: invalidate,
  });
  const reject = useMutation({
    mutationFn: () => rejectWorkflowPattern(pattern!.id),
    onSuccess: async () => {
      await invalidate();
      onClose();
    },
  });
  const mute = useMutation({
    mutationFn: () => muteWorkflowPattern(pattern!.id),
    onSuccess: async () => {
      await invalidate();
      onClose();
    },
  });

  if (!pattern) return null;
  const status = String(pattern.status).toLowerCase();
  const evidence = evidenceQuery.data;

  return (
    <Modal open={open} onClose={onClose} labelledBy="pattern-detail-title" className="max-w-lg">
      <div className="space-y-4 p-4">
        <div className="flex items-start justify-between gap-3">
          <div>
            <h2 id="pattern-detail-title" className="text-base font-semibold">
              {pattern.name}
            </h2>
            <p className="text-muted-foreground text-xs uppercase">{pattern.status}</p>
          </div>
          <Button size="sm" variant="outline" onClick={onClose}>
            {t("common.cancel")}
          </Button>
        </div>

        <div className="text-muted-foreground space-y-1 text-xs">
          <p>
            ✓{pattern.success_count} · ✗{pattern.failure_count}
            {evidence ? ` · evidence ${evidence.evidence_count}` : ""}
            {evidence ? ` · conf ${(evidence.confidence * 100).toFixed(0)}%` : ""}
          </p>
          {pattern.preferred_roles.length > 0 && (
            <p>
              {t("memory.center.roles")}: {pattern.preferred_roles.join(" → ")}
            </p>
          )}
          {pattern.fingerprint && (
            <p className="break-all opacity-70">
              {t("memory.center.matchId")}: {pattern.fingerprint.slice(0, 24)}…
            </p>
          )}
        </div>

        <div className="space-y-2">
          <label className="text-xs font-medium">{t("memory.center.fieldName")}</label>
          <Input value={name} onChange={(e) => setName(e.target.value)} />
          <label className="text-xs font-medium">{t("memory.center.fieldSummary")}</label>
          <Textarea value={summary} onChange={(e) => setSummary(e.target.value)} rows={2} />
          <label className="text-xs font-medium">{t("memory.center.fieldTriggers")}</label>
          <Input value={trigger} onChange={(e) => setTrigger(e.target.value)} />
        </div>

        <button
          type="button"
          className="text-primary text-xs underline-offset-2 hover:underline"
          onClick={() => setAdvanced((v) => !v)}
        >
          {advanced ? t("memory.center.hideAdvanced") : t("memory.center.showAdvanced")}
        </button>

        {advanced && (
          <div className="space-y-2 rounded-md border p-3">
            <label className="text-xs font-medium">{t("memory.center.fieldRoles")}</label>
            <Input
              value={roles}
              onChange={(e) => setRoles(e.target.value)}
              placeholder="Explorer, Coder, Reviewer"
            />
            <label className="text-xs font-medium">{t("memory.center.fieldCollab")}</label>
            <Input value={collab} onChange={(e) => setCollab(e.target.value)} />
            <label className="text-xs font-medium">{t("memory.center.fieldToolStrategy")}</label>
            <Input value={toolStrategy} onChange={(e) => setToolStrategy(e.target.value)} />
            <label className="text-xs font-medium">{t("memory.center.fieldOutput")}</label>
            <Input value={outputKind} onChange={(e) => setOutputKind(e.target.value)} />
            <p className="text-muted-foreground text-[11px]">{t("memory.center.noToolPermission")}</p>
          </div>
        )}

        <div className="flex flex-wrap gap-2">
          <Button size="sm" disabled={save.isPending} onClick={() => save.mutate()}>
            {t("common.save")}
          </Button>
          {status === "suggested" && (
            <>
              <Button size="sm" onClick={() => accept.mutate()}>
                {t("memory.center.acceptEnable")}
              </Button>
              <Button size="sm" variant="outline" onClick={() => reject.mutate()}>
                {t("memory.capabilities.rejectPattern")}
              </Button>
            </>
          )}
          {status !== "muted" && status !== "rejected" && (
            <Button size="sm" variant="outline" onClick={() => mute.mutate()}>
              {t("memory.capabilities.mutePattern")}
            </Button>
          )}
        </div>
      </div>
    </Modal>
  );
}
