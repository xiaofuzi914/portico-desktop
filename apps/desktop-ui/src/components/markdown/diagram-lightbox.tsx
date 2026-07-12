import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Maximize2, Minus, Plus, RotateCcw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type Props = Readonly<{
  open: boolean;
  title?: string;
  /** SVG markup (trusted: mermaid-rendered only). */
  svgHtml: string;
  onClose: () => void;
}>;

const MIN_SCALE = 0.25;
const MAX_SCALE = 20;
const STEP = 0.25;

function measureSvgSize(svg: SVGSVGElement): { w: number; h: number } {
  const attrW = Number.parseFloat(svg.getAttribute("width") || "");
  const attrH = Number.parseFloat(svg.getAttribute("height") || "");
  if (Number.isFinite(attrW) && Number.isFinite(attrH) && attrW > 1 && attrH > 1) {
    return { w: attrW, h: attrH };
  }
  const vb = svg.viewBox?.baseVal;
  if (vb && vb.width > 1 && vb.height > 1) {
    return { w: vb.width, h: vb.height };
  }
  try {
    const box = svg.getBBox();
    if (box.width > 1 && box.height > 1) return { w: box.width, h: box.height };
  } catch {
    // not rendered yet
  }
  const rect = svg.getBoundingClientRect();
  return {
    w: rect.width > 1 ? rect.width : 800,
    h: rect.height > 1 ? rect.height : 600,
  };
}

/**
 * Default open scale: large architecture graphs start zoomed in enough to read
 * node labels (not 1× overview). Small graphs scale up to fill the viewport.
 */
export function computeComfortableScale(
  sw: number,
  sh: number,
  vw: number,
  vh: number,
): number {
  if (sw < 1 || sh < 1 || vw < 1 || vh < 1) return 3;

  // How much diagram “content width” should fill most of the viewport.
  // Smaller focus ⇒ higher initial zoom (dense Mermaid labels stay readable).
  const focusW = Math.min(sw, Math.max(240, Math.min(400, vw * 0.24)));
  const focusH = Math.min(sh, Math.max(180, Math.min(320, vh * 0.28)));
  let scale = Math.min((vw * 0.92) / focusW, (vh * 0.9) / focusH);

  // Diagram already fits the screen — grow to use the lightbox area.
  if (sw <= vw * 0.95 && sh <= vh * 0.95) {
    scale = Math.min((vw * 0.9) / sw, (vh * 0.88) / sh);
  } else {
    // Oversized graphs: open at least ~4× so labels are readable immediately.
    scale = Math.max(scale, 4);
  }

  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale));
}

/**
 * Fullscreen pan/zoom surface for dense Mermaid (and similar) diagrams.
 * Used by Built-in markdown preview; plugin iframe has a parallel implementation.
 */
export function DiagramLightbox({ open, title = "图表", svgHtml, onClose }: Props) {
  const [scale, setScale] = useState(1);
  const [tx, setTx] = useState(0);
  const [ty, setTy] = useState(0);
  const [baseScale, setBaseScale] = useState(1);
  const dragRef = useRef<{ x: number; y: number; tx: number; ty: number } | null>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);

  const applyComfortable = useCallback(() => {
    const viewport = viewportRef.current;
    const stage = stageRef.current;
    const svg = stage?.querySelector("svg");
    if (!viewport || !svg) {
      setScale(3);
      setBaseScale(3);
      setTx(0);
      setTy(0);
      return;
    }
    const { w, h } = measureSvgSize(svg);
    const next = computeComfortableScale(w, h, viewport.clientWidth, viewport.clientHeight);
    setBaseScale(next);
    setScale(next);
    setTx(0);
    setTy(0);
  }, []);

  const reset = useCallback(() => {
    setScale(baseScale);
    setTx(0);
    setTy(0);
  }, [baseScale]);

  useLayoutEffect(() => {
    if (!open) return;
    // Wait a frame so SVG is in the DOM with real metrics.
    const id = window.requestAnimationFrame(() => {
      applyComfortable();
    });
    return () => window.cancelAnimationFrame(id);
  }, [open, svgHtml, applyComfortable]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
      if (event.key === "+" || event.key === "=") {
        setScale((s) => Math.min(MAX_SCALE, s + STEP));
      }
      if (event.key === "-" || event.key === "_") {
        setScale((s) => Math.max(MIN_SCALE, s - STEP));
      }
      if (event.key === "0") reset();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose, reset]);

  if (!open) return null;

  function zoomBy(delta: number) {
    setScale((s) => Math.min(MAX_SCALE, Math.max(MIN_SCALE, s + delta)));
  }

  function onWheel(event: React.WheelEvent) {
    event.preventDefault();
    const delta = event.deltaY > 0 ? -STEP : STEP;
    zoomBy(delta);
  }

  function onPointerDown(event: React.PointerEvent) {
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    dragRef.current = { x: event.clientX, y: event.clientY, tx, ty };
  }

  function onPointerMove(event: React.PointerEvent) {
    const drag = dragRef.current;
    if (!drag) return;
    setTx(drag.tx + (event.clientX - drag.x));
    setTy(drag.ty + (event.clientY - drag.y));
  }

  function onPointerUp(event: React.PointerEvent) {
    dragRef.current = null;
    try {
      (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    } catch {
      // ignore
    }
  }

  return (
    <div
      className="diagram-lightbox fixed inset-0 z-[80] flex flex-col bg-black/55 backdrop-blur-[2px]"
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <header className="flex shrink-0 items-center gap-2 border-b border-white/10 bg-zinc-900/90 px-3 py-2 text-white">
        <p className="min-w-0 flex-1 truncate text-sm font-medium">{title}</p>
        <div className="flex items-center gap-1">
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-8 text-white hover:bg-white/10 hover:text-white"
            onClick={() => zoomBy(-STEP)}
            aria-label="缩小"
          >
            <Minus className="h-4 w-4" />
          </Button>
          <span className="w-14 text-center text-xs text-white/80 tabular-nums">
            {Math.round(scale * 100)}%
          </span>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-8 text-white hover:bg-white/10 hover:text-white"
            onClick={() => zoomBy(STEP)}
            aria-label="放大"
          >
            <Plus className="h-4 w-4" />
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-8 text-white hover:bg-white/10 hover:text-white"
            onClick={reset}
            aria-label="重置到合适倍率"
            title="重置到合适倍率"
          >
            <RotateCcw className="h-4 w-4" />
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-8 text-white hover:bg-white/10 hover:text-white"
            onClick={onClose}
            aria-label="关闭"
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </header>
      <div
        ref={viewportRef}
        className="relative min-h-0 flex-1 cursor-grab overflow-hidden active:cursor-grabbing"
        onWheel={onWheel}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <div
          ref={stageRef}
          className="diagram-lightbox-stage absolute top-1/2 left-1/2 origin-center"
          style={{
            transform: `translate(calc(-50% + ${tx}px), calc(-50% + ${ty}px)) scale(${scale})`,
          }}
          // Mermaid SVG only
          dangerouslySetInnerHTML={{ __html: svgHtml }}
        />
      </div>
      <p className="shrink-0 bg-zinc-900/90 px-3 py-1.5 text-center text-[11px] text-white/60">
        已自动缩放到合适倍率 · 滚轮缩放 · 拖拽平移 · Esc 关闭 · 0 重置
      </p>
    </div>
  );
}

type ShellProps = Readonly<{
  svgHtml: string;
  className?: string;
  title?: string;
}>;

/**
 * Inline diagram shell: horizontal scroll at natural size + open lightbox.
 */
export function DiagramZoomShell({ svgHtml, className, title = "Mermaid 图表" }: ShellProps) {
  const [open, setOpen] = useState(false);

  return (
    <>
      <div className={cn("diagram-zoom-shell group relative", className)}>
        <div
          className="diagram-zoom-scroll"
          // Mermaid SVG only
          dangerouslySetInnerHTML={{ __html: svgHtml }}
        />
        <button
          type="button"
          className="diagram-zoom-open-btn"
          onClick={() => setOpen(true)}
          title="放大查看"
          aria-label="放大查看图表"
        >
          <Maximize2 className="h-3.5 w-3.5" />
          <span>放大</span>
        </button>
      </div>
      <DiagramLightbox
        open={open}
        title={title}
        svgHtml={svgHtml}
        onClose={() => setOpen(false)}
      />
    </>
  );
}
