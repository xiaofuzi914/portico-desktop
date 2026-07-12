import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MessageSquare, Pencil, Plus, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button } from "@/components/ui/button";
import {
  createThread,
  deleteThread,
  listThreads,
  updateThreadTitle,
} from "@/lib/tauri-api";
import type { ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { workspaceKeys } from "@/lib/query-keys";
import {
  SIDEBAR_THREAD_ACTION_CLASS,
  SIDEBAR_THREAD_ACTIVE_CLASS,
  SIDEBAR_THREAD_LINK_CLASS,
} from "./sidebar-thread-styles";
import { cn } from "@/lib/utils";

interface SidebarThreadsProps {
  workspaceId: WorkspaceId;
}

type ContextMenuState = Readonly<{
  threadId: ThreadId;
  title: string;
  x: number;
  y: number;
}>;

export function SidebarThreads({ workspaceId }: SidebarThreadsProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const params = useParams({ strict: false }) as { threadId?: string };
  const activeThreadId = params.threadId as ThreadId | undefined;

  const editInputRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [editingId, setEditingId] = useState<ThreadId | null>(null);
  const [draft, setDraft] = useState("");
  const [menu, setMenu] = useState<ContextMenuState | null>(null);

  const { data: threads, isLoading } = useQuery({
    queryKey: workspaceKeys.threads(workspaceId),
    queryFn: () => listThreads(workspaceId),
  });

  const create = useMutation({
    mutationFn: () => createThread(workspaceId, t("thread.defaultTitle")),
    onSuccess: (thread) => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      void navigate({
        to: "/workspaces/$workspaceId/threads/$threadId",
        params: { workspaceId, threadId: thread.id },
      });
    },
  });

  const rename = useMutation({
    mutationFn: ({ id, title }: { id: ThreadId; title: string }) => updateThreadTitle(id, title),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      setEditingId(null);
      setDraft("");
    },
  });

  const remove = useMutation({
    mutationFn: (id: ThreadId) => deleteThread(workspaceId, id),
    onSuccess: async (_data, deletedId) => {
      queryClient.removeQueries({ queryKey: ["messages", deletedId] });
      queryClient.removeQueries({ queryKey: ["runs", deletedId] });
      await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
      setMenu(null);
      if (activeThreadId === deletedId) {
        const remaining = (threads ?? []).filter((thread) => thread.id !== deletedId);
        if (remaining[0]) {
          void navigate({
            to: "/workspaces/$workspaceId/threads/$threadId",
            params: { workspaceId, threadId: remaining[0].id },
          });
        } else {
          void navigate({
            to: "/workspaces/$workspaceId",
            params: { workspaceId },
          });
        }
      }
    },
  });

  useEffect(() => {
    if (!editingId) return;
    const el = editInputRef.current;
    if (!el) return;
    el.focus();
    el.select();
  }, [editingId]);

  useEffect(() => {
    if (!menu) return;
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (menuRef.current?.contains(target)) return;
      setMenu(null);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenu(null);
    };
    const onScroll = () => setMenu(null);
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [menu]);

  function beginEdit(id: ThreadId, title: string) {
    setMenu(null);
    setEditingId(id);
    setDraft(title);
  }

  function cancelEdit() {
    setEditingId(null);
    setDraft("");
  }

  function commitEdit(id: ThreadId, original: string) {
    const next = draft.trim();
    if (!next || next === original.trim()) {
      cancelEdit();
      return;
    }
    rename.mutate({ id, title: next });
  }

  function openContextMenu(
    event: React.MouseEvent,
    threadId: ThreadId,
    title: string,
  ) {
    event.preventDefault();
    event.stopPropagation();
    // Keep menu inside viewport
    const pad = 8;
    const menuW = 160;
    const menuH = 88;
    const x = Math.min(event.clientX, window.innerWidth - menuW - pad);
    const y = Math.min(event.clientY, window.innerHeight - menuH - pad);
    setMenu({ threadId, title, x: Math.max(pad, x), y: Math.max(pad, y) });
  }

  function confirmDelete(threadId: ThreadId, title: string) {
    const ok = window.confirm(
      t("thread.deleteConfirmNamed").replace("{title}", title) ||
        t("thread.deleteConfirm"),
    );
    if (!ok) {
      setMenu(null);
      return;
    }
    remove.mutate(threadId);
  }

  return (
    <div className="space-y-1" onContextMenu={(event) => event.preventDefault()}>
      <Button
        variant="ghost"
        size="sm"
        className={SIDEBAR_THREAD_ACTION_CLASS}
        onClick={() => create.mutate()}
        disabled={create.isPending}
      >
        <Plus className="mr-1.5 h-3.5 w-3.5" />
        {t("sidebar.newThread")}
      </Button>

      {isLoading ? (
        <p className="text-muted-foreground px-2 text-xs">{t("sidebar.loadingThreads")}</p>
      ) : threads?.length ? (
        <ul className="space-y-0.5">
          {threads.map((thread) => (
            <li key={thread.id}>
              {editingId === thread.id ? (
                <div className={cn(SIDEBAR_THREAD_LINK_CLASS, "cursor-text gap-1.5")}>
                  <MessageSquare className="h-3.5 w-3.5 shrink-0" />
                  <input
                    ref={editInputRef}
                    value={draft}
                    maxLength={80}
                    disabled={rename.isPending}
                    aria-label={t("thread.editTitle")}
                    className="border-input bg-background text-foreground h-6 min-w-0 flex-1 rounded border px-1 text-xs outline-none focus-visible:ring-ring focus-visible:ring-1"
                    onChange={(event) => setDraft(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        commitEdit(thread.id, thread.title);
                      }
                      if (event.key === "Escape") {
                        event.preventDefault();
                        cancelEdit();
                      }
                    }}
                    onBlur={() => commitEdit(thread.id, thread.title)}
                    onClick={(event) => event.stopPropagation()}
                    onContextMenu={(event) => event.preventDefault()}
                  />
                </div>
              ) : (
                <Link
                  to="/workspaces/$workspaceId/threads/$threadId"
                  params={{ workspaceId, threadId: thread.id }}
                  className={SIDEBAR_THREAD_LINK_CLASS}
                  activeProps={{
                    className: SIDEBAR_THREAD_ACTIVE_CLASS,
                  }}
                  title={t("thread.editTitleHint")}
                  onContextMenu={(event) => openContextMenu(event, thread.id, thread.title)}
                  onDoubleClick={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    beginEdit(thread.id, thread.title);
                  }}
                >
                  <MessageSquare className="h-3.5 w-3.5 shrink-0" />
                  <span className="min-w-0 flex-1 truncate">{thread.title}</span>
                </Link>
              )}
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-muted-foreground px-2 text-xs">{t("sidebar.noThreads")}</p>
      )}

      {menu
        ? createPortal(
            <div
              ref={menuRef}
              role="menu"
              aria-label={t("thread.contextMenu")}
              className="bg-background text-foreground border-border fixed z-[100] min-w-[10rem] rounded-md border py-1 shadow-lg"
              style={{ left: menu.x, top: menu.y }}
            >
              <button
                type="button"
                role="menuitem"
                className="hover:bg-muted flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs"
                onClick={() => beginEdit(menu.threadId, menu.title)}
              >
                <Pencil className="h-3.5 w-3.5" />
                {t("thread.rename")}
              </button>
              <button
                type="button"
                role="menuitem"
                className="text-destructive hover:bg-destructive/10 flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs"
                disabled={remove.isPending}
                onClick={() => confirmDelete(menu.threadId, menu.title)}
              >
                <Trash2 className="h-3.5 w-3.5" />
                {t("thread.delete")}
              </button>
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}
