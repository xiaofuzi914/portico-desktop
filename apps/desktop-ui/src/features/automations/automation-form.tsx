import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { Automation } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

export interface AutomationFormData {
  name: string;
  description: string;
  cronExpr: string;
  enabled: boolean;
}

interface AutomationFormProps {
  initial?: Automation | null;
  onSubmit: (data: AutomationFormData) => void;
  onCancel?: () => void;
  isPending?: boolean;
}

export function AutomationForm({ initial, onSubmit, onCancel, isPending }: AutomationFormProps) {
  const { t } = useTranslation();

  return (
    <form
      className="space-y-4"
      onSubmit={(e) => {
        e.preventDefault();
        const form = e.currentTarget;
        const formData = new FormData(form);
        onSubmit({
          name: String(formData.get("name") ?? ""),
          description: String(formData.get("description") ?? ""),
          cronExpr: String(formData.get("cronExpr") ?? ""),
          enabled: formData.get("enabled") === "on",
        });
      }}
    >
      <div className="grid gap-3 sm:grid-cols-2">
        <Input
          name="name"
          placeholder={t("operations.automationName")}
          defaultValue={initial?.name ?? ""}
          required
        />
        <Input
          name="cronExpr"
          placeholder={t("operations.cronExpression")}
          defaultValue={initial?.cron_expr ?? ""}
        />
      </div>
      <Textarea
        name="description"
        placeholder={t("operations.descriptionOptional")}
        defaultValue={initial?.description ?? ""}
        rows={3}
      />
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          name="enabled"
          defaultChecked={initial?.enabled ?? true}
          className="h-4 w-4"
        />
        {t("operations.enabled")}
      </label>
      <div className="flex gap-2">
        <Button type="submit" disabled={isPending}>
          {initial ? t("operations.saveChanges") : t("operations.createAutomation")}
        </Button>
        {onCancel && (
          <Button type="button" variant="outline" onClick={onCancel}>
            {t("memory.cancel")}
          </Button>
        )}
      </div>
    </form>
  );
}
