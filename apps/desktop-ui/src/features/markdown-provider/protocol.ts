export const MARKDOWN_PROVIDER_PROTOCOL = "portico.markdown-provider/v1" as const;

export type MarkdownExportFormat = "html" | "docx" | "pdf";
export type MarkdownRenderOptions = Readonly<{ theme?: string }>;
export type MarkdownRenderResult = Readonly<{ kind: "html"; html: string }>;
export type MarkdownExportResult = Readonly<{
  kind: "file";
  mimeType: string;
  filename: string;
  contentBase64: string;
}>;

export type ProviderRequest =
  | Readonly<{
      protocol: typeof MARKDOWN_PROVIDER_PROTOCOL;
      type: "render";
      id: string;
      markdown: string;
      options: MarkdownRenderOptions;
    }>
  | Readonly<{
      protocol: typeof MARKDOWN_PROVIDER_PROTOCOL;
      type: "export";
      id: string;
      markdown: string;
      format: MarkdownExportFormat;
    }>;

export type ProviderResponse =
  | Readonly<{ protocol: typeof MARKDOWN_PROVIDER_PROTOCOL; type: "ready" }>
  | Readonly<{
      protocol: typeof MARKDOWN_PROVIDER_PROTOCOL;
      type: "result";
      id: string;
      result: MarkdownRenderResult | MarkdownExportResult;
    }>
  | Readonly<{
      protocol: typeof MARKDOWN_PROVIDER_PROTOCOL;
      type: "error";
      id: string;
      error: Readonly<{ code: string; message: string }>;
    }>;

export function isProviderResponse(value: unknown): value is ProviderResponse {
  if (!isRecord(value) || value.protocol !== MARKDOWN_PROVIDER_PROTOCOL) return false;
  if (value.type === "ready") return true;
  if (typeof value.id !== "string" || value.id.length === 0) return false;
  if (value.type === "error") {
    return (
      isRecord(value.error) &&
      typeof value.error.code === "string" &&
      typeof value.error.message === "string"
    );
  }
  return value.type === "result" && isProviderResult(value.result);
}

function isProviderResult(value: unknown): value is MarkdownRenderResult | MarkdownExportResult {
  if (!isRecord(value)) return false;
  if (value.kind === "html") return typeof value.html === "string";
  return (
    value.kind === "file" &&
    typeof value.mimeType === "string" &&
    typeof value.filename === "string" &&
    typeof value.contentBase64 === "string"
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
