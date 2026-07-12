import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  createModel,
  createProvider,
  deleteModel,
  deleteProvider,
  deleteProviderSecret,
  getActiveModel,
  getProviderHealth,
  listModels,
  listProviders,
  setActiveModel,
  setProviderSecret,
  testProviderConnection,
} from "@/lib/tauri-api";
import {
  asModelId,
  asProviderId,
  providerKindSchema,
  type ModelCapability,
  type ModelInfo,
  type ProviderId,
  type ProviderKind,
} from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { modelKeys, providerKeys } from "@/lib/query-keys";
import { ErrorAlert } from "@/components/ui/error-alert";
import { getProviderPreset, providerSetupMode } from "./model-provider-presets";

const PROVIDER_KINDS: ProviderKind[] = [...providerKindSchema.options];

function defaultKeyReference(kind: ProviderKind): string {
  return `${kind.toLowerCase()}-${crypto.randomUUID()}`;
}

function providerKindLabel(kind: ProviderKind, notRunnable: string): string {
  if (kind === "Moonshot") return "Moonshot (Kimi)";
  if (kind === "Google" || kind === "AzureOpenAI") return `${kind} (${notRunnable})`;
  return kind;
}

const defaultCapabilities: ModelCapability = {
  supports_streaming: true,
  supports_tools: true,
  supports_json_schema: false,
  supports_vision: false,
  supports_pdf: false,
  supports_system_prompt: true,
  supports_embeddings: false,
  max_context_tokens: null,
  input_price_per_1k: null,
  output_price_per_1k: null,
};

export function ModelCapabilitiesPanel() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();

  const [selectedProviderId, setSelectedProviderId] = useState<ProviderId | null>(null);

  const initialPreset = getProviderPreset("DeepSeek");
  const [providerKind, setProviderKind] = useState<ProviderKind>("DeepSeek");
  const [providerName, setProviderName] = useState(initialPreset?.displayName ?? "");
  const [providerBaseUrl, setProviderBaseUrl] = useState(initialPreset?.baseUrl ?? "");
  const [providerKeyRefName, setProviderKeyRefName] = useState(defaultKeyReference("DeepSeek"));
  const [providerApiKey, setProviderApiKey] = useState("");
  const [showAdvancedProvider, setShowAdvancedProvider] = useState(false);

  const [editingKeyProviderId, setEditingKeyProviderId] = useState<ProviderId | null>(null);
  const [editingKeyValue, setEditingKeyValue] = useState("");

  const [modelName, setModelName] = useState("");
  const [modelDisplayName, setModelDisplayName] = useState("");
  const [capabilities, setCapabilities] = useState<ModelCapability>(defaultCapabilities);

  const { data: providers, isLoading: providersLoading } = useQuery({
    queryKey: providerKeys.list(),
    queryFn: listProviders,
  });

  const { data: models, isLoading: modelsLoading } = useQuery({
    queryKey: modelKeys.list(selectedProviderId),
    queryFn: () => listModels(selectedProviderId ?? undefined),
    enabled: selectedProviderId !== null,
  });

  const { data: activeModel } = useQuery({
    queryKey: ["active-model", "Global"],
    queryFn: () => getActiveModel("Global"),
  });

  const createProviderMutation = useMutation({
    mutationFn: async () => {
      const config = await createProvider(
        providerKind,
        providerName,
        providerBaseUrl || null,
        providerKeyRefName,
      );
      try {
        if (providerApiKey.trim()) {
          await setProviderSecret(providerKeyRefName, providerApiKey.trim());
        }
        const preset = getProviderPreset(providerKind);
        if (preset) {
          let defaultModel: ModelInfo | null = null;
          for (const model of preset.models) {
            const createdModel = await createModel(
              config.id,
              model.modelName,
              model.displayName,
              model.capabilities,
            );
            defaultModel ??= createdModel;
          }
          if (defaultModel) {
            await testProviderConnection(config.id, defaultModel.id);
            await setActiveModel("Global", null, null, config.id, defaultModel.id);
          }
        }
      } catch (error) {
        await Promise.allSettled([
          deleteProvider(config.id),
          deleteProviderSecret(providerKeyRefName),
        ]);
        throw error;
      }
      return config;
    },
    onSuccess: (config) => {
      void queryClient.invalidateQueries({ queryKey: providerKeys.list() });
      void queryClient.invalidateQueries({ queryKey: modelKeys.list() });
      void queryClient.invalidateQueries({ queryKey: ["active-model"] });
      setSelectedProviderId(config.id);
      setProviderApiKey("");
      setProviderKeyRefName(defaultKeyReference(providerKind));
    },
  });

  const updateKeyMutation = useMutation({
    mutationFn: ({ reference, key }: { reference: string; key: string }) =>
      setProviderSecret(reference, key),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: providerKeys.list() });
      setEditingKeyProviderId(null);
      setEditingKeyValue("");
    },
  });

  const deleteProviderMutation = useMutation({
    mutationFn: (id: ProviderId) => deleteProvider(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: providerKeys.list() });
      void queryClient.invalidateQueries({ queryKey: modelKeys.list() });
      setSelectedProviderId(null);
    },
  });

  const createModelMutation = useMutation({
    mutationFn: () => {
      if (!selectedProviderId) {
        throw new Error("No provider selected");
      }
      return createModel(selectedProviderId, modelName, modelDisplayName, capabilities);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: modelKeys.list() });
      setModelName("");
      setModelDisplayName("");
      setCapabilities(defaultCapabilities);
    },
  });

  const deleteModelMutation = useMutation({
    mutationFn: (id: string) => deleteModel(asModelId(id)),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: modelKeys.list() });
    },
  });

  const setActiveModelMutation = useMutation({
    mutationFn: ({ providerId, modelId }: { providerId: ProviderId; modelId: ModelInfo["id"] }) =>
      setActiveModel("Global", null, null, providerId, modelId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["active-model"] });
    },
  });

  const testConnectionMutation = useMutation({
    mutationFn: ({ providerId, modelId }: { providerId: ProviderId; modelId: ModelInfo["id"] }) =>
      testProviderConnection(providerId, modelId),
    onSuccess: (health) => {
      void queryClient.invalidateQueries({
        queryKey: ["provider-health", health.provider_id, health.model_id],
      });
    },
  });

  const selectedProvider = providers?.find((p) => p.id === selectedProviderId);

  const providerMutationError =
    createProviderMutation.error ?? deleteProviderMutation.error ?? updateKeyMutation.error;
  const modelMutationError =
    createModelMutation.error ??
    deleteModelMutation.error ??
    setActiveModelMutation.error ??
    testConnectionMutation.error;

  const updateCapability = <K extends keyof ModelCapability>(key: K, value: ModelCapability[K]) => {
    setCapabilities((prev) => ({ ...prev, [key]: value }));
  };

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>{t("capabilities.modelProviders")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <form
            data-testid="provider-form"
            className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3"
            onSubmit={(e) => {
              e.preventDefault();
              createProviderMutation.mutate();
            }}
          >
            <select
              data-testid="provider-kind"
              className="border-input bg-background h-9 rounded-md border px-3 text-sm"
              value={providerKind}
              onChange={(e) => {
                const kind = e.target.value as ProviderKind;
                const preset = getProviderPreset(kind);
                setProviderKind(kind);
                setProviderName(preset?.displayName ?? "");
                setProviderBaseUrl(preset?.baseUrl ?? "");
                setProviderKeyRefName(defaultKeyReference(kind));
                setShowAdvancedProvider(providerSetupMode(kind) === "custom");
              }}
              required
            >
              {PROVIDER_KINDS.map((kind) => (
                <option key={kind} value={kind}>
                  {providerKindLabel(kind, t("capabilities.notRunnable"))}
                </option>
              ))}
            </select>
            <Input
              data-testid="provider-api-key"
              type="password"
              placeholder={
                providerKind === "Ollama"
                  ? t("capabilities.apiKeyOptionalOllama")
                  : t("capabilities.apiKey")
              }
              value={providerApiKey}
              onChange={(e) => setProviderApiKey(e.target.value)}
              required={getProviderPreset(providerKind)?.apiKeyRequired ?? true}
            />
            <Button
              type="submit"
              data-testid="add-provider"
              disabled={createProviderMutation.isPending}
            >
              {t("capabilities.addAndConfigure")}
            </Button>
            <div className="col-span-full">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setShowAdvancedProvider((visible) => !visible)}
              >
                {showAdvancedProvider
                  ? t("capabilities.hideAdvanced")
                  : t("capabilities.advancedSettings")}
              </Button>
            </div>
            {showAdvancedProvider && (
              <div className="col-span-full grid gap-3 sm:grid-cols-3">
                <Input
                  data-testid="provider-name"
                  placeholder={t("capabilities.displayName")}
                  value={providerName}
                  onChange={(e) => setProviderName(e.target.value)}
                  required
                />
                <Input
                  data-testid="provider-base-url"
                  placeholder={t("capabilities.baseUrlOptional")}
                  value={providerBaseUrl}
                  onChange={(e) => setProviderBaseUrl(e.target.value)}
                />
                <Input
                  data-testid="provider-key-reference"
                  placeholder={t("capabilities.apiKeyReferenceName")}
                  value={providerKeyRefName}
                  onChange={(e) => setProviderKeyRefName(e.target.value)}
                  required
                />
              </div>
            )}
            {getProviderPreset(providerKind) && (
              <p className="text-muted-foreground col-span-full text-xs">
                {t("capabilities.presetHint")}
              </p>
            )}
            {providerKind === "Ollama" && (
              <p className="text-muted-foreground col-span-full text-xs">
                {t("capabilities.ollamaKeyHint")}
              </p>
            )}
          </form>

          {providerMutationError && (
            <ErrorAlert
              title={t("capabilities.providerMutationFailed")}
              message={
                providerMutationError instanceof Error
                  ? providerMutationError.message
                  : String(providerMutationError)
              }
            />
          )}

          {providersLoading ? (
            <p className="text-muted-foreground">{t("capabilities.loadingProviders")}</p>
          ) : providers?.length ? (
            <ul className="divide-y" data-testid="provider-list">
              {providers.map((provider) => (
                <li
                  key={provider.id}
                  className={`flex items-center justify-between py-3 ${
                    selectedProviderId === provider.id ? "bg-muted/50" : ""
                  }`}
                >
                  <button
                    type="button"
                    className="flex flex-1 flex-col px-2 text-left hover:underline"
                    onClick={() => setSelectedProviderId(asProviderId(provider.id))}
                  >
                    <span className="font-medium">
                      {provider.display_name}{" "}
                      <span className="text-muted-foreground text-sm font-normal">
                        ({provider.kind})
                      </span>
                    </span>
                    <span className="text-muted-foreground text-xs">
                      {provider.base_url ?? t("capabilities.defaultEndpoint")} ·{" "}
                      <span className={provider.enabled ? "text-green-600" : "text-amber-600"}>
                        {provider.enabled ? t("common.enabled") : t("common.disabled")}
                      </span>
                    </span>
                  </button>
                  <div className="flex items-center gap-2">
                    {editingKeyProviderId === asProviderId(provider.id) ? (
                      <>
                        <Input
                          type="password"
                          className="h-8 w-40 sm:w-48"
                          placeholder={t("capabilities.newApiKey")}
                          value={editingKeyValue}
                          onChange={(e) => setEditingKeyValue(e.target.value)}
                        />
                        <Button
                          size="sm"
                          onClick={() =>
                            updateKeyMutation.mutate({
                              reference: provider.api_key_reference,
                              key: editingKeyValue,
                            })
                          }
                          disabled={updateKeyMutation.isPending || !editingKeyValue.trim()}
                        >
                          {t("common.save")}
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => {
                            setEditingKeyProviderId(null);
                            setEditingKeyValue("");
                          }}
                        >
                          {t("common.cancel")}
                        </Button>
                      </>
                    ) : (
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => {
                          setEditingKeyProviderId(asProviderId(provider.id));
                          setEditingKeyValue("");
                        }}
                      >
                        {t("capabilities.updateKey")}
                      </Button>
                    )}
                    <Button
                      variant="destructive"
                      size="sm"
                      onClick={() => deleteProviderMutation.mutate(asProviderId(provider.id))}
                      disabled={deleteProviderMutation.isPending}
                    >
                      {t("operations.delete")}
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-muted-foreground">{t("capabilities.noProviders")}</p>
          )}
        </CardContent>
      </Card>

      {selectedProvider && (
        <Card>
          <CardHeader>
            <CardTitle>
              {t("capabilities.modelsFor")} {selectedProvider.display_name}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <form
              className="grid gap-3"
              onSubmit={(e) => {
                e.preventDefault();
                createModelMutation.mutate();
              }}
            >
              <div className="grid gap-3 sm:grid-cols-3">
                <Input
                  placeholder={
                    selectedProvider?.kind === "Moonshot"
                      ? t("capabilities.moonshotModelName")
                      : t("capabilities.modelName")
                  }
                  value={modelName}
                  onChange={(e) => setModelName(e.target.value)}
                  required
                />
                <Input
                  placeholder={t("capabilities.displayName")}
                  value={modelDisplayName}
                  onChange={(e) => setModelDisplayName(e.target.value)}
                  required
                />
                <Button type="submit" disabled={createModelMutation.isPending}>
                  {t("capabilities.addModel")}
                </Button>
              </div>

              <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                {(
                  [
                    ["supports_streaming", t("capability.streaming")],
                    ["supports_tools", t("capability.tools")],
                    ["supports_json_schema", t("capability.jsonSchema")],
                    ["supports_vision", t("capability.vision")],
                    ["supports_pdf", t("capability.pdf")],
                    ["supports_system_prompt", t("capability.systemPrompt")],
                    ["supports_embeddings", t("capability.embeddings")],
                  ] as [keyof ModelCapability, string][]
                ).map(([key, label]) => (
                  <label key={key} className="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={Boolean(capabilities[key])}
                      onChange={(e) => updateCapability(key, e.target.checked)}
                      className="h-4 w-4"
                    />
                    {label}
                  </label>
                ))}
              </div>

              <div className="grid gap-3 sm:grid-cols-3">
                <Input
                  placeholder={t("capabilities.maxContextTokens")}
                  type="number"
                  value={capabilities.max_context_tokens ?? ""}
                  onChange={(e) => {
                    const value = e.target.value === "" ? null : Number(e.target.value);
                    updateCapability("max_context_tokens", value);
                  }}
                />
                <Input
                  placeholder={t("capabilities.inputPrice")}
                  type="number"
                  step="0.0001"
                  value={capabilities.input_price_per_1k ?? ""}
                  onChange={(e) => {
                    const value = e.target.value === "" ? null : Number(e.target.value);
                    updateCapability("input_price_per_1k", value);
                  }}
                />
                <Input
                  placeholder={t("capabilities.outputPrice")}
                  type="number"
                  step="0.0001"
                  value={capabilities.output_price_per_1k ?? ""}
                  onChange={(e) => {
                    const value = e.target.value === "" ? null : Number(e.target.value);
                    updateCapability("output_price_per_1k", value);
                  }}
                />
              </div>
            </form>

            {modelMutationError && (
              <ErrorAlert
                title={t("capabilities.modelMutationFailed")}
                message={
                  modelMutationError instanceof Error
                    ? modelMutationError.message
                    : String(modelMutationError)
                }
              />
            )}

            {modelsLoading ? (
              <p className="text-muted-foreground">{t("capabilities.loadingModels")}</p>
            ) : models?.length ? (
              <ul className="divide-y">
                {models.map((model) => (
                  <ModelListItem
                    key={model.id}
                    model={model}
                    active={activeModel?.model_id === model.id}
                    onSetActive={() =>
                      setActiveModelMutation.mutate({
                        providerId: model.provider_id,
                        modelId: model.id,
                      })
                    }
                    onTest={() =>
                      testConnectionMutation.mutate({
                        providerId: model.provider_id,
                        modelId: model.id,
                      })
                    }
                    onDelete={() => deleteModelMutation.mutate(model.id)}
                    busy={
                      deleteModelMutation.isPending ||
                      setActiveModelMutation.isPending ||
                      testConnectionMutation.isPending
                    }
                  />
                ))}
              </ul>
            ) : (
              <p className="text-muted-foreground">{t("capabilities.noModels")}</p>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function ModelListItem({
  model,
  active,
  onSetActive,
  onTest,
  onDelete,
  busy,
}: {
  model: ModelInfo;
  active: boolean;
  onSetActive: () => void;
  onTest: () => void;
  onDelete: () => void;
  busy: boolean;
}) {
  const { t } = useTranslation();
  const { data: health } = useQuery({
    queryKey: ["provider-health", model.provider_id, model.id],
    queryFn: () => getProviderHealth(model.provider_id, model.id),
  });

  return (
    <li className="flex items-center justify-between gap-3 py-3">
      <div className="min-w-0">
        <span className="font-medium">{model.display_name}</span>
        <span className="text-muted-foreground ml-2 text-sm">{model.model_name}</span>
        {active && (
          <span className="bg-success/15 text-success ml-2 rounded px-1.5 py-0.5 text-xs">
            {t("capabilities.activeModel")}
          </span>
        )}
        <div className="text-muted-foreground mt-1 flex flex-wrap gap-2 text-xs">
          {model.capabilities.supports_tools && <span>{t("capability.tools")}</span>}
          {model.capabilities.supports_streaming && <span>{t("capability.streaming")}</span>}
          {model.capabilities.supports_vision && <span>{t("capability.vision")}</span>}
          {model.capabilities.supports_json_schema && <span>{t("capability.jsonSchema")}</span>}
          {model.capabilities.supports_embeddings && <span>{t("capability.embeddings")}</span>}
          {model.capabilities.max_context_tokens !== null && (
            <span>{model.capabilities.max_context_tokens.toLocaleString()} tokens</span>
          )}
          {health && (
            <span className={health.status === "Ready" ? "text-success" : "text-warning"}>
              {t("capabilities.health")}: {health.status}
            </span>
          )}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <Button size="sm" variant="outline" onClick={onTest} disabled={busy}>
          {t("capabilities.testConnection")}
        </Button>
        <Button
          size="sm"
          onClick={onSetActive}
          disabled={busy || active || health?.status !== "Ready"}
        >
          {t("capabilities.useModel")}
        </Button>
        <Button variant="destructive" size="sm" onClick={onDelete} disabled={busy || active}>
          {t("operations.delete")}
        </Button>
      </div>
    </li>
  );
}
