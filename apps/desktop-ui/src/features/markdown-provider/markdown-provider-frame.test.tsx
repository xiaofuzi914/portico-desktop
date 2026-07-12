import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { MarkdownProviderFrame, type MarkdownProviderFrameHandle } from "./markdown-provider-frame";

vi.mock("@/lib/tauri-api", () => ({
  readPluginEntrypointHtml: vi.fn(async () =>
    `<!doctype html><html><head></head><body>
<main id="document"><p id="status">就绪，等待内容…</p></main>
<script src="provider.js"></script>
</body></html>`,
  ),
  readPluginPackageTextFile: vi.fn(async (installPath: string, rel: string) => {
    if (rel === "provider.js") {
      return `window.parent.postMessage({protocol:"portico.markdown-provider/v1",type:"ready"},"*");
addEventListener("message", (e) => {
  const d = e.data;
  if (!d || d.protocol !== "portico.markdown-provider/v1" || !d.id) return;
  if (d.type === "render") {
    parent.postMessage({protocol:d.protocol,type:"result",id:d.id,result:{kind:"html",html:"<p>ok</p>"}},"*");
  }
});`;
    }
    if (rel === "styles.css") return "body{margin:0}";
    throw new Error(`missing ${installPath}/${rel}`);
  }),
}));

const mounted: Array<{ root: ReturnType<typeof createRoot>; host: HTMLDivElement }> = [];

beforeEach(() => {
  (window as unknown as { __TAURI_INTERNALS__?: { convertFileSrc: (p: string) => string } }).__TAURI_INTERNALS__ =
    {
      convertFileSrc: (filePath: string) => `asset://localhost/${encodeURIComponent(filePath)}`,
    };
});

afterEach(() => {
  for (const item of mounted.splice(0)) {
    act(() => item.root.unmount());
    item.host.remove();
  }
  vi.unstubAllGlobals();
});

function mount(onReady = vi.fn()) {
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  const ref = { current: null as MarkdownProviderFrameHandle | null };
  act(() => {
    root.render(
      <MarkdownProviderFrame
        ref={ref}
        providerId="docu-md"
        installPath="/tmp/plugins/docu"
        entrypoint="index.html"
        onReady={onReady}
      />,
    );
  });
  mounted.push({ root, host });
  return { host, ref, onReady };
}

describe("MarkdownProviderFrame", () => {
  it("loads provider via srcdoc with script-only sandbox", async () => {
    const { host } = mount();
    await act(async () => {
      await new Promise((r) => setTimeout(r, 30));
    });
    const iframe = host.querySelector("iframe");
    expect(iframe?.getAttribute("sandbox")).toBe("allow-scripts");
    expect(iframe?.hasAttribute("srcdoc") || iframe?.srcdoc).toBeTruthy();
  });

  it("does not treat host-window messages as provider ready extras", async () => {
    const { onReady } = mount();
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });
    const before = onReady.mock.calls.length;
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { protocol: "portico.markdown-provider/v1", type: "ready" },
        source: window,
      }),
    );
    expect(onReady.mock.calls.length).toBe(before);
  });

  it("correlates render responses after srcdoc is ready", async () => {
    const { host, ref } = mount();
    await act(async () => {
      await new Promise((r) => setTimeout(r, 80));
    });
    const iframe = host.querySelector("iframe");
    expect(iframe).toBeTruthy();
    const postMessage = vi.spyOn(iframe!.contentWindow!, "postMessage");

    const renderPromise = ref.current!.render("# Hello", { theme: "light" });
    const renderCall = postMessage.mock.calls
      .map((call) => call[0] as { id?: string; type?: string })
      .reverse()
      .find((payload) => payload?.type === "render");
    expect(renderCall?.type).toBe("render");

    window.dispatchEvent(
      new MessageEvent("message", {
        source: iframe!.contentWindow,
        data: {
          protocol: "portico.markdown-provider/v1",
          type: "result",
          id: renderCall!.id,
          result: { kind: "html", html: "<h1>Hello</h1>" },
        },
      }),
    );
    await expect(renderPromise).resolves.toEqual({ kind: "html", html: "<h1>Hello</h1>" });
  });
});
