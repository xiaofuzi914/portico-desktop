import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { loadProviderSrcDoc } from "./plugin-asset-url";
import {
  isProviderResponse,
  MARKDOWN_PROVIDER_PROTOCOL,
  type MarkdownExportFormat,
  type MarkdownExportResult,
  type MarkdownRenderOptions,
  type MarkdownRenderResult,
  type ProviderRequest,
} from "./protocol";

/** Render/export timeout after handshake; engines lazy-load on demand. */
const REQUEST_TIMEOUT_MS = 45_000;
const PING_INTERVAL_MS = 200;
const FORCE_READY_MS = 800;

type PendingRequest = {
  resolve: (result: MarkdownRenderResult | MarkdownExportResult) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

type ProviderRequestInput =
  | Readonly<{ type: "render"; markdown: string; options: MarkdownRenderOptions }>
  | Readonly<{ type: "export"; markdown: string; format: MarkdownExportFormat }>
  | Readonly<{ type: "ping" }>;

export type MarkdownProviderFrameHandle = Readonly<{
  render: (markdown: string, options?: MarkdownRenderOptions) => Promise<MarkdownRenderResult>;
  export: (markdown: string, format: MarkdownExportFormat) => Promise<MarkdownExportResult>;
  isReady: () => boolean;
}>;

type Props = Readonly<{
  providerId: string;
  /** Absolute install directory (e.g. ~/.portico/plugins/installed/{id}). */
  installPath: string;
  /** Package-relative entry HTML (e.g. index.html). */
  entrypoint: string;
  onReady?: () => void;
  onLoadError?: (message: string) => void;
  className?: string;
}>;

function isMessageFromFrame(event: MessageEvent, frame: HTMLIFrameElement | null): boolean {
  if (!frame) return false;
  const frameWindow = frame.contentWindow;
  if (!frameWindow) return false;
  if (event.source === frameWindow) return true;
  if (event.source === window || event.source === window.parent) return false;
  if (event.source == null) return true;
  return true;
}

export const MarkdownProviderFrame = forwardRef<MarkdownProviderFrameHandle, Props>(
  function MarkdownProviderFrame(
    { providerId, installPath, entrypoint, onReady, onLoadError, className },
    ref,
  ) {
    const iframeRef = useRef<HTMLIFrameElement>(null);
    const pendingRef = useRef(new Map<string, PendingRequest>());
    const sequenceRef = useRef(0);
    const readyRef = useRef(false);
    const onReadyRef = useRef(onReady);
    const onLoadErrorRef = useRef(onLoadError);
    onReadyRef.current = onReady;
    onLoadErrorRef.current = onLoadError;

    const [srcDoc, setSrcDoc] = useState<string | null>(null);
    const [bootError, setBootError] = useState<string | null>(null);

    const markReady = useCallback(() => {
      if (readyRef.current) return;
      readyRef.current = true;
      onReadyRef.current?.();
    }, []);

    const sendPing = useCallback(() => {
      const win = iframeRef.current?.contentWindow;
      if (!win) return;
      try {
        win.postMessage({ protocol: MARKDOWN_PROVIDER_PROTOCOL, type: "ping" }, "*");
      } catch {
        // mid-navigation
      }
    }, []);

    // Build srcdoc with absolute asset:// URLs (relative paths 404 under convertFileSrc).
    useLayoutEffect(() => {
      let cancelled = false;
      setSrcDoc(null);
      setBootError(null);
      readyRef.current = false;

      void loadProviderSrcDoc(installPath, entrypoint)
        .then((html) => {
          if (cancelled) return;
          setSrcDoc(html);
        })
        .catch((error: unknown) => {
          if (cancelled) return;
          const message =
            error instanceof Error ? error.message : `Failed to load provider: ${String(error)}`;
          setBootError(message);
          onLoadErrorRef.current?.(message);
        });

      return () => {
        cancelled = true;
      };
    }, [installPath, entrypoint]);

    const request = useCallback(
      (message: ProviderRequestInput) => {
        const providerWindow = iframeRef.current?.contentWindow;
        if (!providerWindow) {
          return Promise.reject(new Error("Markdown provider is unavailable"));
        }
        if (message.type === "ping") {
          sendPing();
          return Promise.resolve({ kind: "html" as const, html: "" });
        }
        const id = `${providerId}:${++sequenceRef.current}`;
        const payload = {
          ...message,
          protocol: MARKDOWN_PROVIDER_PROTOCOL,
          id,
        } as ProviderRequest;
        return new Promise<MarkdownRenderResult | MarkdownExportResult>((resolve, reject) => {
          const timeout = setTimeout(() => {
            pendingRef.current.delete(id);
            reject(new Error("Markdown provider request timed out"));
          }, REQUEST_TIMEOUT_MS);
          pendingRef.current.set(id, { resolve, reject, timeout });
          try {
            providerWindow.postMessage(payload, "*");
          } catch (error) {
            clearTimeout(timeout);
            pendingRef.current.delete(id);
            reject(
              error instanceof Error
                ? error
                : new Error("Failed to postMessage to Markdown provider"),
            );
          }
        });
      },
      [providerId, sendPing],
    );

    useImperativeHandle(
      ref,
      () => ({
        async render(markdown, options = {}) {
          const result = await request({ type: "render", markdown, options });
          if (result.kind !== "html") throw new Error("Provider returned an invalid render result");
          return result;
        },
        async export(markdown, format) {
          const result = await request({ type: "export", markdown, format });
          if (result.kind !== "file") throw new Error("Provider returned an invalid export result");
          return result;
        },
        isReady: () => readyRef.current,
      }),
      [request],
    );

    useLayoutEffect(() => {
      if (!srcDoc) return;
      readyRef.current = false;
      const pending = pendingRef.current;
      const iframe = iframeRef.current;

      const handleMessage = (event: MessageEvent) => {
        if (!isMessageFromFrame(event, iframeRef.current)) return;
        if (!isProviderResponse(event.data)) return;
        const message = event.data;
        if (message.type === "ready") {
          markReady();
          return;
        }
        const pendingRequest = pending.get(message.id);
        if (!pendingRequest) return;
        clearTimeout(pendingRequest.timeout);
        pending.delete(message.id);
        if (message.type === "error") {
          pendingRequest.reject(new Error(message.error.message));
        } else {
          pendingRequest.resolve(message.result);
        }
      };

      const handleNativeLoad = () => {
        markReady();
        sendPing();
      };

      window.addEventListener("message", handleMessage);
      iframe?.addEventListener("load", handleNativeLoad);

      const pingTimer = window.setInterval(() => {
        if (readyRef.current) return;
        sendPing();
      }, PING_INTERVAL_MS);

      const forceReadyTimer = window.setTimeout(() => {
        if (!readyRef.current && iframeRef.current?.contentWindow) {
          markReady();
          sendPing();
        }
      }, FORCE_READY_MS);

      return () => {
        window.removeEventListener("message", handleMessage);
        iframe?.removeEventListener("load", handleNativeLoad);
        window.clearInterval(pingTimer);
        window.clearTimeout(forceReadyTimer);
        for (const item of pending.values()) {
          clearTimeout(item.timeout);
          item.reject(new Error("Markdown provider was disposed"));
        }
        pending.clear();
      };
    }, [srcDoc, providerId, markReady, sendPing]);

    if (bootError) {
      return (
        <div className={className} role="alert">
          <p className="text-destructive p-4 text-sm">{bootError}</p>
        </div>
      );
    }

    if (!srcDoc) {
      return (
        <div className={className}>
          <p className="text-muted-foreground p-4 text-sm">正在加载渲染引擎…</p>
        </div>
      );
    }

    return (
      <iframe
        ref={iframeRef}
        title={`Markdown provider: ${providerId}`}
        srcDoc={srcDoc}
        // Cross-origin asset scripts; do NOT add allow-same-origin with srcdoc
        // (would inherit parent origin). postMessage still works with allow-scripts.
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        className={className}
        onLoad={() => {
          markReady();
          sendPing();
        }}
      />
    );
  },
);
