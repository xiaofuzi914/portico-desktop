import { useRef, useState, type ReactNode } from "react";
import { SendHorizontal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import type { AgentRunId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";

interface ConversationComposerProps {
  runId?: AgentRunId;
  onSubmit: (content: string) => void;
  isSubmitting: boolean;
  controls?: ReactNode;
  disabled?: boolean;
  placeholder?: string;
}

export function ConversationComposer({
  runId,
  onSubmit,
  isSubmitting,
  controls,
  disabled = false,
  placeholder,
}: ConversationComposerProps) {
  const { t } = useTranslation();
  const [content, setContent] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const inputDisabled = disabled || !runId || isSubmitting;
  const canSubmit = !!runId && !disabled && !isSubmitting && content.trim().length > 0;

  const handleSubmit = () => {
    if (!canSubmit) return;
    onSubmit(content);
    setContent("");
    textareaRef.current?.focus();
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      handleSubmit();
    }
  };

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-3 rounded-lg border bg-background p-3 shadow-xs">
      <Textarea
        ref={textareaRef}
        value={content}
        onChange={(event) => setContent(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={
          placeholder ?? (runId ? t("agent.sendPlaceholder") : t("agent.startBeforeMessage"))
        }
        disabled={inputDisabled}
        className="h-20 max-h-20 min-h-20 resize-none border-0 px-1 py-1 text-sm leading-6 shadow-none focus-visible:ring-0 focus-visible:ring-offset-0"
      />
      <div className="flex items-center justify-between gap-3 border-t pt-3">
        <div className="min-w-0">{controls}</div>
        <Button onClick={handleSubmit} disabled={!canSubmit} className="shrink-0">
          <SendHorizontal className="h-4 w-4" />
          {isSubmitting ? t("agent.sending") : t("agent.send")}
        </Button>
      </div>
    </div>
  );
}
