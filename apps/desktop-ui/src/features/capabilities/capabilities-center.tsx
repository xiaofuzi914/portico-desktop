import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { Home } from "lucide-react";
import { cn } from "@/lib/utils";
import { typography } from "@/components/ui/typography";
import { ModelCapabilitiesPanel } from "./model-capabilities-panel";
import { MemoryCapabilitiesPanel } from "./memory-capabilities-panel";
import { PluginCapabilitiesPanel } from "./plugin-capabilities-panel";
import { McpCapabilitiesPanel } from "./mcp-capabilities-panel";
import { useTranslation } from "@/lib/i18n-react";

export type CapabilitiesTab = "models" | "memory" | "plugins" | "mcp";

interface CapabilitiesCenterProps {
  defaultTab?: CapabilitiesTab;
}

export const CAPABILITIES_CENTER_CLASS =
  "container mx-auto h-full max-w-5xl space-y-6 overflow-y-auto p-6";

export function CapabilitiesCenter({ defaultTab = "models" }: CapabilitiesCenterProps) {
  const [activeTab, setActiveTab] = useState<CapabilitiesTab>(defaultTab);
  const { t } = useTranslation();

  const tabs: { id: CapabilitiesTab; label: string }[] = [
    { id: "models", label: t("capabilities.models") },
    { id: "memory", label: t("capabilities.memory") },
    { id: "plugins", label: t("capabilities.plugins") },
    { id: "mcp", label: t("capabilities.mcp") },
  ];

  return (
    <main className={CAPABILITIES_CENTER_CLASS}>
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className={typography.pageTitle}>{t("capabilities.title")}</h1>
          <p className={typography.pageDescription}>{t("capabilities.description")}</p>
        </div>
        <Link to="/" className="text-primary flex items-center gap-1.5 text-sm hover:underline">
          <Home className="h-4 w-4" />
          {t("common.home")}
        </Link>
      </div>

      <div className="flex flex-wrap gap-2 border-b pb-2">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => setActiveTab(tab.id)}
            className={cn(
              "rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
              activeTab === tab.id
                ? "bg-muted text-foreground"
                : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
            )}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {activeTab === "models" && <ModelCapabilitiesPanel />}
      {activeTab === "memory" && <MemoryCapabilitiesPanel />}
      {activeTab === "plugins" && <PluginCapabilitiesPanel />}
      {activeTab === "mcp" && <McpCapabilitiesPanel />}
    </main>
  );
}
