# Markdown Rich Provider

Reference Portico Markdown Provider package. In Portico, open **Capabilities → Plugins → Choose plugin folder** and select this directory.

It demonstrates sandboxed preview and HTML export. Portico supplies DOCX and print-to-PDF fallback export when a provider does not implement those capabilities.

## Rendering source of truth

**Prefer Portico “Built-in (matches chat)”** for preview that matches conversation messages and aligns with docu.md-style features:

- GitHub Flavored Markdown (tables, task lists, …)
- KaTeX math (`$...$` / `$$...$$`)
- Live Mermaid diagrams

This plugin is a protocol sample only. It does **not** fully reimplement Portico’s built-in engine. Switching to an external provider intentionally allows divergent rendering.
