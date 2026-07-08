export function normalizeDirectorySelection(selection: string | string[] | null): string | null {
  if (!selection) return null;
  if (Array.isArray(selection)) return selection[0] ?? null;
  return selection;
}

export function deriveProjectNameFromPath(path: string): string {
  const normalized = path.replace(/[/\\]+$/, "");
  const segments = normalized.split(/[/\\]/).filter(Boolean);
  return segments.at(-1) ?? "Project";
}
