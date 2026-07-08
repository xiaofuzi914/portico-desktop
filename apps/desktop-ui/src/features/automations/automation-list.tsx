import { Button } from "@/components/ui/button";
import type { Automation } from "@/lib/schemas";
import { formatRelativeTime } from "@/lib/formatters";
import { useTranslation } from "@/lib/i18n-react";

interface AutomationListProps {
  automations: Automation[];
  onEdit: (automation: Automation) => void;
  onDelete: (automation: Automation) => void;
  onRun: (automation: Automation) => void;
  isPending?: boolean;
}

export function AutomationList({
  automations,
  onEdit,
  onDelete,
  onRun,
  isPending,
}: AutomationListProps) {
  const { t } = useTranslation();

  return (
    <ul className="divide-y">
      {automations.map((automation) => (
        <li key={automation.id} className="py-4">
          <div className="flex items-start justify-between gap-4">
            <div className="flex-1">
              <div className="flex items-center gap-2">
                <span className="font-medium">{automation.name}</span>
                {automation.enabled ? (
                  <span className="text-xs text-green-600">{t("common.enabled")}</span>
                ) : (
                  <span className="text-xs text-amber-600">{t("common.disabled")}</span>
                )}
              </div>
              <p className="text-muted-foreground text-sm">
                {automation.description || t("operations.noDescription")}
              </p>
              <div className="text-muted-foreground mt-1 flex flex-wrap gap-2 text-xs">
                <span>
                  {t("operations.trigger")} {automation.trigger}
                </span>
                {automation.cron_expr && (
                  <span>
                    {t("operations.cron")} {automation.cron_expr}
                  </span>
                )}
                {automation.next_run_at && (
                  <span>
                    {t("operations.next")} {formatRelativeTime(automation.next_run_at)}
                  </span>
                )}
                {automation.last_run_at && (
                  <span>
                    {t("operations.last")} {formatRelativeTime(automation.last_run_at)}
                  </span>
                )}
              </div>
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => onRun(automation)}
                disabled={isPending}
              >
                {t("operations.runNow")}
              </Button>
              <Button variant="outline" size="sm" onClick={() => onEdit(automation)}>
                {t("operations.edit")}
              </Button>
              <Button
                variant="destructive"
                size="sm"
                onClick={() => onDelete(automation)}
                disabled={isPending}
              >
                {t("operations.delete")}
              </Button>
            </div>
          </div>
        </li>
      ))}
    </ul>
  );
}
