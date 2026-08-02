import type { ModelInfo, ProviderConfig, ProviderKind, ReasoningEffort, ThinkingMode } from "@/lib/schemas";

export type ThinkingControlKind = "none" | "deepseek_toggle" | "reasoning_effort";

export type ThinkingControlState = {
  kind: ThinkingControlKind;
  /** DeepSeek on/off/auto */
  thinkingMode: ThinkingMode;
  /** Codex / GPT-5 effort */
  reasoningEffort: ReasoningEffort;
  /** Available effort levels for the current model */
  effortLevels: ReasoningEffort[];
};

const STORAGE_KEY = "portico.modelThinkingPrefs.v1";

type StoredPrefs = {
  thinkingMode?: ThinkingMode;
  reasoningEffort?: ReasoningEffort;
};

function readStored(): StoredPrefs {
  if (typeof window === "undefined") return {};
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as StoredPrefs;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeStored(prefs: StoredPrefs) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    /* ignore */
  }
}

export function loadThinkingPrefs(): Pick<ThinkingControlState, "thinkingMode" | "reasoningEffort"> {
  const stored = readStored();
  return {
    thinkingMode: stored.thinkingMode === "off" || stored.thinkingMode === "on" || stored.thinkingMode === "auto"
      ? stored.thinkingMode
      : "auto",
    reasoningEffort:
      stored.reasoningEffort === "low" ||
      stored.reasoningEffort === "medium" ||
      stored.reasoningEffort === "high"
        ? stored.reasoningEffort
        : "medium",
  };
}

export function saveThinkingMode(mode: ThinkingMode) {
  writeStored({ ...readStored(), thinkingMode: mode });
}

export function saveReasoningEffort(effort: ReasoningEffort) {
  writeStored({ ...readStored(), reasoningEffort: effort });
}

function providerKindFor(
  model: ModelInfo | null | undefined,
  providers: readonly ProviderConfig[],
): ProviderKind | null {
  if (!model) return null;
  return providers.find((p) => p.id === model.provider_id)?.kind ?? null;
}

/**
 * Which control to show for the active model.
 * - DeepSeek → thinking on/off/auto
 * - OpenAI / Codex GPT-5 family → reasoning effort
 */
export function resolveThinkingControl(
  model: ModelInfo | null | undefined,
  providers: readonly ProviderConfig[],
  prefs: Pick<ThinkingControlState, "thinkingMode" | "reasoningEffort">,
): ThinkingControlState {
  const kind = providerKindFor(model, providers);
  const name = (model?.model_name ?? "").toLowerCase();
  const display = (model?.display_name ?? "").toLowerCase();
  const hay = `${name} ${display}`;

  if (kind === "DeepSeek") {
    return {
      kind: "deepseek_toggle",
      thinkingMode: prefs.thinkingMode,
      reasoningEffort: prefs.reasoningEffort,
      effortLevels: [],
    };
  }

  // OpenAI / Codex ChatGPT session models (gpt-5*, o-series, etc.)
  const looksReasoningCapable =
    kind === "OpenAI" &&
    (hay.includes("gpt-5") ||
      hay.includes("o1") ||
      hay.includes("o3") ||
      hay.includes("o4") ||
      hay.includes("codex") ||
      hay.includes("sol") ||
      hay.includes("terra") ||
      hay.includes("luna"));

  if (looksReasoningCapable) {
    // Sol defaults lower; others medium (pref still wins if set).
    let effort = prefs.reasoningEffort;
    if (!readStored().reasoningEffort) {
      effort = hay.includes("sol") ? "low" : "medium";
    }
    return {
      kind: "reasoning_effort",
      thinkingMode: prefs.thinkingMode,
      reasoningEffort: effort,
      effortLevels: ["low", "medium", "high"],
    };
  }

  return {
    kind: "none",
    thinkingMode: prefs.thinkingMode,
    reasoningEffort: prefs.reasoningEffort,
    effortLevels: [],
  };
}

/** Payload for sendMessage options. */
export function sendOptionsFromControl(control: ThinkingControlState): {
  thinkingMode?: ThinkingMode | null;
  reasoningEffort?: ReasoningEffort | null;
} {
  if (control.kind === "deepseek_toggle") {
    return { thinkingMode: control.thinkingMode, reasoningEffort: null };
  }
  if (control.kind === "reasoning_effort") {
    return { thinkingMode: null, reasoningEffort: control.reasoningEffort };
  }
  return { thinkingMode: null, reasoningEffort: null };
}
