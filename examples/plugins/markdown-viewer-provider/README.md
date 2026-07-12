# Markdown Viewer Provider

Portico **Markdown Provider** package powered by upstream
**[markdown-viewer-extension](https://github.com/markdown-viewer/markdown-viewer-extension) v5.2.0**.

| Field | Value |
|-------|--------|
| Plugin version | **5.2.0** (same as upstream release) |
| Engine | `vendor/engine/` = Chrome unpack build of upstream |
| License | **GPL-3.0** (`LICENSE-GPL-3.0`) — third-party package, not MIT core |
| Protocol | `portico.markdown-provider/v1` |

## Install

**能力中心 → 插件 → Markdown Viewer → 重新安装 / 安装**

Or drop this folder under `~/.portico/plugins/available/` and install.

After install the catalog badge should show **`v5.2.0`**.

## Capabilities

Uses the full upstream offline engine (Mermaid, KaTeX, diagrams, themes, …)
via the `<markdown-viewer>` custom element.

## Rebuild engine from upstream

See [UPSTREAM.md](./UPSTREAM.md).
