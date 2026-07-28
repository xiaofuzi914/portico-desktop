import type { RunEvent, RuntimeEvent } from "@/lib/schemas";
import { parseRuntimeEvent } from "@/lib/tauri-api";

export type FileChangeKind = "written" | "edited" | "mentioned" | "artifact";

export type MessageFileRef = {
  /** Workspace-relative path when possible (e.g. ARCHITECTURE_DEEP.md). */
  path: string;
  kind: FileChangeKind;
};

const WRITE_TOOLS = new Set([
  "fs_write",
  "fs_edit",
  "write_file",
  "edit_file",
  "apply_patch",
]);

const FILE_EXT =
  "md|mdx|txt|json|ts|tsx|js|jsx|rs|py|toml|yaml|yml|css|html|sql|sh|go|java|kt|swift|c|cpp|h|hpp|xml|svg|plantuml|puml";

/** Bare or relative path that looks like a project file. */
const PATH_RE = new RegExp(
  String.raw`(?:^|[\s\("'\[{])((?:[\w.-]+/)*[\w.-]+\.(?:${FILE_EXT}))(?=$|[\s\)"'\]},:：])`,
  "gi",
);

const BACKTICK_PATH_RE = new RegExp(
  String.raw`\`((?:[\w.-]+/)*[\w.-]+\.(?:${FILE_EXT}))\``,
  "gi",
);

function normalizePath(raw: string): string | null {
  let path = raw.trim().replace(/\\/g, "/");
  if (!path || path.includes("..")) return null;
  // Strip absolute prefixes → keep trailing relative segment when possible.
  const homeLike = path.match(/\/(?:Users|home)\/[^/]+\/(?:Documents|Program|projects?|src)\/[^/]+\/(.+)$/i);
  if (homeLike?.[1]) path = homeLike[1];
  const absRoot = path.match(/^\/[^/]+\/(.+\.(?:md|ts|tsx|rs|py|json))$/i);
  if (path.startsWith("/") && absRoot) {
    // Prefer last 3 segments for deep absolute paths.
    const parts = path.split("/").filter(Boolean);
    path = parts.slice(-Math.min(4, parts.length)).join("/");
  }
  if (path.startsWith("/")) {
    const parts = path.split("/").filter(Boolean);
    path = parts[parts.length - 1] ?? path;
  }
  if (!path || path.length > 260) return null;
  return path;
}

function pathFromUnknown(value: unknown): string | null {
  if (typeof value === "string") return normalizePath(value);
  if (!value || typeof value !== "object") return null;
  const rec = value as Record<string, unknown>;
  for (const key of ["path", "file", "file_path", "relative_path", "target"]) {
    if (typeof rec[key] === "string") {
      const n = normalizePath(rec[key] as string);
      if (n) return n;
    }
  }
  return null;
}

/** Extract file-like paths from assistant markdown / plain text. */
export function extractPathsFromText(text: string): MessageFileRef[] {
  if (!text.trim()) return [];
  const found = new Map<string, MessageFileRef>();

  for (const re of [BACKTICK_PATH_RE, PATH_RE]) {
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      const n = normalizePath(m[1] ?? "");
      if (!n) continue;
      if (!found.has(n)) {
        found.set(n, { path: n, kind: "mentioned" });
      }
    }
  }
  return [...found.values()];
}

function runtimeFromRunEvent(event: RunEvent): RuntimeEvent | undefined {
  // Stored payload may be the full RuntimeEvent, or { kind, data } already.
  return parseRuntimeEvent(event.payload) ?? parseRuntimeEvent({
    kind: event.event_type,
    data: event.payload,
  });
}

/**
 * Collect written/edited/artifact paths from durable run events.
 * Prefer ToolRequested arguments (path) for fs_write / fs_edit.
 */
export function collectFileRefsFromRunEvents(events: RunEvent[] | undefined): MessageFileRef[] {
  if (!events?.length) return [];
  const found = new Map<string, MessageFileRef>();

  const upsert = (path: string, kind: FileChangeKind) => {
    const existing = found.get(path);
    // Prefer write/edit over mere mention.
    const rank = { written: 3, edited: 3, artifact: 2, mentioned: 1 } as const;
    if (!existing || rank[kind] >= rank[existing.kind]) {
      found.set(path, { path, kind });
    }
  };

  for (const event of events) {
    const runtime = runtimeFromRunEvent(event);
    if (!runtime) continue;

    if (runtime.kind === "ToolRequested") {
      const tool = runtime.data.tool_name;
      if (!WRITE_TOOLS.has(tool)) continue;
      const path = pathFromUnknown(runtime.data.arguments);
      if (path) {
        upsert(path, tool.includes("edit") ? "edited" : "written");
      }
    } else if (runtime.kind === "ToolCompleted") {
      const tool = runtime.data.tool_name;
      if (!WRITE_TOOLS.has(tool)) continue;
      const path = pathFromUnknown(runtime.data.result);
      if (path) {
        upsert(path, tool.includes("edit") ? "edited" : "written");
      }
    } else if (runtime.kind === "ArtifactCreated") {
      const art = runtime.data.artifact;
      const path =
        (art.path && normalizePath(art.path)) ||
        (art.name && normalizePath(art.name)) ||
        null;
      if (path) upsert(path, "artifact");
    }
  }

  return [...found.values()];
}

/** Merge tool-derived changes with text mentions; tool changes win on kind. */
export function mergeFileRefs(
  fromTools: MessageFileRef[],
  fromText: MessageFileRef[],
): MessageFileRef[] {
  const map = new Map<string, MessageFileRef>();
  for (const ref of fromText) map.set(ref.path, ref);
  for (const ref of fromTools) {
    const prev = map.get(ref.path);
    if (!prev || prev.kind === "mentioned") {
      map.set(ref.path, ref);
    }
  }
  return [...map.values()].sort((a, b) => a.path.localeCompare(b.path));
}

export function isPreviewablePath(path: string): boolean {
  return /\.(md|mdx|txt|json|ts|tsx|js|jsx|rs|py|toml|yaml|yml|css|html|sql|sh|go|c|cpp|h|xml|svg)$/i.test(
    path,
  );
}

export function isMarkdownPath(path: string): boolean {
  return /\.(md|mdx)$/i.test(path);
}
