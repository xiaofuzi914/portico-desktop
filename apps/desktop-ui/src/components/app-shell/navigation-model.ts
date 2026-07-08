import { Bot, History, Monitor, Puzzle } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export interface NavigationLink {
  to: string;
  labelKey: string;
  icon: LucideIcon;
}

export interface NavigationSection {
  id: "capabilities" | "operations" | "native";
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
        { to: "/plugins", labelKey: "nav.plugins", icon: Puzzle },
        { to: "/skills", labelKey: "nav.skills", icon: Bot },
      ],
    },
    {
      id: "operations",
      labelKey: "nav.operations",
      links: [
        { to: "/automations", labelKey: "nav.automations", icon: History },
        { to: "/audit", labelKey: "nav.audit", icon: History },
      ],
    },
    {
      id: "native",
      labelKey: "nav.nativeTools",
      links: [
        { to: "/browser", labelKey: "nav.browser", icon: Monitor },
        { to: "/desktop", labelKey: "nav.desktop", icon: Monitor },
      ],
    },
  ];
}
