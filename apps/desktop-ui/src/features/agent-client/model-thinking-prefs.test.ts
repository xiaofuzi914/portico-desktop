import { describe, expect, it } from "vitest";
import type { ModelInfo, ProviderConfig } from "@/lib/schemas";
import {
  resolveThinkingControl,
  sendOptionsFromControl,
} from "./model-thinking-prefs";

const caps: ModelInfo["capabilities"] = {
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

function provider(id: string, kind: ProviderConfig["kind"]): ProviderConfig {
  return {
    id: id as ProviderConfig["id"],
    kind,
    display_name: kind,
    base_url: null,
    api_key_reference: `${id}-key`,
    organization_id: null,
    project_id: null,
    default_headers: {},
    timeout_ms: 30_000,
    retry_policy: { max_retries: 1, initial_backoff_ms: 100, max_backoff_ms: 500 },
    fallback_provider_ids: [],
    enabled: true,
    created_at: "2026-08-02T00:00:00.000Z",
    updated_at: "2026-08-02T00:00:00.000Z",
  };
}

function model(id: string, providerId: string, name: string, display: string): ModelInfo {
  return {
    id: id as ModelInfo["id"],
    provider_id: providerId as ModelInfo["provider_id"],
    provider_name: providerId,
    model_name: name,
    display_name: display,
    capabilities: caps,
  };
}

describe("model thinking prefs", () => {
  const prefs = { thinkingMode: "auto" as const, reasoningEffort: "medium" as const };

  it("offers deepseek toggle for DeepSeek models", () => {
    const control = resolveThinkingControl(
      model("m1", "ds", "deepseek-v4-pro", "DeepSeek V4 Pro"),
      [provider("ds", "DeepSeek")],
      prefs,
    );
    expect(control.kind).toBe("deepseek_toggle");
    expect(sendOptionsFromControl(control).thinkingMode).toBe("auto");
    expect(sendOptionsFromControl(control).reasoningEffort).toBeNull();
  });

  it("offers reasoning effort for GPT-5 Codex models", () => {
    const control = resolveThinkingControl(
      model("m2", "oai", "gpt-5.6-sol", "GPT-5.6 Sol"),
      [provider("oai", "OpenAI")],
      prefs,
    );
    expect(control.kind).toBe("reasoning_effort");
    expect(control.effortLevels).toEqual(["low", "medium", "high"]);
    expect(sendOptionsFromControl(control).reasoningEffort).toBeDefined();
    expect(sendOptionsFromControl(control).thinkingMode).toBeNull();
  });

  it("hides control for models without thinking affordances", () => {
    const control = resolveThinkingControl(
      model("m3", "kimi", "kimi-k2", "Kimi K2"),
      [provider("kimi", "Moonshot")],
      prefs,
    );
    expect(control.kind).toBe("none");
  });
});
