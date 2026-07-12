export type MarkdownExportFormat = "html" | "docx";

const EXPORT_CSS = `
:root{color-scheme:light dark}body{max-width:860px;margin:40px auto;padding:0 24px;font:16px/1.65 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#24292f;background:#fff}h1,h2{border-bottom:1px solid #d0d7de;padding-bottom:.3em}pre{overflow:auto;padding:16px;border-radius:6px;background:#f6f8fa}code{font-family:ui-monospace,SFMono-Regular,Consolas,monospace}blockquote{margin-left:0;padding-left:1em;color:#57606a;border-left:4px solid #d0d7de}table{border-collapse:collapse}th,td{padding:6px 13px;border:1px solid #d0d7de}img{max-width:100%}@media(prefers-color-scheme:dark){body{color:#e6edf3;background:#0d1117}h1,h2,th,td{border-color:#30363d}pre{background:#161b22}blockquote{color:#8b949e;border-color:#3b434b}}`;

function escapeHtml(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

export function buildStandaloneMarkdownHtml(title: string, renderedHtml: string): string {
  return `<!doctype html>\n<html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${escapeHtml(title)}</title><style>${EXPORT_CSS}</style></head><body><main>${renderedHtml}</main></body></html>`;
}

export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  queueMicrotask(() => URL.revokeObjectURL(url));
}

export async function exportRenderedMarkdown(
  root: Element,
  sourceFilename: string,
  format: MarkdownExportFormat,
): Promise<void> {
  const basename = sourceFilename.replace(/\.md(?:own)?$/i, "") || "document";
  if (format === "html") {
    const html = buildStandaloneMarkdownHtml(basename, root.innerHTML);
    downloadBlob(new Blob([html], { type: "text/html;charset=utf-8" }), `${basename}.html`);
    return;
  }
  const { buildMarkdownDocx } = await import("./markdown-docx");
  downloadBlob(await buildMarkdownDocx(root), `${basename}.docx`);
}
