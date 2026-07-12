/**
 * Session topic helpers: auto-title from the first user message, and
 * detect placeholder titles so we never overwrite a manual rename.
 */

const DEFAULT_TITLES = new Set([
  "新会话",
  "新會話",
  "New session",
  "New Session",
  "new session",
  "Untitled",
  "untitled",
]);

/** True when the title is still a system placeholder (safe to auto-replace). */
export function isDefaultThreadTitle(title: string | null | undefined): boolean {
  if (title == null) return true;
  const t = title.trim();
  if (!t) return true;
  if (DEFAULT_TITLES.has(t)) return true;
  // "新会话 2" / "New session 3"
  if (/^新会话\s*\d*$/u.test(t)) return true;
  if (/^new session\s*\d*$/iu.test(t)) return true;
  return false;
}

/** Max graphemes for auto-generated session topics (sidebar / header). */
export const AUTO_THREAD_TITLE_MAX_LEN = 10;

/**
 * Derive a short sidebar/header topic from the first user prompt.
 * Hard cap: ≤10 characters (ellipsis included when truncated).
 */
export function deriveThreadTitle(
  content: string,
  maxLen = AUTO_THREAD_TITLE_MAX_LEN,
): string {
  let text = content.replace(/\r\n/g, "\n").trim();
  if (!text) return "";

  // Prefer first non-empty line
  const firstLine = text
    .split("\n")
    .map((l) => l.trim())
    .find((l) => l.length > 0);
  text = firstLine ?? text;

  // Strip common task wrappers
  text = text
    .replace(/^【\s*任务\s*】\s*/u, "")
    .replace(/^\[\s*任务\s*\]\s*/u, "")
    .replace(/^task\s*[:：]\s*/iu, "")
    .replace(/^#+\s*/, "")
    .trim();

  // Collapse whitespace
  text = text.replace(/\s+/g, " ");

  if (!text) return "";

  const chars = [...text];
  if (chars.length <= maxLen) return text;

  // Leave room for ellipsis within the maxLen budget (e.g. 9 + "…").
  const bodyBudget = Math.max(1, maxLen - 1);
  let cut = chars.slice(0, bodyBudget).join("");
  // Prefer break at punctuation / space in the latter half
  const breakChars = ["，", "。", "、", ",", ".", " ", "：", ":", "；", ";"];
  let best = -1;
  for (const ch of breakChars) {
    const idx = cut.lastIndexOf(ch);
    if (idx > bodyBudget * 0.4) best = Math.max(best, idx);
  }
  if (best > 0) {
    cut = cut.slice(0, best);
  }
  cut = cut.replace(/[，。、,.\s：:;；]+$/u, "");
  if (!cut) {
    cut = chars.slice(0, bodyBudget).join("");
  }
  const withEllipsis = `${cut}…`;
  // Enforce hard cap even if cut was short then extended
  return [...withEllipsis].slice(0, maxLen).join("");
}
