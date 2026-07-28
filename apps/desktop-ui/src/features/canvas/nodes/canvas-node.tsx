import { Handle, Position, type NodeProps } from "@xyflow/react";
import { useTranslation } from "@/lib/i18n-react";
import { roleFromNode, roleLabelKey } from "../canvas-node-role";
import {
  kindStyle,
  narrativeMeta,
  type CanvasFlowNode,
} from "../canvas-view-model";
import { cn } from "@/lib/utils";

/**
 * Handles stay invisible until hover/selection so the map is not a field of dots.
 * Top/bottom are **centered** (no left/top % offset) — offset source handles made
 * short vertical edges S-curve and flipped arrow orientation.
 */
const handleClass = cn(
  "!h-2.5 !w-2.5 !min-h-0 !min-w-0 !border !border-background !bg-muted-foreground/50",
  "!opacity-0 transition-opacity duration-150",
  "group-hover/node:!opacity-100 group-focus-within/node:!opacity-100",
  "[[data-selected=true]_&]:!opacity-100",
);

/**
 * Four-side source + target handles so Parent/Related edges can attach with a
 * clear direction (down / up / left / right).
 */
export function CanvasFlowNodeView({ data, selected }: NodeProps<CanvasFlowNode>) {
  const { t } = useTranslation();
  const style = kindStyle(data.kind);
  const narrative = narrativeMeta(data.canvasNode);
  const isBranch = narrative.role === "branch";
  const isSession = data.kind === "ThreadCluster" || narrative.role === "root";
  const isRoot = isSession;
  const nodeRole = roleFromNode(data.canvasNode);
  // Unified role label (便签 / 会话 / 阶段 …) — not raw backend kind names.
  const badge = t(roleLabelKey(nodeRole));
  const highlighted = Boolean(data.highlighted);

  // Branch cards often have title === badge ("意图"); prefer summary as body.
  const branchBody =
    isBranch && data.summary
      ? data.summary
      : isBranch && data.label === narrative.branchLabel
        ? ""
        : data.label;

  return (
    <div
      data-selected={selected ? "true" : "false"}
      title={
        data.summary
          ? `${data.label}\n${data.summary}\n\n${t("canvas.clickToOpenSession")}`
          : `${data.label}\n\n${t("canvas.clickToOpenSession")}`
      }
      className={cn(
        "group/node box-border overflow-visible rounded-xl border px-3 py-2.5 shadow-sm backdrop-blur-sm transition-shadow",
        isSession ? "h-[148px] w-[280px] cursor-pointer" : "w-[248px]",
        !isSession && (isBranch ? "h-[72px]" : "h-[104px]"),
        isRoot && "border-emerald-500/50 bg-emerald-500/15",
        isBranch && narrative.branchStyle,
        !isRoot && !isBranch && style.bg,
        !isRoot && !isBranch && style.border,
        selected && "ring-primary ring-2 ring-offset-1",
        highlighted && "ring-sky-500/80 shadow-md ring-2 ring-offset-1",
        isSession && "hover:shadow-md hover:border-emerald-500/70",
      )}
    >
      {/* Targets — inbound (children / related land here) */}
      <Handle type="target" id="in-top" position={Position.Top} className={handleClass} />
      <Handle type="target" id="in-bottom" position={Position.Bottom} className={handleClass} />
      <Handle type="target" id="in-left" position={Position.Left} className={handleClass} />
      <Handle type="target" id="in-right" position={Position.Right} className={handleClass} />

      {/* Sources — outbound (structure leaves from parent toward children) */}
      <Handle type="source" id="out-top" position={Position.Top} className={handleClass} />
      <Handle type="source" id="out-bottom" position={Position.Bottom} className={handleClass} />
      <Handle type="source" id="out-left" position={Position.Left} className={handleClass} />
      <Handle type="source" id="out-right" position={Position.Right} className={handleClass} />

      <div className="flex items-center justify-between gap-2">
        <span
          className={cn(
            "text-[10px] font-semibold tracking-wide uppercase",
            isRoot
              ? "text-emerald-700 dark:text-emerald-300"
              : isBranch
                ? narrative.branchBadge
                : style.badge,
          )}
        >
          {badge}
        </span>
        {highlighted ? (
          <span className="text-[10px] font-medium text-sky-600 dark:text-sky-300">
            {t("canvas.currentSession")}
          </span>
        ) : !isBranch ? (
          <span className="text-muted-foreground shrink-0 text-[10px]">{data.status}</span>
        ) : null}
      </div>
      {isBranch ? (
        branchBody ? (
          <p className="text-muted-foreground mt-1 line-clamp-1 text-xs leading-snug">
            {branchBody}
          </p>
        ) : null
      ) : (
        <>
          <p className="text-foreground mt-1 line-clamp-2 text-sm leading-snug font-medium">
            {data.label}
          </p>
          {data.summary ? (
            <p
              className={cn(
                "text-muted-foreground mt-0.5 text-xs leading-snug",
                isSession ? "line-clamp-4" : "line-clamp-2",
              )}
            >
              {data.summary}
            </p>
          ) : null}
        </>
      )}
    </div>
  );
}
