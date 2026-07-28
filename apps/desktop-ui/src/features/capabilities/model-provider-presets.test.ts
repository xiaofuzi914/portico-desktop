import { describe, expect, it } from "vitest";
import {
  CURATED_PROVIDER_KINDS,
  getProviderPreset,
  LOGIN_ASSIST_KINDS,
  moonshotEndpointId,
  moonshotLoginUrl,
  MOONSHOT_ENDPOINTS,
  providerSetupMode,
  supportsLoginAssist,
} from "./model-provider-presets";

describe("model provider presets", () => {
  it("curates the product provider list", () => {
    expect(CURATED_PROVIDER_KINDS).toEqual([
      "OpenAI",
      "Anthropic",
      "Moonshot",
      "DeepSeek",
      "Xai",
    ]);
  });

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

  it("adds Grok (xAI) with OpenAI-compatible endpoint", () => {
    const preset = getProviderPreset("Xai");
    expect(preset).toMatchObject({
      displayName: "Grok (xAI)",
      baseUrl: "https://api.x.ai/v1",
      apiKeyRequired: true,
      loginConsoleUrl: "https://console.x.ai/",
    });
    expect(preset?.models.some((m) => m.modelName.startsWith("grok"))).toBe(true);
    expect(supportsLoginAssist("Xai")).toBe(true);
  });

  it("returns fresh preset objects", () => {
    const first = getProviderPreset("DeepSeek");
    if (!first) throw new Error("missing DeepSeek preset");
    first.models[0]!.displayName = "Changed";

    expect(getProviderPreset("DeepSeek")?.models[0]?.displayName).toBe("DeepSeek V4 Pro");
  });

  it("keeps non-curated kinds without presets", () => {
    expect(providerSetupMode("Custom")).toBe("custom");
    expect(getProviderPreset("Custom")).toBeNull();
    expect(getProviderPreset("Ollama")).toBeNull();
    expect(getProviderPreset("Groq")).toBeNull();
  });

  it("exposes Moonshot CN/global endpoints and login assist", () => {
    const preset = getProviderPreset("Moonshot");
    expect(preset?.baseUrl).toBe("https://api.moonshot.cn/v1");
    expect(preset?.models.some((m) => m.modelName.includes("kimi"))).toBe(true);
    expect(MOONSHOT_ENDPOINTS).toHaveLength(2);
    expect(moonshotEndpointId("https://api.moonshot.ai/v1")).toBe("global");
    expect(moonshotEndpointId("https://api.moonshot.cn/v1")).toBe("cn");
    expect(moonshotLoginUrl("https://api.moonshot.ai/v1")).toContain("moonshot.ai");
    expect(LOGIN_ASSIST_KINDS).toEqual(["OpenAI", "Moonshot", "Xai"]);
    expect(supportsLoginAssist("OpenAI")).toBe(true);
    expect(supportsLoginAssist("DeepSeek")).toBe(false);
  });
});
