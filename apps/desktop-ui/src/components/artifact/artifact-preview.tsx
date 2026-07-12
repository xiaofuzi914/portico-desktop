import { useMemo } from "react";
import type { ArtifactPreview } from "@/lib/schemas";
import { MarkdownBody } from "@/components/markdown/markdown-body";

interface ArtifactPreviewProps {
  preview: ArtifactPreview;
  /** View-layer presentation classes for Markdown (mode / polish). */
  presentationClassName?: string;
}

function buildDataUrl(mimeType: string, contentBase64: string): string {
  return `data:${mimeType};base64,${contentBase64}`;
}

function parseCsv(content: string): string[][] {
  return content.split("\n").map((line) => line.split(","));
}

function decodeBase64Utf8(contentBase64: string): string {
  const binary = atob(contentBase64);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

export function ArtifactPreview({ preview, presentationClassName }: ArtifactPreviewProps) {
  const { path, mime_type, content_base64 } = preview;
  const dataUrl = useMemo(
    () => buildDataUrl(mime_type, content_base64),
    [mime_type, content_base64],
  );

  if (mime_type.startsWith("image/")) {
    return <img src={dataUrl} alt={path} className="h-auto max-w-full rounded-md border" />;
  }

  if (mime_type === "application/pdf") {
    return (
      <object
        data={dataUrl}
        type="application/pdf"
        className="h-96 w-full rounded-md border"
        aria-label={path}
      />
    );
  }

  if (mime_type === "text/csv") {
    const rows = parseCsv(decodeBase64Utf8(content_base64));
    return (
      <div className="overflow-auto rounded-md border">
        <table className="w-full border-collapse text-sm">
          <tbody>
            {rows.map((row, rowIndex) => (
              <tr key={rowIndex} className="border-b last:border-b-0">
                {row.map((cell, cellIndex) => (
                  <td key={cellIndex} className="border-r px-3 py-2 last:border-r-0">
                    {cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  const text = decodeBase64Utf8(content_base64);

  if (mime_type === "text/markdown") {
    return <MarkdownBody content={text} presentationClassName={presentationClassName} />;
  }

  return (
    <pre className="max-h-96 overflow-auto rounded-md border p-4 text-sm">
      <code>{text}</code>
    </pre>
  );
}
