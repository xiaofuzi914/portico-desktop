import type {
  ActiveModelSelection,
  ModelId,
  ModelInfo,
  ModelSelectionScope,
  ProviderConfig,
  ProviderId,
  ThreadId,
  WorkspaceId,
} from "@/lib/schemas";

export function selectableThreadModels(
  models: readonly ModelInfo[],
  providers: readonly ProviderConfig[],
): ModelInfo[] {
  const enabledProviderIds = new Set(
    providers.filter((provider) => provider.enabled).map((provider) => provider.id),
  );

  return [...models]
    .filter((model) => enabledProviderIds.has(model.provider_id))
    .sort((left, right) =>
      `${left.provider_name}\u0000${left.display_name}`.localeCompare(
        `${right.provider_name}\u0000${right.display_name}`,
      ),
    );
}

type PersistModelSelection = (
  scope: ModelSelectionScope,
  workspaceId: WorkspaceId | null,
  threadId: ThreadId | null,
  providerId: ProviderId,
  modelId: ModelId,
) => Promise<ActiveModelSelection>;

export function persistThreadModelSelection(
  modelId: ModelId,
  selectableModels: readonly ModelInfo[],
  workspaceId: WorkspaceId,
  threadId: ThreadId,
  persist: PersistModelSelection,
): Promise<ActiveModelSelection> {
  const model = selectableModels.find((candidate) => candidate.id === modelId);
  if (!model) return Promise.reject(new Error("MODEL_UNAVAILABLE"));
  return persist("Thread", workspaceId, threadId, model.provider_id, model.id);
}
