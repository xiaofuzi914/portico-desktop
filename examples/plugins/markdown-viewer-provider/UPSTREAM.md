# Upstream engine + Portico bridge

| Field | Value |
|-------|--------|
| Project | [markdown-viewer/markdown-viewer-extension](https://github.com/markdown-viewer/markdown-viewer-extension) |
| Version | from upstream `package.json` (currently **5.2.0**) |
| Built as | Chrome extension unpack (`dist/chrome` via esbuild) |
| License | **GPL-3.0** — see `LICENSE-GPL-3.0` |
| Location | `vendor/engine/` |

This Portico plugin is a **third-party package** (not MIT core). The host only loads it through the sandboxed Markdown Provider protocol.

## Layered package (upgrade-safe)

```
markdown-viewer-provider/
├── plugin.json              # version = upstream package.json
├── index.html               # Portico shell + meta portico-plugin-version
├── provider.js              # Portico bridge (STABLE — do not replace with upstream)
├── chrome-shim.js           # chrome.* stubs for srcdoc (STABLE)
├── styles.css               # preview scroll/layout (STABLE)
├── vendor/
│   ├── fallback/            # marked / mermaid / katex — default render path (STABLE)
│   └── engine/              # upstream dist/chrome — OPTIONAL / versioned only
└── UPSTREAM.md
```

| Layer | On upgrade | Responsibility |
|-------|------------|----------------|
| **Portico bridge** (`provider.js`, `chrome-shim.js`, `styles.css`, `index.html`, `vendor/fallback`) | **Keep / overwrite only from Portico monorepo template** | Compatibility, performance, host protocol |
| **Upstream engine** (`vendor/engine/**`) | **Replace** with new `dist/chrome` | Optional full chrome extension runtime |
| **Version metadata** (`plugin.json`, `<meta name="portico-plugin-version">`) | **Rewrite** from upstream `package.json` | UI version badge / capability center |

### Performance defaults (must survive upgrades)

1. Default render path is **fallback** (`TRY_UPSTREAM_BY_DEFAULT = false` in `provider.js`).
2. **Do not** boot the multi‑MB upstream engine on every open.
3. Mermaid / KaTeX load **only when** the document needs them.
4. Asset resolution must never double-join `asset://` URLs (`chrome-shim` + `__PORTICO_RESOLVE__`).
5. **Diagram zoom/lightbox** for dense Mermaid lives in bridge `provider.js` + `styles.css`
   (`.diagram-zoom-shell`, lightbox). Never put this only under `vendor/engine` — upgrades
   would wipe it. Host Built-in preview has a parallel React implementation.

Install/reinstall pipelines (`plugin_github.rs`, `scripts/build-markdown-viewer-plugin.sh`) always re-apply the Portico bridge after swapping `vendor/engine`.

## Rebuild / upgrade

```bash
# Bump vendor/engine + plugin version only; bridge files stay owned by Portico.
./scripts/build-markdown-viewer-plugin.sh

# Or reinstall from catalog / GitHub in the app:
# 能力中心 → 插件 → 重新安装 Markdown Viewer
```

After rebuild, capability center should show the new upstream version, while badge stays `vX.Y.Z · fallback` (fast path) unless `options.engine === "upstream"`.
