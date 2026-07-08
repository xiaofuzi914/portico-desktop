import { Button } from "@/components/ui/button";
import { useTranslation } from "@/lib/i18n-react";

export interface ApprovalModalProps {
  open: boolean;
  action: string;
  resource: string;
  onApprove: () => void;
  onDeny: () => void;
  onClose?: () => void;
}

export function ApprovalModal({
  open,
  action,
  resource,
  onApprove,
  onDeny,
  onClose,
}: ApprovalModalProps) {
  const { t } = useTranslation();

  if (!open) {
    return null;
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="approval-title"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClose?.();
        }
      }}
    >
      <div className="bg-background w-full max-w-md rounded-xl border p-6 shadow-lg">
        <h2 id="approval-title" className="text-lg font-semibold">
          {t("approval.required")}
        </h2>
        <p className="text-muted-foreground mt-2 text-sm">
          {t("approval.body")}
        </p>

        <div className="bg-muted mt-4 space-y-3 rounded-lg p-3 text-sm">
          <div>
            <span className="font-medium">{t("approval.action")}</span>{" "}
            <span className="font-mono">{action}</span>
          </div>
          <div>
            <span className="font-medium">{t("approval.resource")}</span>{" "}
            <span className="font-mono break-all">{resource}</span>
          </div>
        </div>

        <div className="mt-6 flex justify-end gap-3">
          <Button variant="outline" onClick={onDeny}>
            {t("approval.deny")}
          </Button>
          <Button onClick={onApprove}>{t("approval.approve")}</Button>
        </div>
      </div>
    </div>
  );
}
