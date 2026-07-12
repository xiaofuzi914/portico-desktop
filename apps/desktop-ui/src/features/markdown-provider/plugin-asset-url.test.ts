import { describe, expect, it, beforeEach } from "vitest";
import {
  convertFileSrcPrefix,
  pluginFileUrl,
  prepareProviderHtml,
} from "./plugin-asset-url";

beforeEach(() => {
  (window as unknown as { __TAURI_INTERNALS__: { convertFileSrc: (p: string) => string } }).__TAURI_INTERNALS__ =
    {
      convertFileSrc: (filePath: string) => `asset://localhost/${encodeURIComponent(filePath)}`,
    };
});

describe("plugin-asset-url", () => {
  it("builds absolute asset URLs so relative resolution is not required", () => {
    const url = pluginFileUrl("/Users/owen/.portico/plugins/installed/id", "provider.js");
    expect(url).toBe(
      `asset://localhost/${encodeURIComponent("/Users/owen/.portico/plugins/installed/id/provider.js")}`,
    );
  });

  it("rewrites relative script src and injects resolve bootstrap", () => {
    const html = `<!doctype html><html><head></head><body>
<script src="provider.js"></script>
<link rel="stylesheet" href="styles.css" />
</body></html>`;
    const prepared = prepareProviderHtml(html, "/tmp/plugin");
    expect(prepared).toContain("__PORTICO_RESOLVE__");
    expect(prepared).toContain(encodeURIComponent("/tmp/plugin/provider.js"));
    expect(prepared).toContain(encodeURIComponent("/tmp/plugin/styles.css"));
    expect(prepared).not.toMatch(/src="provider\.js"/);
  });

  it("derive prefix consistent with convertFileSrc", () => {
    const prefix = convertFileSrcPrefix();
    expect(prefix).toBe("asset://localhost/");
  });

  it("resolve bootstrap returns absolute asset URLs unchanged (no double-join)", () => {
    const prepared = prepareProviderHtml("<html><head></head><body></body></html>", "/tmp/plugin");
    const match = prepared.match(/window\.__PORTICO_RESOLVE__ = function \(rel\) \{[\s\S]*?\n {2}\};/);
    expect(match).toBeTruthy();
    // Evaluate bootstrap in isolation
    const run = new Function(`${prepared.match(/<script>([\s\S]*?)<\/script>/)?.[1] ?? ""}; return window.__PORTICO_RESOLVE__;`);
    const resolve = run() as (rel: string) => string;
    const absolute = "asset://localhost/%2Ftmp%2Fplugin%2Fvendor%2Fengine%2F_locales%2Fen%2Fmessages.json";
    expect(resolve(absolute)).toBe(absolute);
    // Corrupted double-prefix form seen in tauri asset errors
    const corrupted = `vendor/engine/${absolute}`;
    expect(resolve(corrupted)).toBe(absolute);
  });
});
