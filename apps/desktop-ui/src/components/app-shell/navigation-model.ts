import { Bot, Brain, History, Puzzle, Server } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export interface NavigationLink {
  to: string;
  labelKey: string;
  icon: LucideIcon;
}

export interface NavigationSection {
  id: "capabilities" | "operations";
  labelKey: string;
  links: NavigationLink[];
}

export function buildNavigationSections(): NavigationSection[] {
  return [
    {
      id: "capabilities",
      labelKey: "nav.capabilities",
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
      links: [{ to: "/audit", labelKey: "nav.audit", icon: History }],
    },
  ];
}
