import { useEffect, useId, useState } from "react";
import { DiagramZoomShell } from "./diagram-lightbox";

interface MermaidBlockProps {
  chart: string;
}

/**
 * Renders a Mermaid diagram client-side. Dense diagrams use scroll + lightbox
 * zoom (see DiagramZoomShell) so architecture graphs stay readable.
 */
export function MermaidBlock({ chart }: MermaidBlockProps) {
  const reactId = useId().replace(/:/g, "");
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const source = chart.trim();
    if (!source) {
      setSvg(null);
      setError(null);
      return;
    }

    void (async () => {
      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({
          startOnLoad: false,
          securityLevel: "strict",
          theme: "neutral",
          fontFamily: "inherit",
        });
        const id = `mermaid-${reactId}-${Math.random().toString(36).slice(2, 9)}`;
        const { svg: rendered } = await mermaid.render(id, source);
        if (!cancelled) {
          setSvg(rendered);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setSvg(null);
          setError(err instanceof Error ? err.message : String(err));
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [chart, reactId]);

  if (error) {
    return (
      <pre className="markdown-mermaid-error">
        <code>{`Mermaid render failed: ${error}\n\n${chart}`}</code>
      </pre>
    );
  }

  if (!svg) {
    return (
      <pre className="markdown-mermaid-loading">
        <code>{chart}</code>
      </pre>
    );
  }

  return <DiagramZoomShell className="markdown-mermaid" svgHtml={svg} title="Mermaid 图表" />;
}
