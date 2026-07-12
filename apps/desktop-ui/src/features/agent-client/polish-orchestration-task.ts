import type { PatternHint } from "@/lib/schemas";

export interface PolishedOrchestrationTask {
  /** Refined multi-agent brief ready to run. */
  polished: string;
  /** Short bullets explaining what changed. */
  adjustments: string[];
  /** Suggested roles derived from habits + signals. */
  suggestedRoles: string[];
}

/** Keep multi-agent lean: more roles ⇒ more timeouts on remote models. */
const MAX_ROLES = 2;

/**
 * Turn a casual user question into a clearer multi-agent task brief.
 * Prefer one strong role; never invent a large cast.
 */
export function polishOrchestrationTask(
  rawInput: string,
  patterns: PatternHint[] = [],
): PolishedOrchestrationTask {
  const raw = rawInput.trim().replace(/\s+/g, " ");
  if (!raw) {
    return { polished: "", adjustments: [], suggestedRoles: [] };
  }

  const lower = raw.toLowerCase();
  const adjustments: string[] = [];
  const goals: string[] = [];
  const roles: string[] = [];

  const pushRole = (role: string) => {
    if (!roles.includes(role) && roles.length < MAX_ROLES) roles.push(role);
  };

  // Habit priors first (capped).
  if (patterns[0]?.preferred_roles?.length) {
    for (const role of patterns[0].preferred_roles) {
      pushRole(role);
    }
    adjustments.push(`参考习惯「${patterns[0].name}」`);
  }

  const has = (words: string[]) => words.some((w) => lower.includes(w) || raw.includes(w));

  if (has(["安全", "审计", "漏洞", "security", "audit", "cve"])) {
    goals.push("从安全视角检查明显风险并给出优先级");
    pushRole("explorer");
    pushRole("security-reviewer");
  } else if (has(["review", "评审", "代码审查", "code review"])) {
    goals.push("做一次可读性/正确性向的代码评审摘要");
    pushRole("explorer");
    pushRole("reviewer");
  } else if (has(["实现", "修改", "编写", "fix", "implement", "write", "加功能"])) {
    goals.push("先读懂现状，再给出实现思路与改动范围");
    pushRole("explorer");
    pushRole("worker");
  } else if (has(["计划", "拆解", "方案", "plan", "roadmap", "怎么做"])) {
    goals.push("给出可执行的分步计划");
    pushRole("planner");
  } else if (has(["测试", "test", "单元测试", "回归"])) {
    goals.push("指出应覆盖的测试点与现有测试入口");
    pushRole("tester");
  } else if (has(["文档", "doc", "readme", "说明"])) {
    goals.push("整理文档缺口与建议补充内容");
    pushRole("doc-writer");
  } else if (
    has([
      "目录",
      "结构",
      "有哪些",
      "list",
      "explore",
      "文件",
      "folder",
      "项目",
      "架构",
      "代码库",
      "仓库",
      "看下",
      "看看",
    ])
  ) {
    goals.push("梳理项目结构、关键入口与技术架构要点");
    pushRole("explorer");
  } else {
    goals.push(`围绕问题给出结构化结论：${raw}`);
    // One general agent — multi-agent tax is not worth it for vague asks.
    pushRole("default");
    adjustments.push("问题较笼统，使用单个通用 Agent（更稳）");
  }

  if (goals.length && !adjustments.some((a) => a.includes("笼统"))) {
    adjustments.push("已压缩为可执行目标（最多 2 个角色）");
  }

  // Keep the brief short so subagents don't burn the context window.
  const polished = [
    `【任务】${raw}`,
    `【目标】${goals[0] ?? raw}`,
    `【角色】${roles.join(" → ") || "default"}`,
    "【要求】中文结论先行；标注依据路径；不确定则说明。",
  ].join("\n");

  return {
    polished,
    adjustments,
    suggestedRoles: roles,
  };
}

/**
 * True when a task is likely to benefit from multi-role collaboration in real use
 * (more than one role, or a single specialist beyond plain "default").
 */
export function shouldSuggestMultiRole(result: PolishedOrchestrationTask): boolean {
  if (result.suggestedRoles.length >= 2) return true;
  const only = result.suggestedRoles[0];
  return Boolean(only && only !== "default");
}
