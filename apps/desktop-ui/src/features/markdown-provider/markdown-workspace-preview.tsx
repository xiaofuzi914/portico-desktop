import { useQuery } from "@tanstack/react-query";
import { Download, Printer, RefreshCw, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { ArtifactPreview } from "@/components/artifact/artifact-preview";
import { Button } from "@/components/ui/button";
import { exportRenderedMarkdown } from "@/lib/markdown-export";
import { listPlugins } from "@/lib/tauri-api";
import type { ArtifactPreview as ArtifactPreviewType, PluginManifest } from "@/lib/schemas";
import { pluginKeys } from "@/lib/query-keys";
import { cn } from "@/lib/utils";
import { MarkdownProviderFrame, type MarkdownProviderFrameHandle } from "./markdown-provider-frame";
import type { MarkdownExportFormat } from "./protocol";
import {
  type MarkdownFontScale,
  type MarkdownPresentationMode,
  type MarkdownPresentationState,
  PRESENTATION_MODE_OPTIONS,
  loadMarkdownPresentation,
  presentationSurfaceClass,
  saveMarkdownPresentation,
} from "./markdown-presentation";

type Props = Readonly<{ preview: ArtifactPreviewType }>;

function decodeUtf8(contentBase64: string): string {
  const binary = atob(contentBase64);
  return new TextDecoder().decode(Uint8Array.from(binary, (item) => item.charCodeAt(0)));
}

function canUsePluginProvider(plugin: PluginManifest): boolean {
  return Boolean(plugin.entrypoint && plugin.install_path);
}

function downloadBase64(contentBase64: string, mimeType: string, filename: string): void {
  if (contentBase64.length > 70_000_000) throw new Error("Provider export exceeds the 50 MB limit");
  const binary = atob(contentBase64);
  const bytes = Uint8Array.from(binary, (item) => item.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: mimeType }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  queueMicrotask(() => URL.revokeObjectURL(url));
}

const BUILTIN_PROVIDER_ID = "builtin";
const MARKDOWN_PROVIDER_STORAGE_KEY = "portico.markdownProvider";

export function MarkdownWorkspacePreview({ preview }: Props) {
  const providerRef = useRef<MarkdownProviderFrameHandle>(null);
  const fallbackRef = useRef<HTMLDivElement>(null);
  const [providerReady, setProviderReady] = useState(false);
  const [busyFormat, setBusyFormat] = useState<MarkdownExportFormat | null>(null);
  const [error, setError] = useState<string | null>(null);
  // null = not yet resolved from plugins list; string = explicit user/system choice.
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
    () => localStorage.getItem(MARKDOWN_PROVIDER_STORAGE_KEY),
  );
  // View-layer presentation: preview first, then switch effect / polish.
  const [presentation, setPresentation] = useState<MarkdownPresentationState>(() =>
    loadMarkdownPresentation(),
  );

  const source = useMemo(() => decodeUtf8(preview.content_base64), [preview.content_base64]);
  const filename = preview.path.split(/[\\/]/).at(-1) ?? "document.md";
  const { data: plugins = [] } = useQuery({ queryKey: pluginKeys.list(), queryFn: listPlugins });
  const availableProviders = plugins.filter(
    (plugin) =>
      plugin.enabled &&
      plugin.capabilities.includes("markdown.preview") &&
      canUsePluginProvider(plugin),
  );

  // Prefer an installed Markdown provider when the user has not pinned Built-in.
  useEffect(() => {
    if (selectedProviderId !== null) return;
    if (availableProviders.length === 0) {
      setSelectedProviderId(BUILTIN_PROVIDER_ID);
      return;
    }
    const preferred = availableProviders[0]!;
    setSelectedProviderId(preferred.id);
    localStorage.setItem(MARKDOWN_PROVIDER_STORAGE_KEY, preferred.id);
  }, [availableProviders, selectedProviderId]);

  // If stored id was uninstalled/disabled, fall back to first available or built-in.
  const resolvedProviderId = useMemo(() => {
    const choice = selectedProviderId ?? BUILTIN_PROVIDER_ID;
    if (choice === BUILTIN_PROVIDER_ID) return BUILTIN_PROVIDER_ID;
    if (availableProviders.some((plugin) => plugin.id === choice)) return choice;
    return availableProviders[0]?.id ?? BUILTIN_PROVIDER_ID;
  }, [selectedProviderId, availableProviders]);

  const useBuiltin = resolvedProviderId === BUILTIN_PROVIDER_ID;
  const provider = useBuiltin
    ? null
    : (availableProviders.find((plugin) => plugin.id === resolvedProviderId) ?? null);
  const installPath = provider?.install_path ?? null;
  const entrypoint = provider?.entrypoint ?? null;

  function selectProvider(id: string) {
    localStorage.setItem(MARKDOWN_PROVIDER_STORAGE_KEY, id);
    setSelectedProviderId(id);
    setProviderReady(false);
    setError(null);
  }

  // Plugin path is only for opt-in external engines; presentation modes apply to built-in.
  const showBuiltinSurface = !provider || !entrypoint || !installPath;
  const showSource = presentation.mode === "source" && showBuiltinSurface;
  const isDocumentMode =
    showBuiltinSurface && !showSource && presentation.mode === "document";
  const presentationClass = presentationSurfaceClass(presentation.mode, presentation.polish);

  useEffect(() => {
    saveMarkdownPresentation(presentation);
  }, [presentation]);

  useEffect(() => {
    setProviderReady(false);
    setError(null);
  }, [provider?.id]);

  // Fail fast if the provider never becomes ready (frame force-ready is ~0.6s).
  useEffect(() => {
    if (!provider || providerReady) return;
    const timer = window.setTimeout(() => {
      setError(
        "渲染引擎启动超时（未完成握手）。请点「重新安装」Markdown Viewer，或切换到 Built-in。",
      );
    }, 8_000);
    return () => window.clearTimeout(timer);
  }, [provider, providerReady, provider?.id]);

  // Push markdown as soon as iframe is ready (load / ready postMessage / force-ready).
  // Also retry once shortly after ready in case the first postMessage raced boot.
  useEffect(() => {
    if (!providerReady || !provider) return;
    let current = true;
    let attempt = 0;

    const pushRender = () => {
      const handle = providerRef.current;
      if (!handle || !current) return;
      setError(null);
      void handle
        .render(source)
        .then(() => {
          if (current) setError(null);
        })
        .catch((renderError: unknown) => {
          if (!current) return;
          attempt += 1;
          // One automatic retry after a short delay (iframe listener race).
          if (attempt === 1) {
            window.setTimeout(() => {
              if (current) pushRender();
            }, 300);
            return;
          }
          setError(renderError instanceof Error ? renderError.message : String(renderError));
        });
    };

    pushRender();
    return () => {
      current = false;
    };
  }, [provider?.id, providerReady, source, provider]);

  function updatePresentation(patch: Partial<MarkdownPresentationState>) {
    setPresentation((prev) => ({ ...prev, ...patch }));
  }

  function updatePolish(patch: Partial<MarkdownPresentationState["polish"]>) {
    setPresentation((prev) => ({
      ...prev,
      polish: { ...prev.polish, ...patch },
    }));
  }

  function setMode(mode: MarkdownPresentationMode) {
    setPresentation((prev) => ({
      ...prev,
      mode,
      // Source has nothing to polish; keep prior polishOpen for preview/document.
      polishOpen: mode === "source" ? false : prev.polishOpen,
    }));
  }

  async function exportDocument(format: MarkdownExportFormat) {
    setError(null);
    setBusyFormat(format);
    try {
      if (
        provider &&
        providerRef.current &&
        provider.capabilities.includes(`markdown.export.${format}`)
      ) {
        const result = await providerRef.current.export(source, format);
        const expected = {
          html: { mime: "text/html", extension: ".html" },
          docx: {
            mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            extension: ".docx",
          },
          pdf: { mime: "application/pdf", extension: ".pdf" },
        }[format];
        if (
          result.mimeType !== expected.mime ||
          !result.filename.toLowerCase().endsWith(expected.extension)
        ) {
          throw new Error("Provider returned an invalid export type");
        }
        const safeFilename = result.filename
          .split(/[\\/]/)
          .at(-1)
          ?.replace(/[^\p{L}\p{N}._ -]/gu, "_");
        downloadBase64(
          result.contentBase64,
          result.mimeType,
          safeFilename || `document${expected.extension}`,
        );
        return;
      }
      if (format === "pdf") {
        window.print();
        return;
      }
      const root =
        fallbackRef.current?.querySelector(".markdown-preview") ??
        fallbackRef.current?.querySelector(".markdown-source-view");
      if (!root) throw new Error("Markdown export surface is unavailable");
      await exportRenderedMarkdown(root, filename, format);
    } catch (exportError) {
      setError(exportError instanceof Error ? exportError.message : String(exportError));
    } finally {
      setBusyFormat(null);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b px-3 py-2">
        <label className="text-muted-foreground flex min-w-0 items-center gap-1.5 text-[10px]">
          <span className="shrink-0 font-medium">渲染引擎</span>
          <select
            className="border-input bg-background h-7 max-w-[14rem] rounded border px-1.5 text-[10px] font-medium text-foreground"
            value={resolvedProviderId}
            onChange={(event) => selectProvider(event.target.value)}
            aria-label="Markdown 渲染引擎"
          >
            <option value={BUILTIN_PROVIDER_ID}>Built-in（Portico）</option>
            {availableProviders.map((item) => (
              <option key={item.id} value={item.id}>
                {item.display_name}
              </option>
            ))}
          </select>
          {provider && !providerReady && (
            <span className="text-muted-foreground shrink-0">加载中…</span>
          )}
        </label>

        <span className="text-muted-foreground mr-auto truncate text-[10px]">
          {provider
            ? "第三方插件渲染"
            : "本地预览 · 可切换展示效果"}
        </span>

        {/* Presentation modes only apply to Built-in surface */}
        {showBuiltinSurface && (
          <div
            className="border-input bg-muted/40 flex rounded-md border p-0.5"
            role="tablist"
            aria-label="展示效果"
          >
            {PRESENTATION_MODE_OPTIONS.map((option) => (
              <button
                key={option.id}
                type="button"
                role="tab"
                aria-selected={presentation.mode === option.id}
                title={option.description}
                className={cn(
                  "h-6 rounded px-2 text-[10px] transition-colors",
                  presentation.mode === option.id
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground",
                )}
                onClick={() => setMode(option.id)}
              >
                {option.label}
              </button>
            ))}
          </div>
        )}

        {showBuiltinSurface && presentation.mode !== "source" && (
          <Button
            variant={presentation.polishOpen ? "outline" : "ghost"}
            size="sm"
            className="h-7 px-2 text-[10px]"
            onClick={() => updatePresentation({ polishOpen: !presentation.polishOpen })}
            aria-expanded={presentation.polishOpen}
            aria-label="修饰选项"
          >
            <SlidersHorizontal className="mr-1 h-3 w-3" />
            修饰
          </Button>
        )}
        {(["html", "docx"] as const).map((format) => (
          <Button
            key={format}
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-[10px]"
            onClick={() => void exportDocument(format)}
            disabled={busyFormat !== null}
          >
            {busyFormat === format ? (
              <RefreshCw className="mr-1 h-3 w-3 animate-spin" />
            ) : (
              <Download className="mr-1 h-3 w-3" />
            )}
            {format.toUpperCase()}
          </Button>
        ))}
        <Button
          variant="ghost"
          size="sm"
          className="h-7 px-2 text-[10px]"
          onClick={() => void exportDocument("pdf")}
          disabled={busyFormat !== null}
        >
          <Printer className="mr-1 h-3 w-3" />
          PDF
        </Button>
      </div>

      {showBuiltinSurface && presentation.polishOpen && presentation.mode !== "source" && (
        <div className="bg-muted/30 flex flex-wrap items-center gap-3 border-b px-3 py-2 text-[10px]">
          <label className="text-muted-foreground flex items-center gap-1.5">
            字号
            <select
              className="border-input bg-background h-6 rounded border px-1"
              value={presentation.polish.fontScale}
              onChange={(event) =>
                updatePolish({ fontScale: event.target.value as MarkdownFontScale })
              }
            >
              <option value="sm">小</option>
              <option value="md">中</option>
              <option value="lg">大</option>
            </select>
          </label>
          <label className="flex items-center gap-1.5">
            <input
              type="checkbox"
              className="size-3"
              checked={presentation.polish.diagramEmphasis}
              onChange={(event) => updatePolish({ diagramEmphasis: event.target.checked })}
            />
            强调图表
          </label>
          {presentation.mode === "document" && (
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                className="size-3"
                checked={presentation.polish.paperTheme}
                onChange={(event) => updatePolish({ paperTheme: event.target.checked })}
              />
              纸质版式
            </label>
          )}
          <span className="text-muted-foreground ml-auto">仅改变展示，不改源文件内容</span>
        </div>
      )}

      {error && <p className="shrink-0 px-3 py-2 text-xs text-red-600">{error}</p>}
      <div
        className={cn(
          // Plugin iframe fills this box and scrolls inside (.doc-surface).
          // Absolute fill avoids % height failing on non-flex block parents.
          provider && entrypoint && installPath
            ? "relative min-h-0 flex-1 overflow-hidden"
            : "min-h-0 flex-1 overflow-auto",
          isDocumentMode && !(provider && entrypoint && installPath)
            ? "markdown-document-stage p-6 sm:p-8"
            : !provider
              ? "p-3"
              : "",
        )}
      >
        {provider && entrypoint && installPath ? (
          <>
            <MarkdownProviderFrame
              // Only remount when the engine changes — NOT on every content edit
              // (remounting re-downloaded multi‑MB Mermaid and felt hung).
              key={provider.id}
              ref={providerRef}
              providerId={provider.id}
              installPath={installPath}
              entrypoint={entrypoint}
              className="absolute inset-0 block h-full w-full border-0"
              onReady={() => {
                setProviderReady(true);
                setError(null);
              }}
              onLoadError={(message) => {
                setProviderReady(false);
                setError(message);
              }}
            />
            <div ref={fallbackRef} className="markdown-print-surface hidden" aria-hidden="true">
              <ArtifactPreview preview={preview} presentationClassName={presentationClass} />
            </div>
          </>
        ) : showSource ? (
          <div ref={fallbackRef} className="markdown-print-surface">
            <pre
              className={cn(
                "markdown-source-view max-h-full overflow-auto rounded-md border p-4 font-mono text-xs leading-relaxed whitespace-pre",
              )}
            >
              {source}
            </pre>
          </div>
        ) : (
          <div
            ref={fallbackRef}
            className={cn(
              "markdown-print-surface",
              isDocumentMode && "markdown-document-stage-inner",
            )}
          >
            <ArtifactPreview preview={preview} presentationClassName={presentationClass} />
          </div>
        )}
      </div>
    </div>
  );
}
