/**
 * Minimal Chrome extension API stubs so markdown-viewer-extension's
 * element-runtime can boot inside Portico's sandboxed srcdoc (no chrome.*).
 *
 * getURL maps extension-relative paths to Portico asset URLs under vendor/engine/.
 * Absolute asset:/http(s):/data: URLs are returned unchanged (no double-resolve).
 */
(function installChromeShim() {
  if (typeof globalThis.chrome !== "undefined" && globalThis.chrome.runtime?.id) {
    return; // real extension context
  }

  function isAbsoluteUrl(value) {
    return /^(data:|blob:|asset:|https?:|tauri:|chrome-extension:|chrome:)/i.test(
      String(value || ""),
    );
  }

  /** If a prior resolve was incorrectly re-prefixed, peel back to asset://… */
  function recoverEmbeddedAssetUrl(value) {
    const s = String(value || "");
    const markers = ["asset://", "http://asset.localhost", "https://asset.localhost"];
    for (const marker of markers) {
      const idx = s.indexOf(marker);
      if (idx > 0) return s.slice(idx);
    }
    return null;
  }

  function resolve(rel) {
    const raw = String(rel ?? "");
    if (!raw) {
      // chrome.runtime.getURL("") / getURL("/") → engine package root
      return resolve("vendor/engine/");
    }
    if (isAbsoluteUrl(raw)) return raw;
    const recovered = recoverEmbeddedAssetUrl(raw);
    if (recovered) return recovered;

    const cleaned = raw.replace(/^\//, "").replace(/^\.\//, "");
    const path = cleaned.startsWith("vendor/engine/")
      ? cleaned
      : `vendor/engine/${cleaned}`;

    if (typeof globalThis.__PORTICO_RESOLVE__ === "function") {
      return globalThis.__PORTICO_RESOLVE__(path);
    }
    return path;
  }

  const storageData = Object.create(null);
  const storageArea = {
    get(keys, cb) {
      const out = {};
      if (keys == null) {
        Object.assign(out, storageData);
      } else if (typeof keys === "string") {
        if (keys in storageData) out[keys] = storageData[keys];
      } else if (Array.isArray(keys)) {
        for (const k of keys) if (k in storageData) out[k] = storageData[k];
      } else if (typeof keys === "object") {
        for (const k of Object.keys(keys)) {
          out[k] = k in storageData ? storageData[k] : keys[k];
        }
      }
      const p = Promise.resolve(out);
      if (typeof cb === "function") cb(out);
      return p;
    },
    set(items, cb) {
      Object.assign(storageData, items || {});
      const p = Promise.resolve();
      if (typeof cb === "function") cb();
      return p;
    },
    remove(keys, cb) {
      const list = Array.isArray(keys) ? keys : [keys];
      for (const k of list) delete storageData[k];
      const p = Promise.resolve();
      if (typeof cb === "function") cb();
      return p;
    },
  };

  const onMessageListeners = [];
  const runtime = {
    id: "portico-markdown-viewer-shim",
    getURL: (path) => resolve(path),
    sendMessage: (msg, cb) => {
      // No background page — resolve empty / no-op so callers do not hang forever.
      const result = undefined;
      if (typeof cb === "function") {
        try {
          cb(result);
        } catch {
          // ignore
        }
      }
      return Promise.resolve(result);
    },
    onMessage: {
      addListener: (fn) => {
        onMessageListeners.push(fn);
      },
      removeListener: (fn) => {
        const i = onMessageListeners.indexOf(fn);
        if (i >= 0) onMessageListeners.splice(i, 1);
      },
    },
    lastError: undefined,
  };

  const i18n = {
    getMessage: (key) => key || "",
    getUILanguage: () =>
      (typeof navigator !== "undefined" && navigator.language) || "en",
  };

  globalThis.chrome = {
    runtime,
    storage: {
      local: storageArea,
      sync: storageArea,
      session: storageArea,
      onChanged: { addListener() {}, removeListener() {} },
    },
    i18n,
    tabs: {
      query: async () => [],
      sendMessage: async () => undefined,
    },
  };

  // webextension-polyfill style used by some bundles
  if (typeof globalThis.browser === "undefined") {
    globalThis.browser = globalThis.chrome;
  }

  console.info("[portico] chrome API shim installed for markdown-viewer-extension");
})();
