/** Languages that must stay plain text (ASCII diagrams, logs, dumps). */
const PLAIN_FENCE_LANGUAGES = new Set([
  "text",
  "txt",
  "plain",
  "plaintext",
  "ascii",
  "diagram",
  "art",
]);

/** Box-drawing / arrow characters common in architecture ASCII art. */
const ASCII_DIAGRAM_MARKERS = /[┌┐└┘├┤┬┴┼─│═║╔╗╚╝╠╣╦╩╬▶▼▲◀→←↑↓▸▾▴◂━┃┏┓┗┛┣┫┳┻╋]/u;

/**
 * Detect Unicode/ASCII architecture diagrams so we never run syntax
 * highlighters that insert spans and break column alignment.
 */
export function looksLikeAsciiDiagram(text: string): boolean {
  if (!text.includes("\n")) return false;
  const lines = text.split("\n");
  if (lines.length < 2) return false;
  let markerLines = 0;
  for (const line of lines) {
    if (ASCII_DIAGRAM_MARKERS.test(line)) markerLines += 1;
  }
  // At least two lines with box/arrow marks → treat as diagram.
  return markerLines >= 2;
}

export function isPlainFenceLanguage(language: string | null): boolean {
  if (!language) return true;
  return PLAIN_FENCE_LANGUAGES.has(language.toLowerCase());
}

/**
 * Whether a fenced block should be rendered as monospaced plain text
 * (no highlight.js). Prefer this for unlabeled fences and ASCII art.
 */
export function shouldRenderAsPlainCode(text: string, language: string | null): boolean {
  if (isPlainFenceLanguage(language)) return true;
  if (looksLikeAsciiDiagram(text)) return true;
  return false;
}

export function languageFromClassName(className?: string): string | null {
  if (!className) return null;
  const match = /language-([\w-]+)/.exec(className);
  return match?.[1]?.toLowerCase() ?? null;
}

export function escapeHtml(code: string): string {
  return code.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}
