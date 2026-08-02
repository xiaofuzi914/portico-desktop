/**
 * Shared sidebar type scale + row density.
 *
 * Section headers (项目 / 会话 / 能力 / 运行) are slightly larger and heavier.
 * Leaf rows (projects, threads, nav links) stay compact and secondary.
 */

/** Group title: 项目、会话、能力、运行 */
export const SIDEBAR_SECTION_TITLE_CLASS =
  "text-foreground/90 flex h-8 items-center gap-2 text-[13px] font-semibold tracking-tight";

/** Leaf row idle state: AgentTeams, 模型, 审计, 设置… */
export const SIDEBAR_LEAF_CLASS =
  "text-muted-foreground hover:bg-sidebar-accent hover:text-foreground flex h-7 items-center gap-2 rounded-md px-2 text-xs transition-colors";

/** Leaf row active / selected */
export const SIDEBAR_LEAF_ACTIVE_CLASS =
  "flex h-7 items-center gap-2 rounded-md px-2 text-xs bg-sidebar-accent font-medium text-foreground";
