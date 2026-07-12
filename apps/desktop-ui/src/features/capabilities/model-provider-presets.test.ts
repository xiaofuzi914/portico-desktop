import { describe, expect, it } from "vitest";
import { getProviderPreset, providerSetupMode } from "./model-provider-presets";

describe("model provider presets", () => {
  it("preconfigures DeepSeek so only an API key is needed", () => {
    const preset = getProviderPreset("DeepSeek");

    expect(preset).toMatchObject({
      displayName: "DeepSeek",
      baseUrl: "https://api.deepseek.com",
      keyReference: "deepseek-default",
      apiKeyRequired: true,
    });
    expect(preset?.models.map((model) => model.modelName)).toEqual([
      "deepseek-v4-pro",
      "deepseek-v4-flash",
    ]);
    expect(preset?.models[0]?.capabilities).toMatchObject({
      supports_streaming: true,
      supports_tools: true,
      supports_json_schema: true,
      max_context_tokens: 1_000_000,
    });
  });

  it("returns fresh preset objects", () => {
    const first = getProviderPreset("DeepSeek");
    if (!first) throw new Error("missing DeepSeek preset");
    first.models[0]!.displayName = "Changed";

    expect(getProviderPreset("DeepSeek")?.models[0]?.displayName).toBe("DeepSeek V4 Pro");
  });

  it("keeps custom providers in advanced mode and Ollama keyless", () => {
    expect(providerSetupMode("Custom")).toBe("custom");
    expect(getProviderPreset("Custom")).toBeNull();
    expect(getProviderPreset("Ollama")?.apiKeyRequired).toBe(false);
  });
});
