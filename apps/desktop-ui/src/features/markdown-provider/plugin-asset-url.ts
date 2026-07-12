import { convertFileSrc } from "@tauri-apps/api/core";
import { readPluginEntrypointHtml, readPluginPackageTextFile } from "@/lib/tauri-api";

/**
 * Tauri's convertFileSrc encodes the *entire* absolute path as one URL path
 * segment (`asset://localhost/%2FUsers%2F...%2Findex.html`).
 *
 * Relative URLs like `provider.js` then resolve to `asset://localhost/provider.js`
 * and 404 — the iframe shows static HTML ("就绪，等待内容…") with no JS.
 *
 * Always resolve package files through convertFileSrc(absolutePath).
 */
export function pluginFileUrl(installPath: string, relativePath: string): string {
  const base = installPath.replace(/[/\\]+$/, "");
  const rel = relativePath.replace(/^[/\\]+/, "").replace(/\\/g, "/");
  return convertFileSrc(`${base}/${rel}`);
}

/** Prefix such that `prefix + encodeURIComponent(absPath)` === convertFileSrc(absPath). */
export function convertFileSrcPrefix(): string {
  const probe = "/__portico_probe__";
  const url = convertFileSrc(probe);
  const encoded = encodeURIComponent(probe);
  if (url.endsWith(encoded)) {
    return url.slice(0, url.length - encoded.length);
  }
  // Fallback for unexpected encoding
  return url.includes("://") ? `${url.split("://")[0]}://localhost/` : "asset://localhost/";
}

/**
 * Bootstrap injected into the provider document so dynamic script/link loads
 * (marked, katex, mermaid) also use absolute asset URLs.
 */
export function buildResolveBootstrap(installPath: string): string {
  const base = installPath.replace(/[/\\]+$/, "");
  const prefix = convertFileSrcPrefix();
  return `(() => {
  const INSTALL = ${JSON.stringify(base)};
  const PREFIX = ${JSON.stringify(prefix)};
  window.__PORTICO_RESOLVE__ = function (rel) {
    if (rel == null || rel === "") return rel;
    let s = String(rel);
    // Already absolute — never join onto install path (double-resolve bug).
    if (/^(data:|blob:|asset:|https?:|tauri:|chrome-extension:)/i.test(s)) return s;
    // Peel accidental "vendor/engine/asset://…" or filesystem+asset composites.
    for (const marker of ["asset://", "http://asset.localhost", "https://asset.localhost"]) {
      const idx = s.indexOf(marker);
      if (idx > 0) return s.slice(idx);
    }
    const cleaned = s.replace(/^\\.\\//, "").replace(/^\\//, "").replace(/\\\\/g, "/");
    if (!cleaned) return PREFIX + encodeURIComponent(INSTALL + "/");
    const abs = INSTALL + "/" + cleaned;
    return PREFIX + encodeURIComponent(abs);
  };
})();`;
}

/**
 * Rewrite relative src/href in HTML to absolute convertFileSrc URLs, inject
 * resolve bootstrap, and relax CSP so asset: scripts/styles can load from srcdoc.
 */
export function prepareProviderHtml(html: string, installPath: string): string {
  const bootstrap = buildResolveBootstrap(installPath);
  const bootstrapTag = `<script>${bootstrap}</script>`;

  // Allow absolute asset: loads when document is served via srcdoc (opaque origin).
  // connect-src must allow asset: so provider can fetch CSS (KaTeX) and rewrite font URLs.
  let next = html.replace(
    /http-equiv=["']Content-Security-Policy["']\s+content=["'][^"']*["']/i,
    `http-equiv="Content-Security-Policy" content="default-src 'none'; connect-src asset: http://asset.localhost https://asset.localhost; img-src data: blob: asset: http://asset.localhost https://asset.localhost; font-src data: asset: http://asset.localhost https://asset.localhost; media-src 'none'; object-src 'none'; frame-src 'none'; form-action 'none'; base-uri 'none'; style-src 'unsafe-inline' asset: http://asset.localhost https://asset.localhost; script-src 'unsafe-inline' 'wasm-unsafe-eval' asset: http://asset.localhost https://asset.localhost; worker-src blob: asset: http://asset.localhost https://asset.localhost"`,
  );

  // Inject bootstrap as early as possible in <head>.
  if (/<head[^>]*>/i.test(next)) {
    next = next.replace(/<head[^>]*>/i, (open) => `${open}${bootstrapTag}`);
  } else {
    next = bootstrapTag + next;
  }

  // Rewrite relative script src and link href (not data:/absolute).
  next = next.replace(
    /(\b(?:src|href))=(["'])(?!data:|blob:|https?:|asset:|tauri:|\/\/)([^"']+)\2/gi,
    (_match, attr: string, quote: string, rel: string) => {
      const url = pluginFileUrl(installPath, rel);
      return `${attr}=${quote}${url}${quote}`;
    },
  );

  return next;
}

export async function loadProviderSrcDoc(
  installPath: string,
  entrypoint: string,
): Promise<string> {
  // Prefer Rust read (no CSP / asset-fetch issues). Fall back to convertFileSrc fetch.
  let html: string;
  try {
    html = await readPluginEntrypointHtml(installPath, entrypoint);
  } catch {
    const entryUrl = pluginFileUrl(installPath, entrypoint);
    const response = await fetch(entryUrl);
    if (!response.ok) {
      throw new Error(
        `Failed to load plugin entrypoint (${response.status}): ${entrypoint}`,
      );
    }
    html = await response.text();
  }

  let prepared = prepareProviderHtml(html, installPath);

  // Inline provider.js so the protocol listener is guaranteed to run even if
  // external asset:// script loads are blocked by the webview.
  try {
    const providerJs = await readPluginPackageTextFile(installPath, "provider.js");
    const safeJs = providerJs.replace(/<\/script/gi, "<\\/script");
    const inline = `<script data-portico-inline-provider="1">\n${safeJs}\n</script>`;
    // Remove external provider.js references (relative or absolute asset URLs).
    prepared = prepared.replace(
      /<script\b[^>]*\bsrc=(["'])[^"']*provider\.js\1[^>]*>\s*<\/script>/gi,
      "",
    );
    if (/<\/body>/i.test(prepared)) {
      prepared = prepared.replace(/<\/body>/i, `${inline}</body>`);
    } else {
      prepared += inline;
    }
  } catch {
    // Keep absolute asset script tags from prepareProviderHtml.
  }

  // Prefer inlined styles.css as well (relative link would 404 under convertFileSrc).
  try {
    const css = await readPluginPackageTextFile(installPath, "styles.css");
    const safeCss = css.replace(/<\/style/gi, "<\\/style");
    const styleTag = `<style data-portico-inline-style="1">\n${safeCss}\n</style>`;
    prepared = prepared.replace(
      /<link\b[^>]*\bhref=(["'])[^"']*styles\.css\1[^>]*>/gi,
      styleTag,
    );
  } catch {
    // keep link rewrite
  }

  return prepared;
}
