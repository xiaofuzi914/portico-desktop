import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, KeyRound, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Modal } from "@/components/ui/modal";
import {
  createModel,
  createProvider,
  deleteModel,
  deleteProvider,
  deleteProviderSecret,
  getActiveModel,
  getProviderHealth,
  importCliAuthSource,
  listCliAuthSources,
  listModels,
  listProviders,
  setActiveModel,
  setProviderSecret,
  testProviderConnection,
  updateProvider,
  type CliAuthSource,
} from "@/lib/tauri-api";
import {
  asModelId,
  asProviderId,
  type ModelCapability,
  type ModelInfo,
  type ProviderConfig,
  type ProviderId,
  type ProviderKind,
} from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { modelKeys, providerKeys } from "@/lib/query-keys";
import { ErrorAlert } from "@/components/ui/error-alert";
import {
  CURATED_PROVIDER_KINDS,
  getProviderPreset,
  MOONSHOT_ENDPOINTS,
  moonshotEndpointId,
  moonshotLoginUrl,
  providerSetupMode,
  supportsLoginAssist,
} from "./model-provider-presets";
import { cn } from "@/lib/utils";

const PROVIDER_KINDS: ProviderKind[] = CURATED_PROVIDER_KINDS;

function defaultKeyReference(kind: ProviderKind): string {
  return `${kind.toLowerCase()}-${crypto.randomUUID()}`;
}

function providerKindLabel(kind: ProviderKind): string {
  if (kind === "Moonshot") return "Moonshot (Kimi)";
  if (kind === "Xai") return "Grok (xAI)";
  return kind;
}

async function openExternalUrl(url: string): Promise<void> {
  try {
    const { open } = await import("@tauri-apps/plugin-shell");
    await open(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

function resolveLoginConsoleUrl(kind: ProviderKind, baseUrl: string): string | null {
  if (kind === "Moonshot") return moonshotLoginUrl(baseUrl);
  return getProviderPreset(kind)?.loginConsoleUrl ?? null;
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

  /** Import session credentials from local Codex / Kimi / Grok CLI installs. */
  const [cliImportOpen, setCliImportOpen] = useState(false);
  const [cliSources, setCliSources] = useState<CliAuthSource[]>([]);
  const [cliSourcesLoading, setCliSourcesLoading] = useState(false);
  const [cliSourcesError, setCliSourcesError] = useState<string | null>(null);

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
    mutationFn: async (opts?: { apiKey?: string }) => {
      const config = await createProvider(
        providerKind,
        providerName || getProviderPreset(providerKind)?.displayName || providerKind,
        providerBaseUrl || getProviderPreset(providerKind)?.baseUrl || null,
        providerKeyRefName,
      );
      try {
        const key = (opts?.apiKey ?? providerApiKey).trim();
        if (key) {
          await setProviderSecret(providerKeyRefName, key);
        } else if (getProviderPreset(providerKind)?.apiKeyRequired) {
          throw new Error(t("capabilities.apiKeyRequired"));
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
            // Probe after secret is stored; surface InvalidCredentials in the model list.
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
    mutationFn: async ({
      providerId,
      reference,
      key,
    }: {
      providerId: ProviderId;
      reference: string;
      key: string;
    }) => {
      await setProviderSecret(reference, key.trim());
      // Re-probe first model under this provider so health updates immediately.
      const providerModels = await listModels(providerId);
      const first = providerModels[0];
      if (first) {
        return testProviderConnection(providerId, first.id);
      }
      return null;
    },
    onSuccess: (health) => {
      void queryClient.invalidateQueries({ queryKey: providerKeys.list() });
      void queryClient.invalidateQueries({ queryKey: ["provider-health"] });
      if (health) {
        void queryClient.invalidateQueries({
          queryKey: ["provider-health", health.provider_id, health.model_id],
        });
      }
      setEditingKeyProviderId(null);
      setEditingKeyValue("");
    },
  });

  const importCliMutation = useMutation({
    mutationFn: (sourceId: string) => importCliAuthSource(sourceId),
    onSuccess: (config) => {
      void queryClient.invalidateQueries({ queryKey: providerKeys.list() });
      void queryClient.invalidateQueries({ queryKey: modelKeys.list() });
      void queryClient.invalidateQueries({ queryKey: ["active-model"] });
      void queryClient.invalidateQueries({ queryKey: ["provider-health"] });
      setSelectedProviderId(config.id);
      setCliImportOpen(false);
    },
  });

  async function openCliImportDialog() {
    setCliImportOpen(true);
    setCliSourcesError(null);
    setCliSourcesLoading(true);
    try {
      const sources = await listCliAuthSources();
      // Prefer sources matching the currently selected provider kind when possible.
      const sorted = [...sources].sort((a, b) => {
        const aMatch = a.kind === providerKind ? 0 : 1;
        const bMatch = b.kind === providerKind ? 0 : 1;
        if (aMatch !== bMatch) return aMatch - bMatch;
        return Number(b.available) - Number(a.available);
      });
      setCliSources(sorted);
    } catch (err) {
      setCliSources([]);
      setCliSourcesError(err instanceof Error ? err.message : String(err));
    } finally {
      setCliSourcesLoading(false);
    }
  }

  const switchMoonshotEndpointMutation = useMutation({
    mutationFn: async ({
      provider,
      baseUrl,
    }: {
      provider: ProviderConfig;
      baseUrl: string;
    }) => {
      await updateProvider({ ...provider, base_url: baseUrl });
      const providerModels = await listModels(asProviderId(provider.id));
      const first = providerModels[0];
      if (first) {
        return testProviderConnection(asProviderId(provider.id), first.id);
      }
      return null;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: providerKeys.list() });
      void queryClient.invalidateQueries({ queryKey: ["provider-health"] });
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

  /** Persist the global default used when a session has no thread-level model. */
  const setDefaultModelMutation = useMutation({
    mutationFn: ({ providerId, modelId }: { providerId: ProviderId; modelId: ModelInfo["id"] }) =>
      setActiveModel("Global", null, null, providerId, modelId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["active-model"] });
      void queryClient.invalidateQueries({ queryKey: ["active-model", "Global"] });
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
    createProviderMutation.error ??
    deleteProviderMutation.error ??
    updateKeyMutation.error ??
    switchMoonshotEndpointMutation.error ??
    importCliMutation.error;
  const modelMutationError =
    createModelMutation.error ??
    deleteModelMutation.error ??
    setDefaultModelMutation.error ??
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
              createProviderMutation.mutate({});
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
                setProviderApiKey("");
              }}
              required
            >
              {PROVIDER_KINDS.map((kind) => (
                <option key={kind} value={kind}>
                  {providerKindLabel(kind)}
                </option>
              ))}
            </select>
            <Input
              data-testid="provider-api-key"
              type="password"
              placeholder={t("capabilities.apiKey")}
              value={providerApiKey}
              onChange={(e) => setProviderApiKey(e.target.value)}
              required={getProviderPreset(providerKind)?.apiKeyRequired ?? true}
              autoComplete="off"
            />
            <Button
              type="submit"
              data-testid="add-provider"
              disabled={createProviderMutation.isPending}
            >
              {t("capabilities.addAndConfigure")}
            </Button>

            {supportsLoginAssist(providerKind) ? (
              <div className="bg-muted/40 col-span-full flex flex-col gap-2 rounded-lg border px-3 py-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="min-w-0 space-y-0.5">
                  <p className="text-sm font-medium">{t("capabilities.cliAuth.title")}</p>
                  <p className="text-muted-foreground text-xs leading-snug">
                    {t("capabilities.cliAuth.body")}
                  </p>
                </div>
                <Button
                  type="button"
                  variant="default"
                  size="sm"
                  className="shrink-0 gap-1.5"
                  disabled={importCliMutation.isPending}
                  onClick={() => void openCliImportDialog()}
                >
                  <KeyRound className="h-3.5 w-3.5" />
                  {t("capabilities.cliAuth.button")}
                </Button>
              </div>
            ) : null}

            {providerKind === "Moonshot" && (
              <div className="col-span-full flex flex-wrap items-center gap-2">
                <span className="text-muted-foreground text-xs">
                  {t("capabilities.moonshotRegion")}
                </span>
                {MOONSHOT_ENDPOINTS.map((ep) => (
                  <Button
                    key={ep.id}
                    type="button"
                    size="sm"
                    variant={providerBaseUrl === ep.baseUrl ? "default" : "outline"}
                    onClick={() => setProviderBaseUrl(ep.baseUrl)}
                  >
                    {t(ep.labelKey)}
                  </Button>
                ))}
                <span className="text-muted-foreground w-full text-[11px] leading-4">
                  {t("capabilities.moonshotRegionHint")}
                </span>
              </div>
            )}
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
          </form>

          <Modal
            open={cliImportOpen}
            onClose={() => {
              if (importCliMutation.isPending) return;
              setCliImportOpen(false);
            }}
            labelledBy="cli-auth-import-title"
            className="max-w-lg p-5"
          >
            <h2 id="cli-auth-import-title" className="text-base font-semibold">
              {t("capabilities.cliAuth.dialogTitle")}
            </h2>
            <p className="text-muted-foreground mt-2 text-sm leading-relaxed">
              {t("capabilities.cliAuth.dialogBody")}
            </p>
            <ol className="text-muted-foreground mt-3 list-decimal space-y-1.5 pl-5 text-xs leading-relaxed">
              <li>{t("capabilities.cliAuth.stepCodex")}</li>
              <li>{t("capabilities.cliAuth.stepKimi")}</li>
              <li>{t("capabilities.cliAuth.stepGrok")}</li>
            </ol>

            <div className="mt-4 max-h-64 space-y-2 overflow-y-auto">
              {cliSourcesLoading ? (
                <p className="text-muted-foreground flex items-center gap-2 text-sm">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t("capabilities.cliAuth.scanning")}
                </p>
              ) : cliSourcesError ? (
                <ErrorAlert
                  title={t("capabilities.cliAuth.scanFailed")}
                  message={cliSourcesError}
                />
              ) : (
                cliSources.map((src) => (
                  <div
                    key={src.id}
                    className={cn(
                      "flex items-start justify-between gap-3 rounded-lg border px-3 py-2.5",
                      src.available ? "bg-background" : "bg-muted/30 opacity-80",
                    )}
                  >
                    <div className="min-w-0">
                      <p className="text-sm font-medium">{src.label}</p>
                      <p className="text-muted-foreground mt-0.5 truncate font-mono text-[11px]">
                        {src.path}
                      </p>
                      {src.available ? (
                        <p className="text-muted-foreground mt-1 text-[11px]">
                          {t("capabilities.cliAuth.preview")}:{" "}
                          <span className="font-mono">{src.preview}</span>
                          {" · "}
                          {src.auth_mode}
                        </p>
                      ) : (
                        <p className="text-muted-foreground mt-1 text-[11px] leading-snug">
                          {src.hint ?? t("capabilities.cliAuth.unavailable")}
                        </p>
                      )}
                    </div>
                    <Button
                      type="button"
                      size="sm"
                      className="shrink-0"
                      disabled={!src.available || importCliMutation.isPending}
                      onClick={() => importCliMutation.mutate(src.id)}
                    >
                      {importCliMutation.isPending ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        t("capabilities.cliAuth.import")
                      )}
                    </Button>
                  </div>
                ))
              )}
            </div>

            <div className="mt-4 flex flex-wrap items-center justify-between gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="gap-1.5"
                disabled={cliSourcesLoading}
                onClick={() => void openCliImportDialog()}
              >
                {t("common.refresh")}
              </Button>
              <div className="flex gap-2">
                {resolveLoginConsoleUrl(providerKind, providerBaseUrl) ? (
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="gap-1.5"
                    onClick={() => {
                      const url = resolveLoginConsoleUrl(providerKind, providerBaseUrl);
                      if (url) void openExternalUrl(url);
                    }}
                  >
                    <ExternalLink className="h-3.5 w-3.5" />
                    {t("capabilities.cliAuth.openCliDocs")}
                  </Button>
                ) : null}
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setCliImportOpen(false)}
                  disabled={importCliMutation.isPending}
                >
                  {t("common.close")}
                </Button>
              </div>
            </div>
          </Modal>

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
                      <span className={provider.enabled ? "text-success" : "text-warning-ink"}>
                        {provider.enabled ? t("common.enabled") : t("common.disabled")}
                      </span>
                    </span>
                    {provider.kind === "Moonshot" ? (
                      <span className="mt-1 flex flex-wrap gap-1">
                        {MOONSHOT_ENDPOINTS.map((ep) => (
                          <Button
                            key={ep.id}
                            type="button"
                            size="sm"
                            variant={
                              moonshotEndpointId(provider.base_url) === ep.id
                                ? "default"
                                : "outline"
                            }
                            className="h-6 px-2 text-[10px]"
                            disabled={switchMoonshotEndpointMutation.isPending}
                            onClick={(e) => {
                              e.stopPropagation();
                              switchMoonshotEndpointMutation.mutate({
                                provider,
                                baseUrl: ep.baseUrl,
                              });
                            }}
                          >
                            {t(ep.labelKey)}
                          </Button>
                        ))}
                      </span>
                    ) : null}
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
                          autoComplete="off"
                        />
                        <Button
                          size="sm"
                          onClick={() =>
                            updateKeyMutation.mutate({
                              providerId: asProviderId(provider.id),
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
                    onSetDefault={() =>
                      setDefaultModelMutation.mutate({
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
                      setDefaultModelMutation.isPending ||
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
  onSetDefault,
  onTest,
  onDelete,
  busy,
}: {
  model: ModelInfo;
  /** Global default model for sessions that have no thread/workspace override. */
  active: boolean;
  onSetDefault: () => void;
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
          <span className="bg-primary/15 text-primary ml-2 rounded px-1.5 py-0.5 text-xs font-medium">
            {t("capabilities.defaultModelBadge")}
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
            <span
              className={cn(
                "max-w-xl",
                health.status === "Ready" ? "text-success" : "text-warning",
              )}
              title={health.message ?? health.status}
            >
              {t("capabilities.health")}: {health.status}
              {health.message ? ` — ${health.message}` : null}
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
          variant={active ? "outline" : "default"}
          onClick={onSetDefault}
          disabled={busy || active}
          title={t("capabilities.setDefaultModelHint")}
        >
          {active
            ? t("capabilities.defaultModelBadge")
            : t("capabilities.setDefaultModel")}
        </Button>
        <Button variant="destructive" size="sm" onClick={onDelete} disabled={busy || active}>
          {t("operations.delete")}
        </Button>
      </div>
    </li>
  );
}
