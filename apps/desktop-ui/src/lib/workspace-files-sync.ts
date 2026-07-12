import type { QueryClient } from "@tanstack/react-query";
import {
  runtimeEventStore,
  shouldRefreshWorkspaceFiles,
} from "@/lib/tauri-events";
import type { RuntimeEvent } from "@/lib/schemas";

/**
 * Keep the inspector folder listing in sync when agents write/update files.
 *
 * Invalidates every `["workspace-files", …]` query (all workspaces / paths)
 * so the currently open folder refetches automatically.
 */
export function attachWorkspaceFilesSync(queryClient: QueryClient): () => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending = false;

  const flush = () => {
    timer = null;
    if (!pending) return;
    pending = false;
    void queryClient.invalidateQueries({ queryKey: ["workspace-files"] });
  };

  const schedule = () => {
    pending = true;
    if (timer != null) return;
    // Coalesce bursts of fs_write during a single agent turn.
    timer = setTimeout(flush, 250);
  };

  const onEvent = (event?: RuntimeEvent) => {
    if (!event) return;
    if (shouldRefreshWorkspaceFiles(event)) {
      schedule();
    }
  };

  const unsubscribe = runtimeEventStore.subscribe(onEvent);

  return () => {
    unsubscribe();
    if (timer != null) {
      clearTimeout(timer);
      timer = null;
    }
  };
}
