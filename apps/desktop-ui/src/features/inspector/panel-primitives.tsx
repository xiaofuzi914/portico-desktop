import { useTranslation } from "@/lib/i18n-react";

export function InlineError({ title, message }: { title: string; message: string }) {
  return (
    <div className="p-3">
      <div className="rounded border border-red-200 bg-red-50 p-3 text-xs text-red-700 dark:border-red-900 dark:bg-red-950">
        <p className="font-semibold">{title}</p>
        <p>{message}</p>
      </div>
    </div>
  );
}

export function PanelLoading() {
  const { t } = useTranslation();
  return <p className="text-muted-foreground p-3 text-xs">{t("inspector.loading")}</p>;
}

export function EmptyState({ message }: { message: string }) {
  return <p className="text-muted-foreground p-3 text-xs">{message}</p>;
}
