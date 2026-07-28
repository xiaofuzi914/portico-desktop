import type { ReactNode } from "react";
import { ClipboardList, Folder, History } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useTranslation } from "@/lib/i18n-react";
import { inspectorTabs, type InspectorTab } from "./inspector-state";

const TAB_META: Record<InspectorTab, { labelKey: string; icon: LucideIcon }> = {
  timeline: { labelKey: "inspector.timeline", icon: History },
  files: { labelKey: "inspector.files", icon: Folder },
  audit: { labelKey: "inspector.audit", icon: ClipboardList },
};

interface InspectorTabsProps {
  activeTab: InspectorTab;
  onChange: (tab: InspectorTab) => void;
  /** Optional control rendered after the tab buttons (e.g. collapse on the right). */
  trailing?: ReactNode;
}

export function InspectorTabs({ activeTab, onChange, trailing }: InspectorTabsProps) {
  const { t } = useTranslation();

  return (
    <div className="bg-background/70 flex h-10 shrink-0 items-center gap-1 overflow-x-auto border-b px-2">
      <TabsList
        aria-label={t("inspector.title")}
        className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto"
      >
        {inspectorTabs.map((tab) => {
          const meta = TAB_META[tab];
          const Icon = meta.icon;
          const label = t(meta.labelKey);
          return (
            <TabsTrigger
              key={tab}
              variant="compact"
              id={`inspector-tab-${tab}`}
              aria-controls={`inspector-panel-${tab}`}
              active={activeTab === tab}
              onClick={() => onChange(tab)}
            >
              <Icon className="h-3.5 w-3.5 shrink-0" />
              <span className="truncate">{label}</span>
            </TabsTrigger>
          );
        })}
      </TabsList>
      {trailing ? <div className="ml-auto flex shrink-0 items-center">{trailing}</div> : null}
    </div>
  );
}
