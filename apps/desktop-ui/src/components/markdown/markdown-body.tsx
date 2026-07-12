import { useMemo } from "react";
import type { Components } from "react-markdown";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import hljs from "highlight.js/lib/core";
import javascript from "highlight.js/lib/languages/javascript";
import typescript from "highlight.js/lib/languages/typescript";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import json from "highlight.js/lib/languages/json";
import bash from "highlight.js/lib/languages/bash";
import xml from "highlight.js/lib/languages/xml";
import css from "highlight.js/lib/languages/css";
import markdown from "highlight.js/lib/languages/markdown";
import "katex/dist/katex.min.css";
import "highlight.js/styles/github.css";
import { cn } from "@/lib/utils";
import { MermaidBlock } from "./mermaid-block";
import {
  escapeHtml,
  languageFromClassName,
  looksLikeAsciiDiagram,
  shouldRenderAsPlainCode,
} from "./markdown-code";

// Register once for the process lifetime.
let languagesRegistered = false;
function ensureHighlightLanguages() {
  if (languagesRegistered) return;
  hljs.registerLanguage("javascript", javascript);
  hljs.registerLanguage("js", javascript);
  hljs.registerLanguage("typescript", typescript);
  hljs.registerLanguage("ts", typescript);
  hljs.registerLanguage("python", python);
  hljs.registerLanguage("py", python);
  hljs.registerLanguage("rust", rust);
  hljs.registerLanguage("rs", rust);
  hljs.registerLanguage("json", json);
  hljs.registerLanguage("bash", bash);
  hljs.registerLanguage("shell", bash);
  hljs.registerLanguage("sh", bash);
  hljs.registerLanguage("xml", xml);
  hljs.registerLanguage("html", xml);
  hljs.registerLanguage("css", css);
  hljs.registerLanguage("markdown", markdown);
  hljs.registerLanguage("md", markdown);
  languagesRegistered = true;
}

interface MarkdownBodyProps {
  content: string;
  className?: string;
  /** Compact padding for conversation bubbles. */
  compact?: boolean;
  /**
   * View-layer presentation tokens (font scale, document chrome, etc.).
   * Does not change source content — only how it is displayed.
   */
  presentationClassName?: string;
}

/**
 * Highlight only when the fence declares a known language.
 * Never run highlightAuto — it mangles ASCII architecture diagrams.
 */
function highlightCode(code: string, language: string): string {
  ensureHighlightLanguages();
  if (!hljs.getLanguage(language)) {
    return escapeHtml(code);
  }
  try {
    return hljs.highlight(code, { language }).value;
  } catch {
    return escapeHtml(code);
  }
}

/**
 * Single product Markdown renderer for artifacts, workspace preview fallback,
 * and conversation assistant messages.
 *
 * Feature set:
 * - GFM (tables, task lists, strikethrough, autolinks)
 * - Math via KaTeX (`$...$` / `$$...$$`)
 * - Mermaid diagrams in fenced ` ```mermaid ` blocks
 * - Plain monospaced fences for ASCII architecture diagrams (no auto-highlight)
 * - Syntax highlighting only for explicit, known languages
 *
 * Security: raw HTML is not enabled; remote images are not loaded.
 */
export function MarkdownBody({
  content,
  className,
  compact = false,
  presentationClassName,
}: MarkdownBodyProps) {
  const components = useMemo<Components>(
    () => ({
      a: ({ children, href }) => (
        <span className="underline underline-offset-2" title={href}>
          {children}
        </span>
      ),
      img: ({ alt, src }) =>
        src?.startsWith("http://") || src?.startsWith("https://") ? (
          <span className="text-muted-foreground">[{alt || "remote image"}]</span>
        ) : (
          <img src={src} alt={alt || ""} />
        ),
      // react-markdown nests fenced blocks as pre>code. Unwrap pre so block
      // handlers own the surface (mermaid cannot live inside a code pre).
      pre: ({ children }) => <>{children}</>,
      code: ({ className: codeClassName, children }) => {
        const language = languageFromClassName(codeClassName);
        const text = String(children).replace(/\n$/, "");
        const isBlock = Boolean(codeClassName) || text.includes("\n");

        if (!isBlock) {
          return <code className={codeClassName}>{children}</code>;
        }

        if (language === "mermaid") {
          return <MermaidBlock chart={text} />;
        }

        const plain = shouldRenderAsPlainCode(text, language);
        const ascii = looksLikeAsciiDiagram(text);

        if (plain) {
          return (
            <pre className={cn("markdown-code-block", ascii && "markdown-ascii-diagram")}>
              <code className={codeClassName}>{text}</code>
            </pre>
          );
        }

        const highlighted = highlightCode(text, language!);
        return (
          <pre className="markdown-code-block">
            <code
              className={cn(codeClassName, "hljs")}
              dangerouslySetInnerHTML={{ __html: highlighted }}
            />
          </pre>
        );
      },
    }),
    [],
  );

  return (
    <article
      className={cn(
        "markdown-preview text-sm",
        compact ? "p-3" : "rounded-md border p-4",
        presentationClassName,
        className,
      )}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={components}
      >
        {content}
      </ReactMarkdown>
    </article>
  );
}
