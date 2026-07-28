/**
 * Expand a text selection to word / sentence / paragraph units.
 * Works for Chinese + Latin mixed content (conversation markdown).
 */

export type SelectionUnit = "word" | "sentence" | "paragraph";

const SENTENCE_END = /[。！？!?；;\n]/;
const WORD_BOUNDARY = /[\s\n，,、。！？!?；;:：""''「」『』（）()[\]{}<>《》]/;

/** Block-level tags used as paragraph containers in markdown output. */
const BLOCK_SELECTOR =
  "p, li, h1, h2, h3, h4, h5, h6, pre, blockquote, td, th, article, section";

export function findSelectionBlock(
  node: Node | null,
  root: HTMLElement,
): HTMLElement | null {
  if (!node) return null;
  let el: HTMLElement | null =
    node.nodeType === Node.TEXT_NODE
      ? (node.parentElement as HTMLElement | null)
      : (node as HTMLElement);
  while (el && el !== root) {
    if (el.matches?.(BLOCK_SELECTOR)) return el;
    el = el.parentElement;
  }
  // Fallback: nearest element with substantial text under root.
  el =
    node.nodeType === Node.TEXT_NODE
      ? (node.parentElement as HTMLElement | null)
      : (node as HTMLElement);
  while (el && el !== root) {
    if ((el.innerText?.trim().length ?? 0) > 0 && el.children.length <= 8) {
      return el;
    }
    el = el.parentElement;
  }
  return root;
}

/**
 * Expand offsets within `full` around [start, end) to the given unit.
 * Offsets are UTF-16 code unit indices (JS string).
 */
export function expandOffsets(
  full: string,
  start: number,
  end: number,
  unit: SelectionUnit,
): { start: number; end: number } {
  const len = full.length;
  let s = Math.max(0, Math.min(start, len));
  let e = Math.max(s, Math.min(end, len));

  if (unit === "paragraph") {
    return { start: 0, end: len };
  }

  if (unit === "sentence") {
    while (s > 0 && !SENTENCE_END.test(full[s - 1]!)) s -= 1;
    while (e < len && !SENTENCE_END.test(full[e]!)) e += 1;
    // Include trailing sentence punctuation.
    if (e < len && SENTENCE_END.test(full[e]!)) e += 1;
    // Skip leading whitespace after previous break.
    while (s < e && /\s/.test(full[s]!)) s += 1;
    return { start: s, end: e };
  }

  // word — Latin word or contiguous CJK/run without punctuation
  while (s > 0 && !WORD_BOUNDARY.test(full[s - 1]!)) s -= 1;
  while (e < len && !WORD_BOUNDARY.test(full[e]!)) e += 1;
  while (s < e && /\s/.test(full[s]!)) s += 1;
  while (e > s && /\s/.test(full[e - 1]!)) e -= 1;
  return { start: s, end: e };
}

/**
 * Map a DOM Range into offsets inside a block's plain text (innerText-like via textContent walk).
 * Returns null if the range is outside the block.
 */
export function rangeToOffsetsInBlock(
  range: Range,
  block: HTMLElement,
): { full: string; start: number; end: number } | null {
  const full = block.innerText.replace(/\r\n/g, "\n");
  if (!full.trim()) return null;

  // Build a temporary range from block start to selection points.
  try {
    const preStart = document.createRange();
    preStart.selectNodeContents(block);
    preStart.setEnd(range.startContainer, range.startOffset);
    const start = preStart.toString().replace(/\r\n/g, "\n").length;

    const preEnd = document.createRange();
    preEnd.selectNodeContents(block);
    preEnd.setEnd(range.endContainer, range.endOffset);
    const end = preEnd.toString().replace(/\r\n/g, "\n").length;

    return {
      full,
      start: Math.max(0, Math.min(start, full.length)),
      end: Math.max(0, Math.min(Math.max(start, end), full.length)),
    };
  } catch {
    // Fallback: selection string only.
    const selected = range.toString().replace(/\r\n/g, "\n");
    const idx = full.indexOf(selected);
    if (idx < 0) {
      return { full, start: 0, end: full.length };
    }
    return { full, start: idx, end: idx + selected.length };
  }
}

export function expandSelectionInContainer(
  root: HTMLElement,
  unit: SelectionUnit,
): { text: string; blockText: string; unit: SelectionUnit } | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return null;
  const range = sel.getRangeAt(0);
  const common = range.commonAncestorContainer;
  const node =
    common.nodeType === Node.TEXT_NODE ? common.parentElement : (common as Element);
  if (!node || !root.contains(node)) return null;

  const block = findSelectionBlock(common, root);
  if (!block) return null;

  const mapped = rangeToOffsetsInBlock(range, block);
  if (!mapped) return null;

  const { start, end } = expandOffsets(mapped.full, mapped.start, mapped.end, unit);
  const text = mapped.full.slice(start, end).trim();
  if (!text) return null;

  // Try to re-select expanded region in the DOM for visual feedback.
  trySelectOffsetsInBlock(block, start, end);

  return { text, blockText: mapped.full.trim(), unit };
}

function trySelectOffsetsInBlock(block: HTMLElement, start: number, end: number) {
  try {
    const points = textOffsetsToBoundaryPoints(block, start, end);
    if (!points) return;
    const sel = window.getSelection();
    if (!sel) return;
    const r = document.createRange();
    r.setStart(points.startNode, points.startOffset);
    r.setEnd(points.endNode, points.endOffset);
    sel.removeAllRanges();
    sel.addRange(r);
  } catch {
    /* visual expand is best-effort */
  }
}

function textOffsetsToBoundaryPoints(
  root: HTMLElement,
  start: number,
  end: number,
): {
  startNode: Node;
  startOffset: number;
  endNode: Node;
  endOffset: number;
} | null {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let count = 0;
  let startNode: Node | null = null;
  let startOffset = 0;
  let endNode: Node | null = null;
  let endOffset = 0;
  let node = walker.nextNode();
  while (node) {
    const len = node.textContent?.length ?? 0;
    if (!startNode && count + len >= start) {
      startNode = node;
      startOffset = start - count;
    }
    if (!endNode && count + len >= end) {
      endNode = node;
      endOffset = end - count;
      break;
    }
    count += len;
    node = walker.nextNode();
  }
  if (!startNode || !endNode) return null;
  return { startNode, startOffset, endNode, endOffset };
}

/** Read current selection as raw text if inside root. */
export function readRawSelection(root: HTMLElement): string | null {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  const common = range.commonAncestorContainer;
  const node =
    common.nodeType === Node.TEXT_NODE ? common.parentElement : (common as Element);
  if (!node || !root.contains(node)) return null;
  const text = sel.toString().replace(/\s+/g, " ").trim();
  return text.length >= 1 ? text : null;
}
