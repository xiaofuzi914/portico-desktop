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

const PRESETS: Partial<Record<ProviderKind, ProviderPreset>> = {
  OpenAI: {
    displayName: "OpenAI",
    baseUrl: null,
    keyReference: "openai-default",
    apiKeyRequired: true,
    models: [{ modelName: "gpt-4.1", displayName: "GPT-4.1", capabilities: textCapabilities }],
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
    baseUrl: "https://api.moonshot.cn/v1",
    keyReference: "moonshot-default",
    apiKeyRequired: true,
    models: [
      { modelName: "kimi-k2-0711-preview", displayName: "Kimi K2", capabilities: textCapabilities },
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
  Groq: {
    displayName: "Groq",
    baseUrl: "https://api.groq.com/openai/v1",
    keyReference: "groq-default",
    apiKeyRequired: true,
    models: [
      { modelName: "llama3-70b-8192", displayName: "Llama 3 70B", capabilities: textCapabilities },
    ],
  },
  OpenRouter: {
    displayName: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    keyReference: "openrouter-default",
    apiKeyRequired: true,
    models: [
      {
        modelName: "openai/gpt-4o-mini",
        displayName: "GPT-4o mini",
        capabilities: textCapabilities,
      },
    ],
  },
  Ollama: {
    displayName: "Ollama",
    baseUrl: "http://localhost:11434/v1",
    keyReference: "ollama-default",
    apiKeyRequired: false,
    models: [{ modelName: "llama3", displayName: "Llama 3", capabilities: textCapabilities }],
  },
};

export function providerSetupMode(kind: ProviderKind): "preset" | "custom" {
  return PRESETS[kind] ? "preset" : "custom";
}

export function getProviderPreset(kind: ProviderKind): ProviderPreset | null {
  const preset = PRESETS[kind];
  return preset ? structuredClone(preset) : null;
}
