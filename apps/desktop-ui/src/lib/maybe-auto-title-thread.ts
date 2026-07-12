import type { QueryClient } from "@tanstack/react-query";
import { getThread, updateThreadTitle } from "@/lib/tauri-api";
import type { ThreadId, WorkspaceId } from "@/lib/schemas";
import { workspaceKeys } from "@/lib/query-keys";
import { deriveThreadTitle, isDefaultThreadTitle } from "@/lib/thread-title";

/**
 * If the session still has a placeholder title, rename it from the first user prompt.
 * Never overwrites a title the user already customized.
 */
export async function maybeAutoTitleThread(
  queryClient: QueryClient,
  workspaceId: WorkspaceId,
  threadId: ThreadId,
  firstUserMessage: string,
): Promise<void> {
  const topic = deriveThreadTitle(firstUserMessage);
  if (!topic) return;

  try {
    const thread = await getThread(threadId);
    if (!isDefaultThreadTitle(thread.title)) return;
    await updateThreadTitle(threadId, topic);
    await queryClient.invalidateQueries({ queryKey: workspaceKeys.threads(workspaceId) });
  } catch {
    // Title is non-critical — never block the conversation turn.
  }
}
