import type { ModelCapability, ProviderKind } from "@/lib/schemas";

export interface PresetModel {
  modelName: string;
  displayName: string;
  capabilities: ModelCapability;
}

export interface ProviderPreset {
  displayName: string;
  baseUrl: string | null;
  keyReference: string;
  apiKeyRequired: boolean;
  models: PresetModel[];
  /**
   * Official console / API-key page opened for “login to get credentials”.
   * Only set for providers that support the guided login assist flow.
   */
  loginConsoleUrl?: string;
  loginHintKey?: string;
}

const textCapabilities: ModelCapability = {
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

/**
 * Product catalog shown in Capabilities → Models.
 * Order matches the sidebar dropdown.
 */
export const CURATED_PROVIDER_KINDS: ProviderKind[] = [
  "OpenAI",
  "Anthropic",
  "Moonshot",
  "DeepSeek",
  "Xai",
];

/** Providers that offer guided browser login → paste API key. */
export const LOGIN_ASSIST_KINDS: ProviderKind[] = ["OpenAI", "Moonshot", "Xai"];

const PRESETS: Partial<Record<ProviderKind, ProviderPreset>> = {
  OpenAI: {
    displayName: "OpenAI",
    baseUrl: null,
    keyReference: "openai-default",
    apiKeyRequired: true,
    loginConsoleUrl: "https://platform.openai.com/api-keys",
    loginHintKey: "capabilities.loginAssist.openaiHint",
    models: [
      { modelName: "gpt-4.1", displayName: "GPT-4.1", capabilities: textCapabilities },
      {
        modelName: "gpt-4.1-mini",
        displayName: "GPT-4.1 mini",
        capabilities: textCapabilities,
      },
    ],
  },
  Anthropic: {
    displayName: "Anthropic",
    baseUrl: null,
    keyReference: "anthropic-default",
    apiKeyRequired: true,
    models: [
      {
        modelName: "claude-sonnet-4-5",
        displayName: "Claude Sonnet 4.5",
        capabilities: textCapabilities,
      },
    ],
  },
  Moonshot: {
    displayName: "Moonshot (Kimi)",
    // China console keys → api.moonshot.cn; international keys → api.moonshot.ai
    baseUrl: "https://api.moonshot.cn/v1",
    keyReference: "moonshot-default",
    apiKeyRequired: true,
    loginConsoleUrl: "https://platform.moonshot.cn/console/api-keys",
    loginHintKey: "capabilities.loginAssist.moonshotHint",
    models: [
      {
        modelName: "kimi-k2-turbo-preview",
        displayName: "Kimi K2 Turbo",
        capabilities: textCapabilities,
      },
      {
        modelName: "kimi-k2-0711-preview",
        displayName: "Kimi K2",
        capabilities: textCapabilities,
      },
      {
        modelName: "moonshot-v1-128k",
        displayName: "Moonshot v1 128K",
        capabilities: { ...textCapabilities, max_context_tokens: 128_000 },
      },
    ],
  },
  DeepSeek: {
    displayName: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    keyReference: "deepseek-default",
    apiKeyRequired: true,
    models: [
      {
        modelName: "deepseek-v4-pro",
        displayName: "DeepSeek V4 Pro",
        capabilities: {
          ...textCapabilities,
          supports_json_schema: true,
          max_context_tokens: 1_000_000,
        },
      },
      {
        modelName: "deepseek-v4-flash",
        displayName: "DeepSeek V4 Flash",
        capabilities: {
          ...textCapabilities,
          supports_json_schema: true,
          max_context_tokens: 1_000_000,
        },
      },
    ],
  },
  Xai: {
    displayName: "Grok (xAI)",
    baseUrl: "https://api.x.ai/v1",
    keyReference: "xai-default",
    apiKeyRequired: true,
    loginConsoleUrl: "https://console.x.ai/",
    loginHintKey: "capabilities.loginAssist.xaiHint",
    models: [
      {
        modelName: "grok-3",
        displayName: "Grok 3",
        capabilities: {
          ...textCapabilities,
          supports_json_schema: true,
          max_context_tokens: 131_072,
        },
      },
      {
        modelName: "grok-3-mini",
        displayName: "Grok 3 Mini",
        capabilities: {
          ...textCapabilities,
          supports_json_schema: true,
          max_context_tokens: 131_072,
        },
      },
    ],
  },
};

export function providerSetupMode(kind: ProviderKind): "preset" | "custom" {
  return PRESETS[kind] ? "preset" : "custom";
}

export function getProviderPreset(kind: ProviderKind): ProviderPreset | null {
  const preset = PRESETS[kind];
  return preset ? structuredClone(preset) : null;
}

export function supportsLoginAssist(kind: ProviderKind): boolean {
  return LOGIN_ASSIST_KINDS.includes(kind);
}

/** Moonshot/Kimi regional OpenAI-compatible bases (key must match region). */
export const MOONSHOT_ENDPOINTS = [
  {
    id: "cn" as const,
    labelKey: "capabilities.moonshotEndpointCn",
    baseUrl: "https://api.moonshot.cn/v1",
    loginConsoleUrl: "https://platform.moonshot.cn/console/api-keys",
  },
  {
    id: "global" as const,
    labelKey: "capabilities.moonshotEndpointGlobal",
    baseUrl: "https://api.moonshot.ai/v1",
    loginConsoleUrl: "https://platform.moonshot.ai/console/api-keys",
  },
] as const;

export type MoonshotEndpointId = (typeof MOONSHOT_ENDPOINTS)[number]["id"];

export function moonshotEndpointId(baseUrl: string | null | undefined): MoonshotEndpointId {
  const u = (baseUrl ?? "").toLowerCase();
  if (u.includes("moonshot.ai")) return "global";
  return "cn";
}

export function moonshotLoginUrl(baseUrl: string | null | undefined): string {
  const id = moonshotEndpointId(baseUrl);
  return MOONSHOT_ENDPOINTS.find((e) => e.id === id)?.loginConsoleUrl
    ?? MOONSHOT_ENDPOINTS[0].loginConsoleUrl;
}
