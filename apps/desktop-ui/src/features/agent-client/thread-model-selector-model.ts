import type {
  ActiveModelSelection,
  ModelId,
  ModelInfo,
  ModelSelectionScope,
  ProviderConfig,
  ProviderId,
  ProviderKind,
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

/** Short label for the picker: model name only (no provider suffix). */
export function modelPickerLabel(model: Pick<ModelInfo, "display_name" | "model_name">): string {
  const name = model.display_name.trim() || model.model_name.trim();
  return name || "Model";
}

/** Resolve provider kind for a model (for icons). */
export function providerKindForModel(
  model: Pick<ModelInfo, "provider_id">,
  providers: readonly ProviderConfig[],
): ProviderKind | null {
  return providers.find((p) => p.id === model.provider_id)?.kind ?? null;
}

/** Hover / a11y detail — not shown as primary label. */
export function modelPickerDetail(
  model: Pick<ModelInfo, "display_name" | "provider_name" | "model_name">,
): string {
  const label = modelPickerLabel(model);
  const provider = model.provider_name?.trim();
  if (provider && !label.includes(provider)) {
    return `${label} · ${provider}`;
  }
  return label;
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
