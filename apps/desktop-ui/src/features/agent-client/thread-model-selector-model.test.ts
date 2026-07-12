import { describe, expect, it, vi } from "vitest";
import type { ModelInfo, ProviderConfig, ThreadId, WorkspaceId } from "@/lib/schemas";
import { persistThreadModelSelection, selectableThreadModels } from "./thread-model-selector-model";

const capabilities: ModelInfo["capabilities"] = {
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

function provider(id: string, enabled: boolean): ProviderConfig {
  return {
    id: id as ProviderConfig["id"],
    kind: "DeepSeek",
    display_name: id,
    base_url: null,
    api_key_reference: `${id}-key`,
    organization_id: null,
    project_id: null,
    default_headers: {},
    timeout_ms: 30_000,
    retry_policy: { max_retries: 2, initial_backoff_ms: 100, max_backoff_ms: 1_000 },
    fallback_provider_ids: [],
    enabled,
    created_at: "2026-07-11T00:00:00.000Z",
    updated_at: "2026-07-11T00:00:00.000Z",
  };
}

function model(id: string, providerId: string, displayName: string): ModelInfo {
  return {
    id: id as ModelInfo["id"],
    provider_id: providerId as ModelInfo["provider_id"],
    provider_name: providerId,
    model_name: id,
    display_name: displayName,
    capabilities,
  };
}

describe("thread model selector model", () => {
  it("lists registered models from enabled providers in a stable display order", () => {
    const result = selectableThreadModels(
      [model("model-b", "enabled", "Zulu"), model("model-a", "enabled", "Alpha")],
      [provider("enabled", true)],
    );

    expect(result.map((item) => item.id)).toEqual(["model-a", "model-b"]);
  });

  it("excludes models whose provider is disabled or missing", () => {
    const result = selectableThreadModels(
      [model("ready", "enabled", "Ready"), model("off", "disabled", "Off")],
      [provider("enabled", true), provider("disabled", false)],
    );

    expect(result.map((item) => item.id)).toEqual(["ready"]);
  });

  it("persists a model switch without probing the provider", async () => {
    const selected = model("model-1", "provider-1", "Model 1");
    const persist = vi.fn().mockResolvedValue({ model_id: selected.id });

    await persistThreadModelSelection(
      selected.id,
      [selected],
      "workspace-1" as WorkspaceId,
      "thread-1" as ThreadId,
      persist,
    );

    expect(persist).toHaveBeenCalledOnce();
    expect(persist).toHaveBeenCalledWith(
      "Thread",
      "workspace-1",
      "thread-1",
      selected.provider_id,
      selected.id,
    );
  });
});
