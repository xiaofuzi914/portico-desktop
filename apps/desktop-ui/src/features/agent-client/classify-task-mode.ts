/**
 * Auto-pick how to run a user message: single agent (default) or a multi-agent recipe.
 * Pure rules — no LLM call — so Send stays instant and predictable.
 */

import type { PatternHint } from "@/lib/schemas";
import { polishOrchestrationTask, shouldSuggestMultiRole } from "./polish-orchestration-task";

export type AutoRunMode =
  | { kind: "single"; labelKey: "orchestration.auto.single" }
  | {
      kind: "multi";
      /** null = adaptive stage graph; string = catalog key */
      workflowId: string | null;
      labelKey: string;
    };

export type ClassifiedTask = {
  mode: AutoRunMode;
  /** Brief used for multi-agent starts (polished); raw text for single. */
  taskText: string;
  /** Short human reason for the hint line. */
  reasonKey: string;
  confidence: "high" | "medium" | "low";
};

function hasAny(text: string, words: string[]): boolean {
  const lower = text.toLowerCase();
  return words.some((w) => lower.includes(w.toLowerCase()) || text.includes(w));
}

/**
 * Classify a composer message into single-agent vs multi-agent recipe.
 * Prefers single-agent for short / casual asks; multi only when signals are clear.
 */
export function classifyTaskMode(
  rawInput: string,
  patterns: PatternHint[] = [],
  multiAgentReady = true,
): ClassifiedTask {
  const raw = rawInput.trim();
  if (!raw) {
    return {
      mode: { kind: "single", labelKey: "orchestration.auto.single" },
      taskText: "",
      reasonKey: "orchestration.auto.reason.empty",
      confidence: "low",
    };
  }

  if (!multiAgentReady) {
    return {
      mode: { kind: "single", labelKey: "orchestration.auto.single" },
      taskText: raw,
      reasonKey: "orchestration.auto.reason.singleOnly",
      confidence: "high",
    };
  }

  // Very short chat → always single (greetings, yes/no, one-liners).
  // CJK often has no spaces — do not treat a long Chinese sentence as "3 words".
  const wordCount = raw.split(/\s+/).filter(Boolean).length;
  const charCount = [...raw].length; // code points
  const looksCjk = /[\u4e00-\u9fff]/.test(raw);
  const isShort = looksCjk
    ? charCount < 8
    : charCount < 12 || (wordCount <= 3 && charCount < 24);
  if (isShort) {
    return {
      mode: { kind: "single", labelKey: "orchestration.auto.single" },
      taskText: raw,
      reasonKey: "orchestration.auto.reason.short",
      confidence: "high",
    };
  }

  // Explicit multi-angle / review recipes.
  if (
    hasAny(raw, [
      "多镜头",
      "多角度",
      "multi-lens",
      "lens review",
      "code review",
      "代码审查",
      "评审",
      "审查",
      "审计",
      "挑错",
      "风险排查",
      "security review",
    ])
  ) {
    return {
      mode: {
        kind: "multi",
        workflowId: "multi-lens-review",
        labelKey: "orchestration.wf.multi-lens-review.title",
      },
      taskText: polishOrchestrationTask(raw, patterns).polished || raw,
      reasonKey: "orchestration.auto.reason.review",
      confidence: "high",
    };
  }

  // Deep research / map the system.
  if (
    hasAny(raw, [
      "深入摸清",
      "深度研究",
      "deep research",
      "调研",
      "搞清结构",
      "架构分析",
      "全面分析",
      "摸清",
      "来龙去脉",
      "证据",
      "open questions",
      "investigate",
    ]) &&
    charCount >= 16
  ) {
    return {
      mode: {
        kind: "multi",
        workflowId: "deep-research",
        labelKey: "orchestration.wf.deep-research.title",
      },
      taskText: polishOrchestrationTask(raw, patterns).polished || raw,
      reasonKey: "orchestration.auto.reason.research",
      confidence: "high",
    };
  }

  // Iterative polish / refine deliverable.
  if (
    hasAny(raw, [
      "反复打磨",
      "打磨",
      "改到能交差",
      "polish",
      "refine",
      "iterate",
      "迭代改",
      "多轮修改",
      "改到过关",
      "完善草稿",
    ])
  ) {
    return {
      mode: {
        kind: "multi",
        workflowId: "iterative-refine",
        labelKey: "orchestration.wf.iterative-refine.title",
      },
      taskText: polishOrchestrationTask(raw, patterns).polished || raw,
      reasonKey: "orchestration.auto.reason.refine",
      confidence: "high",
    };
  }

  // Multi-step delivery without a specific recipe → adaptive graph.
  const polished = polishOrchestrationTask(raw, patterns);
  const multiStep =
    hasAny(raw, [
      "并且",
      "然后",
      "同时",
      "分步",
      "步骤",
      "先…再",
      "以及",
      "and then",
      "step by step",
      "实现并",
      "分析并",
      "梳理并",
    ]) ||
    (shouldSuggestMultiRole(polished) && charCount >= 40);

  if (multiStep && polished.suggestedRoles.length >= 2) {
    return {
      mode: {
        kind: "multi",
        workflowId: null,
        labelKey: "orchestration.adaptiveGraph",
      },
      taskText: polished.polished || raw,
      reasonKey: "orchestration.auto.reason.adaptive",
      confidence: "medium",
    };
  }

  // Default: single agent chat (main product path).
  return {
    mode: { kind: "single", labelKey: "orchestration.auto.single" },
    taskText: raw,
    reasonKey: "orchestration.auto.reason.defaultSingle",
    confidence: patterns.length > 0 ? "medium" : "high",
  };
}

/** Display label resolution helper for tests / UI. */
export function classifiedModeWorkflowId(mode: AutoRunMode): string | null | undefined {
  if (mode.kind === "single") return undefined;
  return mode.workflowId;
}
