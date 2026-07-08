import { useMemo } from "react";
import type { ArtifactPreview } from "@/lib/schemas";

interface ArtifactPreviewProps {
  preview: ArtifactPreview;
}

function buildDataUrl(mimeType: string, contentBase64: string): string {
  return `data:${mimeType};base64,${contentBase64}`;
}

function parseCsv(content: string): string[][] {
  return content.split("\n").map((line) => line.split(","));
}

export function ArtifactPreview({ preview }: ArtifactPreviewProps) {
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
    const rows = parseCsv(atob(content_base64));
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

  const text = atob(content_base64);

  if (mime_type === "text/markdown") {
    return (
      <div className="rounded-md border p-4 font-mono text-sm whitespace-pre-wrap">{text}</div>
    );
  }

  return (
    <pre className="max-h-96 overflow-auto rounded-md border p-4 text-sm">
      <code>{text}</code>
    </pre>
  );
}
