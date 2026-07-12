import { useCallback, useRef, type PointerEvent as ReactPointerEvent } from "react";

interface PanelResizeHandleProps {
  /** Current size of the panel being resized (pixels). */
  value: number;
  /** Live updates while dragging. */
  onChange: (next: number) => void;
  /** Persist after the pointer is released. */
  onCommit: (next: number) => void;
  /** Double-click restores the default size. */
  onReset: () => void;
  /** Accessible label for the separator. */
  label: string;
  /**
   * Direction of growth when the pointer moves right.
   * `start` means dragging left increases the panel (right-side panel).
   */
  growth?: "start" | "end";
}

/**
 * Vertical drag handle used between main content and the inspector.
 * Width is derived as: startValue ± (pointer delta), depending on growth side.
 */
export function PanelResizeHandle({
  value,
  onChange,
  onCommit,
  onReset,
  label,
  growth = "start",
}: PanelResizeHandleProps) {
  const dragRef = useRef<{
    pointerId: number;
    startX: number;
    startValue: number;
  } | null>(null);

  const computeNext = useCallback(
    (clientX: number, startX: number, startValue: number) => {
      const delta = clientX - startX;
      return growth === "start" ? startValue - delta : startValue + delta;
    },
    [growth],
  );

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startValue: value,
    };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    onChange(computeNext(event.clientX, drag.startX, drag.startValue));
  };

  const endDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    dragRef.current = null;
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    onCommit(computeNext(event.clientX, drag.startX, drag.startValue));
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(value)}
      tabIndex={0}
      className="group relative z-10 hidden w-1 shrink-0 cursor-col-resize xl:block"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDoubleClick={(event) => {
        event.preventDefault();
        onReset();
      }}
      onKeyDown={(event) => {
        const step = event.shiftKey ? 32 : 16;
        if (event.key === "ArrowLeft") {
          event.preventDefault();
          const next = growth === "start" ? value + step : value - step;
          onChange(next);
          onCommit(next);
        } else if (event.key === "ArrowRight") {
          event.preventDefault();
          const next = growth === "start" ? value - step : value + step;
          onChange(next);
          onCommit(next);
        } else if (event.key === "Home" || event.key === "Enter") {
          event.preventDefault();
          onReset();
        }
      }}
    >
      <div className="bg-border group-hover:bg-primary/40 group-focus-visible:bg-primary/50 group-active:bg-primary/60 absolute inset-y-0 left-1/2 w-px -translate-x-1/2 transition-colors" />
      <div className="absolute inset-y-0 -right-1 -left-1" />
    </div>
  );
}
