import { Box, ClipboardList, FileText, Folder, Globe, Monitor, Terminal } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { useTranslation } from "@/lib/i18n-react";
import { inspectorTabs, type InspectorTab } from "./inspector-state";

const TAB_META: Record<InspectorTab, { labelKey: string; icon: LucideIcon }> = {
  context: { labelKey: "inspector.context", icon: FileText },
  files: { labelKey: "inspector.files", icon: Folder },
  terminal: { labelKey: "inspector.terminal", icon: Terminal },
  browser: { labelKey: "inspector.browser", icon: Globe },
  desktop: { labelKey: "inspector.desktop", icon: Monitor },
  artifacts: { labelKey: "inspector.artifacts", icon: Box },
  audit: { labelKey: "inspector.audit", icon: ClipboardList },
};

interface InspectorTabsProps {
  activeTab: InspectorTab;
  onChange: (tab: InspectorTab) => void;
}

export function InspectorTabs({ activeTab, onChange }: InspectorTabsProps) {
  const { t } = useTranslation();

  return (
    <div className="flex shrink-0 overflow-x-auto border-b bg-background/70 px-2 py-2">
      {inspectorTabs.map((tab) => {
        const meta = TAB_META[tab];
        const Icon = meta.icon;
        const isActive = activeTab === tab;
        const label = t(meta.labelKey);
        return (
          <button
            key={tab}
            type="button"
            onClick={() => onChange(tab)}
            className={cn(
              "text-muted-foreground hover:bg-muted hover:text-foreground flex h-8 min-w-14 shrink-0 items-center justify-center gap-1 rounded-md px-2 text-[11px] transition-colors",
              isActive && "bg-muted text-foreground shadow-xs",
            )}
            aria-label={label}
            aria-pressed={isActive}
          >
            <Icon className="h-3.5 w-3.5 shrink-0" />
            <span className="truncate">{label}</span>
          </button>
        );
      })}
    </div>
  );
}
