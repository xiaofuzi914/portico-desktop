import { useCallback, useMemo, useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

export interface JsonTreeProps {
  value: unknown;
  /** Optional section title above the tree. */
  title?: string;
  /** How many levels open by default (root = depth 0). Default 1. */
  defaultExpandDepth?: number;
  className?: string;
  /** Max height for the scroll area. */
  maxHeightClassName?: string;
  expandAllLabel?: string;
  collapseAllLabel?: string;
}

type JsonKind = "object" | "array" | "string" | "number" | "boolean" | "null" | "undefined";

function kindOf(value: unknown): JsonKind {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  if (Array.isArray(value)) return "array";
  if (typeof value === "object") return "object";
  if (typeof value === "string") return "string";
  if (typeof value === "number") return "number";
  if (typeof value === "boolean") return "boolean";
  return "undefined";
}

function isExpandable(value: unknown): boolean {
  const k = kindOf(value);
  if (k === "object") return Object.keys(value as object).length > 0;
  if (k === "array") return (value as unknown[]).length > 0;
  if (k === "string") return (value as string).length > 80 || (value as string).includes("\n");
  return false;
}

function collectionSize(value: unknown): number {
  if (Array.isArray(value)) return value.length;
  if (value && typeof value === "object") return Object.keys(value).length;
  return 0;
}

function summaryLabel(value: unknown): string {
  const k = kindOf(value);
  if (k === "array") {
    const n = (value as unknown[]).length;
    return n === 0 ? "[]" : `Array(${n})`;
  }
  if (k === "object") {
    const n = Object.keys(value as object).length;
    return n === 0 ? "{}" : `{${n}}`;
  }
  if (k === "string") {
    const s = value as string;
    const oneLine = s.replace(/\s+/g, " ").trim();
    const clipped = oneLine.length > 48 ? `${oneLine.slice(0, 47)}…` : oneLine;
    return `"${clipped}"`;
  }
  if (k === "null") return "null";
  if (k === "undefined") return "undefined";
  return String(value);
}

function typeClass(kind: JsonKind): string {
  switch (kind) {
    case "string":
      return "text-emerald-700 dark:text-emerald-400";
    case "number":
      return "text-sky-700 dark:text-sky-400";
    case "boolean":
      return "text-amber-700 dark:text-amber-400";
    case "null":
    case "undefined":
      return "text-muted-foreground italic";
    case "object":
    case "array":
      return "text-muted-foreground";
    default:
      return "text-foreground";
  }
}

function formatPrimitive(value: unknown): string {
  const k = kindOf(value);
  if (k === "string") return JSON.stringify(value);
  if (k === "null") return "null";
  if (k === "undefined") return "undefined";
  return String(value);
}

function pathKey(parentPath: string, key: string | number): string {
  return parentPath ? `${parentPath}.${key}` : String(key);
}

/** Collect paths that should start open (collections up to maxDepth). */
function collectOpenPaths(value: unknown, maxDepth: number): Set<string> {
  const out = new Set<string>();

  const walk = (v: unknown, path: string, depth: number) => {
    if (!isExpandable(v)) return;
    const k = kindOf(v);
    // Long strings stay collapsed by default (user expands to read).
    if (k === "string") return;
    if (depth <= maxDepth) {
      out.add(path || "root");
    }
    if (depth >= maxDepth) return;
    if (Array.isArray(v)) {
      v.forEach((item, i) => walk(item, pathKey(path, i), depth + 1));
    } else if (v && typeof v === "object") {
      for (const [key, child] of Object.entries(v as Record<string, unknown>)) {
        walk(child, pathKey(path, key), depth + 1);
      }
    }
  };

  walk(value, "", 0);
  return out;
}

function collectAllPaths(value: unknown): Set<string> {
  const out = new Set<string>();

  const walk = (v: unknown, path: string) => {
    if (!isExpandable(v)) return;
    out.add(path || "root");
    if (Array.isArray(v)) {
      v.forEach((item, i) => walk(item, pathKey(path, i)));
    } else if (v && typeof v === "object") {
      for (const [key, child] of Object.entries(v as Record<string, unknown>)) {
        walk(child, pathKey(path, key));
      }
    }
  };

  walk(value, "");
  return out;
}

function JsonNode({
  name,
  value,
  path,
  depth,
  openPaths,
  toggle,
}: {
  name?: string;
  value: unknown;
  path: string;
  depth: number;
  openPaths: Set<string>;
  toggle: (path: string) => void;
}) {
  const kind = kindOf(value);
  const expandable = isExpandable(value);
  const nodePath = path || "root";
  const open = expandable && openPaths.has(nodePath);
  const size = collectionSize(value);

  const keyEl =
    name !== undefined ? (
      <span className="text-violet-800 dark:text-violet-300">
        {name}
        <span className="text-muted-foreground">: </span>
      </span>
    ) : null;

  // Long string: expandable leaf
  if (kind === "string" && expandable) {
    const text = value as string;
    return (
      <div className="min-w-0">
        <button
          type="button"
          className="hover:bg-muted/50 flex w-full items-start gap-0.5 rounded px-0.5 py-px text-left font-mono text-[10px] leading-relaxed"
          style={{ paddingLeft: depth * 12 }}
          onClick={() => toggle(nodePath)}
          aria-expanded={open}
        >
          <span className="text-muted-foreground mt-px w-3 shrink-0">
            {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
          </span>
          <span className="min-w-0 break-all">
            {keyEl}
            {open ? (
              <span className={cn(typeClass("string"), "whitespace-pre-wrap")}>
                {JSON.stringify(text)}
              </span>
            ) : (
              <>
                <span className={typeClass("string")}>{summaryLabel(text)}</span>
                <span className="text-muted-foreground ml-1">({text.length} chars)</span>
              </>
            )}
          </span>
        </button>
      </div>
    );
  }

  // Non-expandable primitive
  if (!expandable || (kind !== "object" && kind !== "array")) {
    return (
      <div
        className="flex min-w-0 items-start gap-0.5 px-0.5 py-px font-mono text-[10px] leading-relaxed"
        style={{ paddingLeft: depth * 12 }}
      >
        <span className="w-3 shrink-0" aria-hidden />
        <span className="min-w-0 break-all">
          {keyEl}
          <span className={typeClass(kind)}>{formatPrimitive(value)}</span>
        </span>
      </div>
    );
  }

  // Object / array
  const isArray = kind === "array";
  const entries: Array<[string, unknown]> = isArray
    ? (value as unknown[]).map((item, i) => [String(i), item])
    : Object.entries(value as Record<string, unknown>);

  return (
    <div className="min-w-0">
      <button
        type="button"
        className="hover:bg-muted/50 flex w-full items-start gap-0.5 rounded px-0.5 py-px text-left font-mono text-[10px] leading-relaxed"
        style={{ paddingLeft: depth * 12 }}
        onClick={() => toggle(nodePath)}
        aria-expanded={open}
      >
        <span className="text-muted-foreground mt-px w-3 shrink-0">
          {open ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
        </span>
        <span className="min-w-0">
          {keyEl}
          <span className="text-muted-foreground">{isArray ? "[" : "{"}</span>
          {!open ? (
            <>
              <span className="text-muted-foreground mx-0.5">…</span>
              <span className="text-muted-foreground">{isArray ? "]" : "}"}</span>
              <span className="text-muted-foreground ml-1">
                {isArray ? `${size} items` : `${size} keys`}
              </span>
            </>
          ) : null}
        </span>
      </button>
      {open ? (
        <>
          {entries.map(([key, child]) => (
            <JsonNode
              key={pathKey(nodePath, key)}
              name={key}
              value={child}
              path={pathKey(path, key)}
              depth={depth + 1}
              openPaths={openPaths}
              toggle={toggle}
            />
          ))}
          <div
            className="text-muted-foreground px-0.5 py-px font-mono text-[10px] leading-relaxed"
            style={{ paddingLeft: depth * 12 + 14 }}
          >
            {isArray ? "]" : "}"}
          </div>
        </>
      ) : null}
    </div>
  );
}

/**
 * Hierarchical JSON viewer: each object/array/long-string can expand or collapse level by level.
 *
 * Remount with a stable `key` when the root value changes so expand state resets cleanly.
 */
export function JsonTree({
  value,
  title,
  defaultExpandDepth = 1,
  className,
  maxHeightClassName = "max-h-72",
  expandAllLabel = "Expand all",
  collapseAllLabel = "Collapse",
}: JsonTreeProps) {
  const initialOpen = useMemo(
    () => collectOpenPaths(value, defaultExpandDepth),
    [value, defaultExpandDepth],
  );

  const [openPaths, setOpenPaths] = useState<Set<string>>(() => initialOpen);

  const toggle = useCallback((path: string) => {
    setOpenPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const expandAll = useCallback(() => {
    setOpenPaths(collectAllPaths(value));
  }, [value]);

  const collapseAll = useCallback(() => {
    setOpenPaths(new Set());
  }, []);

  const rootKind = kindOf(value);
  const rootExpandable = isExpandable(value);

  return (
    <div className={cn("min-w-0", className)}>
      {(title || rootExpandable) && (
        <div className="mb-1 flex items-center gap-2">
          {title ? (
            <div className="text-muted-foreground text-[10px] font-semibold tracking-wide uppercase">
              {title}
            </div>
          ) : null}
          {rootExpandable ? (
            <div className="ml-auto flex items-center gap-1">
              <button
                type="button"
                className="text-muted-foreground hover:text-foreground hover:bg-muted rounded px-1 py-px text-[10px]"
                onClick={expandAll}
              >
                {expandAllLabel}
              </button>
              <span className="text-muted-foreground text-[10px]">·</span>
              <button
                type="button"
                className="text-muted-foreground hover:text-foreground hover:bg-muted rounded px-1 py-px text-[10px]"
                onClick={collapseAll}
              >
                {collapseAllLabel}
              </button>
            </div>
          ) : null}
        </div>
      )}
      <div
        className={cn(
          "bg-background/80 overflow-auto rounded border px-1.5 py-1",
          maxHeightClassName,
        )}
      >
        {rootKind === "object" || rootKind === "array" || rootExpandable ? (
          <JsonNode value={value} path="" depth={0} openPaths={openPaths} toggle={toggle} />
        ) : (
          <div className={cn("px-1 py-0.5 font-mono text-[10px]", typeClass(rootKind))}>
            {formatPrimitive(value)}
          </div>
        )}
      </div>
    </div>
  );
}
