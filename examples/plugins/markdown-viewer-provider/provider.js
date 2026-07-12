/**
 * Portico Markdown Provider bridge (stable host protocol).
 *
 * UPGRADE CONTRACT — keep these properties across upstream version bumps:
 * 1. Portico owns this file + chrome-shim.js + styles.css + vendor/fallback.
 *    Upstream may only replace vendor/engine/ (and package version metadata).
 * 2. Default render path is lightweight fallback (TRY_UPSTREAM_BY_DEFAULT=false).
 *    Never re-introduce "upstream-first with long timeouts" as the default.
 * 3. Mermaid / KaTeX must stay lazy (load only when the document needs them).
 * 4. Asset URLs must go through resolveAsset / __PORTICO_RESOLVE__ (no double-join).
 * 5. Version comes from <meta name="portico-plugin-version"> (written at package time).
 * 6. Dense Mermaid zoom/lightbox (enhanceMermaidWrap) stays in this bridge file + styles.css.
 *
 * Host: portico.markdown-provider/v1
 */
const protocol = "portico.markdown-provider/v1";
const documentRoot = document.getElementById("document");
const UPSTREAM_VERSION =
  (typeof document !== "undefined" &&
    document.querySelector?.('meta[name="portico-plugin-version"]')?.getAttribute("content")) ||
  (typeof window !== "undefined" && window.__PORTICO_PLUGIN_VERSION__) ||
  "5.2.0";

/**
 * Full chrome extension runtime is incomplete under srcdoc. Prefer fallback so
 * first paint is ~tens of ms (marked only) instead of multi-second upstream boot.
 * Set true only when debugging the vendored engine.
 */
const TRY_UPSTREAM_BY_DEFAULT = false;

const engine = {
  mode: null, // "upstream" | "fallback"
  mermaid: null,
  katex: null,
  marked: null,
  loading: null,
  katexLoading: null,
  mermaidLoading: null,
};

let viewerEl = null;

function resolveAsset(path) {
  const raw = String(path ?? "");
  if (/^(data:|blob:|asset:|https?:|tauri:)/i.test(raw)) return raw;
  // Peel accidental "…/vendor/engine/asset://…" double-prefix
  for (const marker of ["asset://", "http://asset.localhost", "https://asset.localhost"]) {
    const idx = raw.indexOf(marker);
    if (idx > 0) return raw.slice(idx);
  }
  if (typeof window.__PORTICO_RESOLVE__ === "function") {
    return window.__PORTICO_RESOLVE__(path);
  }
  return path;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function setStatus(text) {
  if (!documentRoot) return;
  documentRoot.innerHTML = `<p class="doc-placeholder" id="status">${escapeHtml(text)}</p>`;
  viewerEl = null;
}

function loadScript(relPath) {
  const url = resolveAsset(relPath);
  return new Promise((resolve, reject) => {
    const existing = document.querySelector(`script[data-src="${relPath}"]`);
    if (existing) {
      if (existing.dataset.loaded === "1") {
        resolve();
        return;
      }
      existing.addEventListener("load", () => resolve(), { once: true });
      existing.addEventListener(
        "error",
        () => reject(new Error(`Failed to load ${relPath}`)),
        { once: true },
      );
      return;
    }
    const script = document.createElement("script");
    script.src = url;
    script.async = false;
    script.dataset.src = relPath;
    script.onload = () => {
      script.dataset.loaded = "1";
      resolve();
    };
    script.onerror = () => reject(new Error(`Failed to load ${relPath} (${url})`));
    document.head.appendChild(script);
  });
}

async function loadStylesheet(relPath) {
  if (document.querySelector(`style[data-href="${relPath}"], link[data-href="${relPath}"]`)) {
    return;
  }
  const url = resolveAsset(relPath);
  const baseDir = relPath.includes("/") ? relPath.replace(/[^/]+$/, "") : "";
  try {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    let css = await response.text();
    css = css.replace(/url\(([^)]+)\)/g, (match, raw) => {
      const token = String(raw).trim().replace(/^['"]|['"]$/g, "");
      if (!token || /^(data:|blob:|https?:|asset:)/i.test(token)) return match;
      // Already absolute after a previous rewrite
      if (token.includes("asset://") || token.includes("asset.localhost")) {
        const recovered =
          token.includes("asset://")
            ? token.slice(token.indexOf("asset://"))
            : token;
        return `url(${JSON.stringify(recovered)})`;
      }
      const rel = token.startsWith("/") ? token.slice(1) : baseDir + token;
      return `url(${JSON.stringify(resolveAsset(rel))})`;
    });
    const style = document.createElement("style");
    style.dataset.href = relPath;
    style.textContent = css;
    document.head.appendChild(style);
  } catch {
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.href = url;
    link.dataset.href = relPath;
    document.head.appendChild(link);
  }
}

async function tryLoadUpstream() {
  setStatus(`加载 markdown-viewer-extension ${UPSTREAM_VERSION}…`);
  // Shim must run before engine scripts.
  await loadScript("chrome-shim.js");
  // Fail-fast: don't wait on multi‑MB core if custom element never appears.
  await Promise.all([
    loadStylesheet("vendor/engine/ui/styles.css"),
    loadScript("vendor/engine/core/element-runtime.js"),
  ]);
  await loadScript("vendor/engine/core/element-runtime-main.js");

  if (!customElements.get("markdown-viewer")) {
    await Promise.race([
      customElements.whenDefined("markdown-viewer"),
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error("markdown-viewer element not defined (2.5s)")), 2500),
      ),
    ]);
  }

  // Smoke-test with a short timeout — incomplete chrome APIs fail fast.
  const probe = document.createElement("markdown-viewer");
  probe.setAttribute("mode", "inline");
  probe.style.cssText = "position:absolute;left:-9999px;width:1px;height:1px;overflow:hidden";
  document.body.appendChild(probe);
  try {
    if (typeof probe.render !== "function") {
      throw new Error("markdown-viewer.render is not a function");
    }
    await Promise.race([
      probe.render("# probe\n\nok"),
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error("upstream render timed out (2s)")), 2000),
      ),
    ]);
    const text = (probe.textContent || "").replace(/\s+/g, " ").trim();
    if (text.length < 2) {
      throw new Error("upstream render produced empty content (chrome APIs incomplete)");
    }
  } finally {
    probe.remove();
  }

  engine.mode = "upstream";
}

/** Core parser only (~44KB). Mermaid / KaTeX load on demand. */
async function loadFallbackCore() {
  setStatus("加载渲染引擎…");
  await loadScript("vendor/fallback/marked.umd.js");
  engine.marked = window.marked;
  if (!engine.marked) throw new Error("marked failed to load");
  if (typeof engine.marked.setOptions === "function") {
    engine.marked.setOptions({ gfm: true, breaks: false });
  }
  engine.mode = "fallback";
}

async function ensureKatex() {
  if (engine.katex) return;
  if (engine.katexLoading) {
    await engine.katexLoading;
    return;
  }
  engine.katexLoading = (async () => {
    await loadStylesheet("vendor/fallback/katex.min.css");
    await loadScript("vendor/fallback/katex.min.js");
    engine.katex = window.katex;
  })();
  try {
    await engine.katexLoading;
  } finally {
    engine.katexLoading = null;
  }
}

async function ensureMermaid() {
  if (engine.mermaid) return;
  if (engine.mermaidLoading) {
    await engine.mermaidLoading;
    return;
  }
  engine.mermaidLoading = (async () => {
    await loadScript("vendor/fallback/mermaid.min.js");
    engine.mermaid = window.mermaid;
    if (engine.mermaid) {
      engine.mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        theme: "base",
        fontFamily: "system-ui, sans-serif",
      });
    }
  })();
  try {
    await engine.mermaidLoading;
  } finally {
    engine.mermaidLoading = null;
  }
}

function documentNeedsMath(markdown) {
  // $$...$$ or $...$ (single-line inline)
  return /\$\$[\s\S]+?\$\$|(?<!\$)\$(?!\$)[^$\n]+?\$(?!\$)/.test(markdown);
}

function documentNeedsMermaid(markdown) {
  return /```[ \t]*mermaid\b/i.test(markdown) || /~~~[ \t]*mermaid\b/i.test(markdown);
}

async function ensureEngine(options = {}) {
  if (engine.mode) return;
  if (engine.loading) {
    await engine.loading;
    return;
  }
  const wantUpstream = options.engine === "upstream" || TRY_UPSTREAM_BY_DEFAULT;
  engine.loading = (async () => {
    if (wantUpstream) {
      try {
        await tryLoadUpstream();
        return;
      } catch (upstreamError) {
        console.warn("[portico] upstream 5.2.0 engine failed, using fallback", upstreamError);
      }
    }
    await loadFallbackCore();
  })();
  try {
    await engine.loading;
  } catch (error) {
    engine.loading = null;
    throw error;
  }
}

function protectMath(markdown) {
  const slots = [];
  let text = markdown.replace(/\$\$([\s\S]+?)\$\$/g, (m) => {
    const i = slots.length;
    slots.push({ raw: m, display: true });
    return `<!--PORTICO_MATH_${i}-->`;
  });
  text = text.replace(/(?<!\$)\$(?!\$)([^$\n]+?)\$(?!\$)/g, (m) => {
    const i = slots.length;
    slots.push({ raw: m, display: false });
    return `<!--PORTICO_MATH_${i}-->`;
  });
  return { text, slots };
}

function renderMathSlots(html, slots) {
  if (!slots.length || !engine.katex) return html;
  return html.replace(/<!--PORTICO_MATH_(\d+)-->/g, (_m, idx) => {
    const slot = slots[Number(idx)];
    if (!slot) return _m;
    const tex = slot.display
      ? slot.raw.replace(/^\$\$/, "").replace(/\$\$$/, "").trim()
      : slot.raw.replace(/^\$/, "").replace(/\$$/, "").trim();
    try {
      const rendered = engine.katex.renderToString(tex, {
        displayMode: slot.display,
        throwOnError: false,
        strict: "ignore",
      });
      return slot.display
        ? `<div class="katex-display">${rendered}</div>`
        : `<span class="katex-inline">${rendered}</span>`;
    } catch {
      return escapeHtml(slot.raw);
    }
  });
}

/**
 * Portico bridge: dense Mermaid diagrams get natural size + scroll + lightbox.
 * Lives in provider.js so reinstall/upgrade always re-applies (not vendor/engine).
 */
function enhanceMermaidWrap(wrap, svgHtml) {
  wrap.className = "mermaid-wrap diagram-zoom-shell";
  wrap.innerHTML = "";
  const scroll = document.createElement("div");
  scroll.className = "diagram-zoom-scroll";
  scroll.innerHTML = svgHtml;
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "diagram-zoom-open-btn";
  btn.title = "放大查看";
  btn.setAttribute("aria-label", "放大查看图表");
  btn.innerHTML = "<span aria-hidden=\"true\">⛶</span><span>放大</span>";
  btn.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    openDiagramLightbox(svgHtml, "Mermaid 图表");
  });
  wrap.appendChild(scroll);
  wrap.appendChild(btn);
}

function measureSvgSize(svg) {
  const attrW = Number.parseFloat(svg.getAttribute("width") || "");
  const attrH = Number.parseFloat(svg.getAttribute("height") || "");
  if (Number.isFinite(attrW) && Number.isFinite(attrH) && attrW > 1 && attrH > 1) {
    return { w: attrW, h: attrH };
  }
  const vb = svg.viewBox && svg.viewBox.baseVal;
  if (vb && vb.width > 1 && vb.height > 1) {
    return { w: vb.width, h: vb.height };
  }
  try {
    const box = svg.getBBox();
    if (box.width > 1 && box.height > 1) return { w: box.width, h: box.height };
  } catch {
    // not rendered
  }
  const rect = svg.getBoundingClientRect();
  return {
    w: rect.width > 1 ? rect.width : 800,
    h: rect.height > 1 ? rect.height : 600,
  };
}

/** Same heuristic as Built-in diagram-lightbox (readable default, not 1×). */
function computeComfortableScale(sw, sh, vw, vh) {
  const MIN = 0.25;
  const MAX = 20;
  if (sw < 1 || sh < 1 || vw < 1 || vh < 1) return 3;
  const focusW = Math.min(sw, Math.max(240, Math.min(400, vw * 0.24)));
  const focusH = Math.min(sh, Math.max(180, Math.min(320, vh * 0.28)));
  let scale = Math.min((vw * 0.92) / focusW, (vh * 0.9) / focusH);
  if (sw <= vw * 0.95 && sh <= vh * 0.95) {
    scale = Math.min((vw * 0.9) / sw, (vh * 0.88) / sh);
  } else {
    scale = Math.max(scale, 4);
  }
  return Math.min(MAX, Math.max(MIN, scale));
}

function openDiagramLightbox(svgHtml, title) {
  closeDiagramLightbox();
  const root = document.createElement("div");
  root.className = "diagram-lightbox";
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-modal", "true");
  root.dataset.porticoLightbox = "1";

  let scale = 1;
  let baseScale = 1;
  let tx = 0;
  let ty = 0;
  let drag = null;

  const header = document.createElement("header");
  header.className = "diagram-lightbox-header";
  header.innerHTML = `<p class="diagram-lightbox-title">${escapeHtml(title)}</p>`;
  const tools = document.createElement("div");
  tools.className = "diagram-lightbox-tools";

  const pct = document.createElement("span");
  pct.className = "diagram-lightbox-pct";
  pct.textContent = "100%";

  function applyTransform() {
    stage.style.transform = `translate(calc(-50% + ${tx}px), calc(-50% + ${ty}px)) scale(${scale})`;
    pct.textContent = `${Math.round(scale * 100)}%`;
  }

  function zoomBy(delta) {
    scale = Math.min(20, Math.max(0.25, scale + delta));
    applyTransform();
  }

  function resetView() {
    scale = baseScale;
    tx = 0;
    ty = 0;
    applyTransform();
  }

  function applyComfortable() {
    const svg = stage.querySelector("svg");
    if (!svg) {
      baseScale = 3;
      scale = 3;
      applyTransform();
      return;
    }
    const { w, h } = measureSvgSize(svg);
    baseScale = computeComfortableScale(w, h, viewport.clientWidth, viewport.clientHeight);
    scale = baseScale;
    tx = 0;
    ty = 0;
    applyTransform();
  }

  for (const [label, action, aria] of [
    ["−", () => zoomBy(-0.25), "缩小"],
    ["+", () => zoomBy(0.25), "放大"],
    ["↺", resetView, "重置到合适倍率"],
    ["×", closeDiagramLightbox, "关闭"],
  ]) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "diagram-lightbox-tool";
    b.textContent = label;
    b.setAttribute("aria-label", aria);
    b.addEventListener("click", action);
    if (label === "−") {
      tools.appendChild(b);
      tools.appendChild(pct);
    } else {
      tools.appendChild(b);
    }
  }
  header.appendChild(tools);

  const viewport = document.createElement("div");
  viewport.className = "diagram-lightbox-viewport";
  const stage = document.createElement("div");
  stage.className = "diagram-lightbox-stage";
  stage.innerHTML = svgHtml;
  viewport.appendChild(stage);

  viewport.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      zoomBy(event.deltaY > 0 ? -0.25 : 0.25);
    },
    { passive: false },
  );
  viewport.addEventListener("pointerdown", (event) => {
    viewport.setPointerCapture(event.pointerId);
    drag = { x: event.clientX, y: event.clientY, tx, ty };
  });
  viewport.addEventListener("pointermove", (event) => {
    if (!drag) return;
    tx = drag.tx + (event.clientX - drag.x);
    ty = drag.ty + (event.clientY - drag.y);
    applyTransform();
  });
  const endDrag = (event) => {
    drag = null;
    try {
      viewport.releasePointerCapture(event.pointerId);
    } catch {
      // ignore
    }
  };
  viewport.addEventListener("pointerup", endDrag);
  viewport.addEventListener("pointercancel", endDrag);

  const footer = document.createElement("p");
  footer.className = "diagram-lightbox-hint";
  footer.textContent = "已自动缩放到合适倍率 · 滚轮缩放 · 拖拽平移 · Esc 关闭 · 0 重置";

  root.appendChild(header);
  root.appendChild(viewport);
  root.appendChild(footer);
  document.body.appendChild(root);

  // Layout then choose a readable initial scale (not 1×).
  requestAnimationFrame(() => applyComfortable());

  root._onKey = (event) => {
    if (event.key === "Escape") closeDiagramLightbox();
    if (event.key === "+" || event.key === "=") zoomBy(0.25);
    if (event.key === "-" || event.key === "_") zoomBy(-0.25);
    if (event.key === "0") resetView();
  };
  window.addEventListener("keydown", root._onKey);
}

function closeDiagramLightbox() {
  const existing = document.querySelector("[data-portico-lightbox='1']");
  if (!existing) return;
  if (existing._onKey) window.removeEventListener("keydown", existing._onKey);
  existing.remove();
}

async function renderMermaidIn(root) {
  if (!engine.mermaid) return;
  const blocks = [
    ...root.querySelectorAll("pre > code.language-mermaid, pre > code[class*='mermaid']"),
  ];
  // Render diagrams concurrently (mermaid supports parallel render with unique ids).
  await Promise.all(
    blocks.map(async (code, i) => {
      const pre = code.parentElement;
      if (!pre) return;
      const source = (code.textContent || "").trim();
      const wrap = document.createElement("div");
      wrap.className = "mermaid-wrap";
      try {
        const id = `mmd-${Date.now()}-${i}`;
        const { svg } = await engine.mermaid.render(id, source);
        enhanceMermaidWrap(wrap, svg);
      } catch (error) {
        wrap.textContent = `Mermaid: ${error instanceof Error ? error.message : String(error)}`;
      }
      pre.replaceWith(wrap);
    }),
  );
}

function mountVersionPill(modeLabel) {
  const pill = document.createElement("div");
  pill.className = "doc-version-pill";
  pill.innerHTML = `<span title="Plugin package version">v${UPSTREAM_VERSION}${modeLabel ? ` · ${modeLabel}` : ""}</span>`;
  return pill;
}

async function renderWithUpstream(markdown) {
  documentRoot.innerHTML = "";
  documentRoot.appendChild(mountVersionPill("engine"));
  viewerEl = document.createElement("markdown-viewer");
  viewerEl.setAttribute("mode", "inline");
  viewerEl.id = "mv-root";
  documentRoot.appendChild(viewerEl);

  await Promise.race([
    viewerEl.render(String(markdown ?? "")),
    new Promise((_, reject) =>
      setTimeout(() => reject(new Error("upstream render timeout 12s")), 12_000),
    ),
  ]);

  const text = (viewerEl.textContent || "").replace(/\s+/g, " ").trim();
  if (text.length < 2 && String(markdown || "").trim().length > 0) {
    throw new Error("upstream render empty");
  }
  return documentRoot.innerHTML;
}

async function renderWithFallback(markdown) {
  const source = String(markdown ?? "").replace(/\r\n/g, "\n");
  const needsMath = documentNeedsMath(source);
  const needsMermaid = documentNeedsMermaid(source);

  // Core parser + optional heavy deps in parallel (mermaid is ~3.4MB when needed).
  await Promise.all([
    engine.mode === "fallback" || engine.marked
      ? Promise.resolve()
      : loadFallbackCore(),
    needsMath ? ensureKatex() : Promise.resolve(),
    needsMermaid ? ensureMermaid() : Promise.resolve(),
  ]);

  if (!engine.marked) throw new Error("marked failed to load");

  const { text, slots } = protectMath(source);
  let html = engine.marked.parse(text);
  if (needsMath) html = renderMathSlots(html, slots);

  documentRoot.innerHTML = "";
  documentRoot.appendChild(mountVersionPill("fallback"));
  const body = document.createElement("div");
  body.className = "doc-body";
  body.innerHTML = html;
  documentRoot.appendChild(body);

  if (needsMermaid) {
    await renderMermaidIn(documentRoot);
  }
  return documentRoot.innerHTML;
}

async function renderMarkdown(markdown, options = {}) {
  if (options.theme === "compact") documentRoot.classList.add("theme-compact");
  else documentRoot.classList.remove("theme-compact");

  const wantUpstream = options.engine === "upstream" || TRY_UPSTREAM_BY_DEFAULT;
  // Default path: fallback only — skip multi‑MB chrome extension boot.
  if (wantUpstream) {
    await ensureEngine(options);
    if (engine.mode === "upstream") {
      try {
        return await renderWithUpstream(markdown);
      } catch (error) {
        console.warn("[portico] upstream render failed, switching to fallback", error);
        engine.mode = null;
        engine.loading = null;
      }
    }
  }
  return renderWithFallback(markdown);
}

function exportHtmlDocument() {
  return `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>export</title></head><body>${documentRoot.outerHTML}</body></html>`;
}

function toBase64(value) {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary);
}

function postToHost(payload) {
  const targets = [];
  try {
    if (window.parent && window.parent !== window) targets.push(window.parent);
  } catch {
    // ignore
  }
  try {
    if (window.top && window.top !== window && !targets.includes(window.top)) {
      targets.push(window.top);
    }
  } catch {
    // ignore
  }
  if (targets.length === 0) targets.push(window.parent);
  for (const target of targets) {
    try {
      target.postMessage(payload, "*");
    } catch {
      // ignore
    }
  }
}

function announceReady() {
  postToHost({ protocol, type: "ready" });
}

addEventListener("message", (event) => {
  const request = event.data;
  if (!request || request.protocol !== protocol) return;
  if (request.type === "ping") {
    announceReady();
    return;
  }
  if (typeof request.id !== "string") return;

  void (async () => {
    try {
      if (request.type === "render") {
        setStatus("正在渲染…");
        const html = await renderMarkdown(request.markdown, request.options || {});
        postToHost({
          protocol,
          type: "result",
          id: request.id,
          result: { kind: "html", html },
        });
      } else if (request.type === "export" && request.format === "html") {
        await renderMarkdown(request.markdown, {});
        postToHost({
          protocol,
          type: "result",
          id: request.id,
          result: {
            kind: "file",
            mimeType: "text/html",
            filename: "document.html",
            contentBase64: toBase64(exportHtmlDocument()),
          },
        });
      } else {
        throw new Error(`Unsupported request: ${request.type} ${request.format || ""}`);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      documentRoot.innerHTML = `<div class="engine-error">Markdown Viewer ${UPSTREAM_VERSION} 渲染失败:\n${escapeHtml(message)}</div>`;
      postToHost({
        protocol,
        type: "error",
        id: request.id,
        error: { code: "provider_error", message },
      });
    }
  })();
});

// Ready immediately — engine loads on first render (avoids competing with host
// handshake and prevents multi-second upstream boot on every open).
setStatus(`v${UPSTREAM_VERSION} 就绪`);
announceReady();
let readyBurst = 0;
const readyTimer = setInterval(() => {
  announceReady();
  readyBurst += 1;
  if (readyBurst >= 20) clearInterval(readyTimer);
}, 100);
addEventListener("load", () => announceReady());
