# Portico plugins

## Closed-loop install from GitHub

Registry: **[`sources.json`](./sources.json)**

Portico can install plugins without manual folder picking:

1. **Catalog one-click** — packages listed in `sources.json` + UI catalog.
2. **GitHub URL** — paste `https://github.com/owner/repo` in 能力中心 → 插件 →「从 GitHub 安装」.
3. The app **clones** into `~/.portico/plugins/cache/github/`, **builds** known recipes, **writes** a Portico `plugin.json` package, and **installs** under `~/.portico/plugins/installed/`.

### Source kinds

| `kind` | Behavior |
|--------|----------|
| `portico-package` | Copy monorepo folder (`package_path`) that already has `plugin.json` |
| `markdown-viewer-extension` | Clone GitHub repo → npm/esbuild Chrome dist → **re-apply Portico bridge** → version from upstream `package.json` |
| `auto` (raw URL) | If root has `plugin.json`, install as-is; else detect markdown-viewer monorepo layout |

### Markdown Viewer upgrade contract

Upgrading upstream (5.2.0 → later) **must not** regress Portico performance/compat:

| Keep (Portico-owned) | Replace (upstream) |
|----------------------|--------------------|
| `provider.js`, `chrome-shim.js`, `styles.css`, `index.html` | `vendor/engine/**` |
| `vendor/fallback/**` (default fast render) | `plugin.json` version / display name |

Default render stays **fallback** (lazy Mermaid/KaTeX). Full chrome engine is optional.
See [`markdown-viewer-provider/UPSTREAM.md`](./markdown-viewer-provider/UPSTREAM.md).

### Example

```text
https://github.com/markdown-viewer/markdown-viewer-extension
→ version 5.2.0 (from that repo)
→ capability center shows v5.2.0
→ bridge + vendor/fallback always re-applied from Portico template
```

Requirements on the machine for GitHub recipes: `git`, `node`, `npm`.
