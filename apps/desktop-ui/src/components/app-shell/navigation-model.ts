import { Activity, Bot, Brain, History, Puzzle, Server, Sparkles } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export interface NavigationLink {
  to: string;
  labelKey: string;
  icon: LucideIcon;
}

export interface NavigationSection {
  id: "capabilities" | "operations";
  labelKey: string;
  /** Section header icon — same weight as 项目 / 会话. */
  icon: LucideIcon;
  links: NavigationLink[];
}

export function buildNavigationSections(): NavigationSection[] {
  return [
    {
      id: "capabilities",
      labelKey: "nav.capabilities",
      icon: Sparkles,
      links: [
        { to: "/models", labelKey: "nav.models", icon: Bot },
        { to: "/memory", labelKey: "nav.memory", icon: Brain },
        { to: "/plugins", labelKey: "nav.plugins", icon: Puzzle },
        { to: "/mcp", labelKey: "nav.mcp", icon: Server },
      ],
    },
    {
      id: "operations",
      labelKey: "nav.operations",
      icon: Activity,
      links: [{ to: "/audit", labelKey: "nav.audit", icon: History }],
    },
  ];
}
