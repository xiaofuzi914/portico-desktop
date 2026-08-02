/**
 * Build a progressive session tree for the sidebar from flat Thread rows.
 * Uses parent_thread_id (branch / 划词发散). Orphan parents become roots.
 */

import type { Thread, ThreadId } from "@/lib/schemas";

export type SessionTreeNode = {
  thread: Thread;
  depth: number;
  /** Direct child count (for chevron / badge). */
  childCount: number;
  /** Whether this node is a branch child (has parent in the same list). */
  isBranch: boolean;
};

/**
 * Flatten threads into display order: roots by updated_at desc, children after
 * their parent (children by updated_at desc). Cycles / missing parents → root.
 */
export function buildSessionTreeRows(
  threads: Thread[],
  collapsedIds: ReadonlySet<string> = new Set(),
): SessionTreeNode[] {
  if (threads.length === 0) return [];

  const byId = new Map<string, Thread>();
  for (const t of threads) {
    byId.set(t.id, t);
  }

  const childrenOf = new Map<string, Thread[]>();
  const roots: Thread[] = [];

  for (const t of threads) {
    const parentId = t.parent_thread_id;
    if (parentId && byId.has(parentId) && parentId !== t.id) {
      const list = childrenOf.get(parentId) ?? [];
      list.push(t);
      childrenOf.set(parentId, list);
    } else {
      roots.push(t);
    }
  }

  const sortByUpdatedDesc = (a: Thread, b: Thread) =>
    Date.parse(b.updated_at) - Date.parse(a.updated_at);

  roots.sort(sortByUpdatedDesc);
  for (const kids of childrenOf.values()) {
    kids.sort(sortByUpdatedDesc);
  }

  const rows: SessionTreeNode[] = [];
  const visiting = new Set<string>();

  function walk(thread: Thread, depth: number) {
    if (visiting.has(thread.id)) return;
    visiting.add(thread.id);

    const kids = childrenOf.get(thread.id) ?? [];
    const isBranch = depth > 0;
    rows.push({
      thread,
      depth,
      childCount: kids.length,
      isBranch,
    });

    if (kids.length > 0 && !collapsedIds.has(thread.id)) {
      for (const child of kids) {
        walk(child, depth + 1);
      }
    }
    visiting.delete(thread.id);
  }

  for (const root of roots) {
    walk(root, 0);
  }

  // Only re-attach true orphans (not intentionally hidden by collapse).
  const listed = new Set(rows.map((r) => r.thread.id));
  const hiddenUnderCollapse = new Set<string>();
  for (const parentId of collapsedIds) {
    const stack = [...(childrenOf.get(parentId) ?? [])];
    while (stack.length) {
      const node = stack.pop()!;
      if (hiddenUnderCollapse.has(node.id)) continue;
      hiddenUnderCollapse.add(node.id);
      for (const grand of childrenOf.get(node.id) ?? []) {
        stack.push(grand);
      }
    }
  }

  for (const t of threads) {
    if (listed.has(t.id) || hiddenUnderCollapse.has(t.id)) continue;
    rows.push({
      thread: t,
      depth: 0,
      childCount: childrenOf.get(t.id)?.length ?? 0,
      isBranch: false,
    });
  }

  return rows;
}

/** Auto-expand ancestors of the active thread so it is visible. */
export function expandAncestorsForActive(
  threads: Thread[],
  activeId: ThreadId | undefined,
  collapsed: Set<string>,
): Set<string> {
  if (!activeId) return collapsed;
  const byId = new Map(threads.map((t) => [t.id, t]));
  const next = new Set(collapsed);
  let cur = byId.get(activeId);
  const guard = new Set<string>();
  while (cur?.parent_thread_id && byId.has(cur.parent_thread_id)) {
    if (guard.has(cur.id)) break;
    guard.add(cur.id);
    next.delete(cur.parent_thread_id);
    cur = byId.get(cur.parent_thread_id);
  }
  return next;
}
