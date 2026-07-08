import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { Home } from "lucide-react";
import { cn } from "@/lib/utils";
import { typography } from "@/components/ui/typography";
import { AutomationSummary } from "./automation-summary";
import { NotificationInbox } from "./notification-inbox";
import { BackgroundTaskList } from "./background-task-list";
import { AuditLogPanel } from "./audit-log-panel";
import { useTranslation } from "@/lib/i18n-react";

export type OperationsTab =
  | "automations"
  | "notifications"
  | "background-tasks"
  | "audit";

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
          <p className={typography.pageDescription}>
            {t("operations.description")}
          </p>
        </div>
        <Link
          to="/"
          className="text-primary flex items-center gap-1.5 text-sm hover:underline"
        >
          <Home className="h-4 w-4" />
          {t("common.home")}
        </Link>
      </div>

      <div className="flex gap-2 border-b pb-2">
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

      {activeTab === "automations" && <AutomationSummary />}
      {activeTab === "notifications" && <NotificationInbox />}
      {activeTab === "background-tasks" && <BackgroundTaskList />}
      {activeTab === "audit" && <AuditLogPanel />}
    </main>
  );
}
