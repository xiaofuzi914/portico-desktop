import { useEffect } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { ArtifactPreview } from "@/lib/schemas";
import { MarkdownWorkspacePreview } from "./markdown-workspace-preview";

type Props = Readonly<{
  preview: ArtifactPreview;
  onClose: () => void;
}>;

export function MarkdownPreviewDialog({ preview, onClose }: Props) {
  const filename = preview.path.split(/[\\/]/).at(-1) ?? "Markdown preview";

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <div
      className="bg-background/80 fixed inset-0 z-50 flex items-center justify-center p-5 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label={filename}
    >
      <section className="bg-background flex h-[min(92vh,1080px)] w-[min(92vw,1440px)] flex-col overflow-hidden rounded-xl border shadow-2xl">
        <header className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
          <p className="min-w-0 flex-1 truncate text-sm font-semibold">{filename}</p>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={onClose}
            aria-label="Close preview"
          >
            <X className="h-4 w-4" />
          </Button>
        </header>
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <MarkdownWorkspacePreview preview={preview} />
        </div>
      </section>
    </div>
  );
}
