import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listModels, listProviders, resolveActiveModel, setActiveModel } from "@/lib/tauri-api";
import { modelKeys, providerKeys } from "@/lib/query-keys";
import type { ModelId, ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { persistThreadModelSelection, selectableThreadModels } from "./thread-model-selector-model";

interface ThreadModelSelectorProps {
  workspaceId: WorkspaceId;
  threadId: ThreadId;
}

export function ThreadModelSelector({ workspaceId, threadId }: ThreadModelSelectorProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const resolvedKey = ["active-model", "resolved", workspaceId, threadId] as const;
  const { data: providers = [], isLoading: providersLoading } = useQuery({
    queryKey: providerKeys.list(),
    queryFn: listProviders,
  });
  const { data: models = [], isLoading: modelsLoading } = useQuery({
    queryKey: modelKeys.list(),
    queryFn: () => listModels(),
  });
  const { data: activeModel } = useQuery({
    queryKey: resolvedKey,
    queryFn: () => resolveActiveModel(workspaceId, threadId),
    retry: false,
  });
  const selectableModels = selectableThreadModels(models, providers);
  const selectMutation = useMutation({
    mutationFn: async (modelId: ModelId) => {
      try {
        return await persistThreadModelSelection(
          modelId,
          selectableModels,
          workspaceId,
          threadId,
          setActiveModel,
        );
      } catch (error) {
        if (error instanceof Error && error.message === "MODEL_UNAVAILABLE") {
          throw new Error(t("agent.modelUnavailable"));
        }
        throw error;
      }
    },
    onSuccess: (selection) => {
      queryClient.setQueryData(resolvedKey, selection);
      void queryClient.invalidateQueries({ queryKey: ["active-model"] });
    },
  });

  if (providersLoading || modelsLoading) {
    return <span className="text-muted-foreground">{t("common.loading")}</span>;
  }

  if (selectableModels.length === 0) {
    return (
      <Link to="/models" className="text-foreground truncate hover:underline">
        {providers.length > 0 ? t("agent.noRegisteredModels") : t("agent.modelNotConfigured")}
      </Link>
    );
  }

  return (
    <span className="flex min-w-0 items-center gap-1">
      <select
        aria-label={t("agent.selectModel")}
        className="text-foreground max-w-48 cursor-pointer truncate bg-transparent outline-none"
        value={activeModel?.model_id ?? ""}
        disabled={selectMutation.isPending}
        onChange={(event) => selectMutation.mutate(event.target.value as ModelId)}
      >
        {!activeModel && <option value="">{t("agent.selectModel")}</option>}
        {selectableModels.map((model) => (
          <option key={model.id} value={model.id}>
            {model.provider_name} / {model.display_name}
          </option>
        ))}
      </select>
      {selectMutation.error && (
        <span
          role="alert"
          className="text-destructive cursor-help font-semibold"
          title={selectMutation.error.message}
        >
          !
        </span>
      )}
    </span>
  );
}
