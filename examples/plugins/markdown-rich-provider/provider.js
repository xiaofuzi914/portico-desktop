/**
 * Reference Markdown Provider — intentionally lightweight and offline.
 *
 * Portico's built-in renderer (chat + default workspace preview) is the
 * product source of truth for GFM / KaTeX / Mermaid. This package only
 * demonstrates the sandboxed provider protocol. Prefer "Built-in (matches chat)"
 * in the preview toolbar for docu.md-aligned rendering.
 */
const protocol = "portico.markdown-provider/v1";
const documentRoot = document.getElementById("document");

const escapeHtml = (value) =>
  value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");

const inline = (value) =>
  value
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\*([^*]+)\*/g, "<em>$1</em>")
    .replace(
      /\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g,
      '<a href="$2" target="_blank" rel="noreferrer">$1</a>',
    )
    // Lightweight math placeholders — not KaTeX. Use Built-in for real math.
    .replace(/\$\$([^$]+)\$\$/g, '<span class="math-block">$1</span>')
    .replace(/\$([^$\n]+)\$/g, '<span class="math-inline">$1</span>');

function renderTable(block) {
  const rows = block
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  if (rows.length < 2 || !rows.every((line) => line.includes("|"))) return null;
  const cells = (line) =>
    line
      .replace(/^\|/, "")
      .replace(/\|$/, "")
      .split("|")
      .map((cell) => cell.trim());
  const header = cells(rows[0]);
  const sep = rows[1];
  if (!/^\|?[\s:-]+\|/.test(sep) && !/^[\s|:-]+$/.test(sep)) return null;
  const body = rows.slice(2).map(cells);
  return `<table><thead><tr>${header
    .map((cell) => `<th>${inline(escapeHtml(cell))}</th>`)
    .join("")}</tr></thead><tbody>${body
    .map(
      (row) =>
        `<tr>${row.map((cell) => `<td>${inline(escapeHtml(cell))}</td>`).join("")}</tr>`,
    )
    .join("")}</tbody></table>`;
}

function renderMarkdown(markdown) {
  const escaped = escapeHtml(markdown).replace(
    /```([\w-]*)\n([\s\S]*?)```/g,
    (_, language, code) => {
      if (language === "mermaid") {
        return `<div class="mermaid-placeholder"><strong>Mermaid</strong><pre><code>${code}</code></pre><p class="hint">Use Portico Built-in preview for live diagrams.</p></div>`;
      }
      return `<pre><code data-language="${language}">${code}</code></pre>`;
    },
  );
  return escaped
    .split(/\n{2,}/)
    .map((block) => {
      if (block.startsWith("<pre>") || block.startsWith('<div class="mermaid-placeholder"')) {
        return block;
      }
      const table = renderTable(block);
      if (table) return table;
      const heading = block.match(/^(#{1,6})\s+(.+)$/s);
      if (heading) {
        return `<h${heading[1].length}>${inline(heading[2])}</h${heading[1].length}>`;
      }
      if (block.startsWith("&gt; ")) {
        return `<blockquote>${inline(block.slice(5))}</blockquote>`;
      }
      const lines = block.split("\n");
      if (lines.every((line) => /^[-*]\s+/.test(line))) {
        return `<ul>${lines
          .map((line) => `<li>${inline(line.slice(2))}</li>`)
          .join("")}</ul>`;
      }
      return `<p>${inline(block).replaceAll("\n", "<br>")}</p>`;
    })
    .join("\n");
}

function toBase64(value) {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary);
}

addEventListener("message", (event) => {
  const request = event.data;
  if (!request || request.protocol !== protocol || typeof request.id !== "string") return;
  try {
    if (request.type === "render") {
      documentRoot.innerHTML = renderMarkdown(request.markdown);
      parent.postMessage(
        {
          protocol,
          type: "result",
          id: request.id,
          result: { kind: "html", html: documentRoot.innerHTML },
        },
        "*",
      );
    } else if (request.type === "export" && request.format === "html") {
      const html = `<!doctype html>${document.documentElement.outerHTML}`;
      parent.postMessage(
        {
          protocol,
          type: "result",
          id: request.id,
          result: {
            kind: "file",
            mimeType: "text/html",
            filename: "document.html",
            contentBase64: toBase64(html),
          },
        },
        "*",
      );
    } else {
      throw new Error(`Unsupported export format: ${request.format}`);
    }
  } catch (error) {
    parent.postMessage(
      {
        protocol,
        type: "error",
        id: request.id,
        error: {
          code: "provider_error",
          message: String(error.message || error),
        },
      },
      "*",
    );
  }
});

parent.postMessage({ protocol, type: "ready" }, "*");
