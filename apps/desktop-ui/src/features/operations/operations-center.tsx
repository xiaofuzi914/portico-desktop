import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { Home } from "lucide-react";
import { TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { typography } from "@/components/ui/typography";
import { AutomationSummary } from "./automation-summary";
import { NotificationInbox } from "./notification-inbox";
import { BackgroundTaskList } from "./background-task-list";
import { AuditLogPanel } from "./audit-log-panel";
import { useTranslation } from "@/lib/i18n-react";

export type OperationsTab = "automations" | "notifications" | "background-tasks" | "audit";

interface OperationsCenterProps {
  defaultTab?: OperationsTab;
}

export function OperationsCenter({ defaultTab = "automations" }: OperationsCenterProps) {
  const [activeTab, setActiveTab] = useState<OperationsTab>(defaultTab);
  const { t } = useTranslation();

  const tabs: { id: OperationsTab; label: string }[] = [
    { id: "automations", label: t("operations.automations") },
    { id: "notifications", label: t("notifications.title") },
    { id: "background-tasks", label: t("operations.backgroundTasks") },
    { id: "audit", label: t("operations.audit") },
  ];

  return (
    <main className="container mx-auto max-w-5xl space-y-6 p-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className={typography.pageTitle}>{t("operations.title")}</h1>
          <p className={typography.pageDescription}>{t("operations.description")}</p>
        </div>
        <Link to="/" className="text-primary flex items-center gap-1.5 text-sm hover:underline">
          <Home className="h-4 w-4" />
          {t("common.home")}
        </Link>
      </div>

      <TabsList aria-label={t("operations.title")} className="flex gap-2 border-b pb-2">
        {tabs.map((tab) => (
          <TabsTrigger
            key={tab.id}
            id={`operations-tab-${tab.id}`}
            aria-controls={`operations-panel-${tab.id}`}
            active={activeTab === tab.id}
            onClick={() => setActiveTab(tab.id)}
          >
            {tab.label}
          </TabsTrigger>
        ))}
      </TabsList>

      <TabsContent
        id={`operations-panel-${activeTab}`}
        aria-labelledby={`operations-tab-${activeTab}`}
      >
        {activeTab === "automations" && <AutomationSummary />}
        {activeTab === "notifications" && <NotificationInbox />}
        {activeTab === "background-tasks" && <BackgroundTaskList />}
        {activeTab === "audit" && <AuditLogPanel />}
      </TabsContent>
    </main>
  );
}
