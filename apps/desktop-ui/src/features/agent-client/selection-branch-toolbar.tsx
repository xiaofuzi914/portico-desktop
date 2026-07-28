import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { GitBranch, Loader2, X } from "lucide-react";
import { useTranslation } from "@/lib/i18n-react";
import { cn } from "@/lib/utils";
import {
  expandSelectionInContainer,
  findSelectionBlock,
  readRawSelection,
  type SelectionUnit,
} from "./selection-expand";

export type SelectionBranchPayload = {
  focusText: string;
  question: string;
  unit: SelectionUnit | "custom";
};

type MenuState = {
  focusText: string;
  question: string;
  unit: SelectionUnit | "custom";
  blockText?: string;
  x: number;
  y: number;
};

type Props = {
  containerRef: React.RefObject<HTMLElement | null>;
  disabled?: boolean;
  pending?: boolean;
  /** Create child session + send question immediately. */
  onConfirm: (payload: SelectionBranchPayload) => void;
};

const MIN_CHARS = 1;
const MAX_CHARS = 4000;
const UNITS: SelectionUnit[] = ["word", "sentence", "paragraph"];

/**
 * Single surface for 划词发散: edit focus, type question, expand 词/句/段, start child session.
 * No intermediate modal.
 */
export function SelectionBranchToolbar({
  containerRef,
  disabled = false,
  pending = false,
  onConfirm,
}: Props) {
  const { t } = useTranslation();
  const [menu, setMenu] = useState<MenuState | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const questionRef = useRef<HTMLTextAreaElement | null>(null);
  const focusTextRef = useRef("");
  const questionRefValue = useRef("");
  /** True while interacting inside the popover (question field / scrolling focus). */
  const editingRef = useRef(false);

  const closeMenu = useCallback(() => {
    setMenu(null);
    editingRef.current = false;
    focusTextRef.current = "";
    questionRefValue.current = "";
  }, []);

  const placeMenu = useCallback(
    (
      focusText: string,
      unit: SelectionUnit | "custom",
      blockText?: string,
      opts?: { keepQuestion?: boolean },
    ) => {
      const sel = window.getSelection();
      let x = 12;
      let y = 12;
      if (sel && sel.rangeCount > 0) {
        const rect = sel.getRangeAt(0).getBoundingClientRect();
        if (rect.width > 0 || rect.height > 0) {
          const pad = 12;
          const menuW = 440;
          const menuH = 300;
          x = rect.left + rect.width / 2 - menuW / 2;
          y = rect.top - menuH - 10;
          if (y < pad) y = rect.bottom + 10;
          x = Math.min(Math.max(pad, x), window.innerWidth - menuW - pad);
          y = Math.min(Math.max(pad, y), window.innerHeight - menuH - pad);
        }
      }
      // Show full selected text (no ellipsis) — scroll inside the panel if long.
      const nextFocus =
        focusText.length > MAX_CHARS ? focusText.slice(0, MAX_CHARS) : focusText;
      focusTextRef.current = nextFocus;
      const nextQuestion = opts?.keepQuestion === false ? "" : questionRefValue.current;
      setMenu((prev) => ({
        focusText: nextFocus,
        question: nextQuestion || prev?.question || "",
        unit,
        blockText: blockText ?? prev?.blockText,
        x: prev && editingRef.current ? prev.x : x,
        y: prev && editingRef.current ? prev.y : y,
      }));
    },
    [],
  );

  const syncFromDomSelection = useCallback(() => {
    if (editingRef.current) return;

    const root = containerRef.current;
    if (!root) {
      closeMenu();
      return;
    }
    const raw = readRawSelection(root);
    if (!raw || raw.length < MIN_CHARS) {
      closeMenu();
      return;
    }
    let blockText: string | undefined;
    const sel = window.getSelection();
    if (sel && sel.rangeCount > 0) {
      const range = sel.getRangeAt(0);
      const block = findSelectionBlock(range.commonAncestorContainer, root);
      const bt = block?.innerText?.replace(/\r\n/g, "\n").trim();
      if (bt) blockText = bt;
    }
    questionRefValue.current = "";
    placeMenu(raw, "custom", blockText, { keepQuestion: false });
    window.setTimeout(() => questionRef.current?.focus(), 30);
  }, [closeMenu, containerRef, placeMenu]);

  useEffect(() => {
    if (disabled) {
      closeMenu();
      return;
    }

    const onMouseUp = (event: MouseEvent) => {
      if (event.button !== 0) return;
      const target = event.target;
      if (
        target instanceof Element &&
        menuRef.current &&
        menuRef.current.contains(target)
      ) {
        return;
      }
      editingRef.current = false;
      window.requestAnimationFrame(() => syncFromDomSelection());
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !pending) {
        closeMenu();
        return;
      }
      if (event.shiftKey && !editingRef.current) {
        window.requestAnimationFrame(() => syncFromDomSelection());
      }
    };

    const onScroll = () => {
      if (!menu || editingRef.current) return;
      const root = containerRef.current;
      if (!root || !readRawSelection(root)) {
        // Keep open while typing the question even if DOM selection cleared.
        if (questionRefValue.current.trim() || focusTextRef.current.trim()) return;
        closeMenu();
        return;
      }
      placeMenu(focusTextRef.current || menu.focusText, menu.unit, menu.blockText);
    };

    document.addEventListener("mouseup", onMouseUp);
    document.addEventListener("keyup", onKeyUp);
    document.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("mouseup", onMouseUp);
      document.removeEventListener("keyup", onKeyUp);
      document.removeEventListener("scroll", onScroll, true);
    };
  }, [closeMenu, containerRef, disabled, menu, pending, placeMenu, syncFromDomSelection]);

  const expandTo = useCallback(
    (unit: SelectionUnit) => {
      const root = containerRef.current;
      if (!root) return;
      const expanded = expandSelectionInContainer(root, unit);
      if (expanded) {
        placeMenu(expanded.text, expanded.unit, expanded.blockText || menu?.blockText);
        return;
      }
      if (unit === "paragraph" && menu?.blockText) {
        placeMenu(menu.blockText, "paragraph", menu.blockText);
        return;
      }
      if (menu?.focusText) {
        placeMenu(menu.focusText, unit, menu.blockText);
      }
    },
    [containerRef, menu?.blockText, menu?.focusText, placeMenu],
  );

  if (!menu || disabled) return null;

  const unitLabel = (u: SelectionUnit) =>
    u === "word"
      ? t("agent.selectionUnitWord")
      : u === "sentence"
        ? t("agent.selectionUnitSentence")
        : t("agent.selectionUnitParagraph");

  const canSubmit =
    menu.focusText.trim().length >= 1 &&
    menu.question.trim().length >= 1 &&
    !pending;

  const submit = () => {
    const focusText = (focusTextRef.current || menu.focusText).trim();
    const question = (questionRefValue.current || menu.question).trim();
    if (!focusText || !question || pending) return;
    window.getSelection()?.removeAllRanges();
    onConfirm({ focusText, question, unit: menu.unit });
    closeMenu();
  };

  return createPortal(
    <div
      ref={menuRef}
      role="dialog"
      aria-label={t("agent.selectionBranchToolbar")}
      className={cn(
        "bg-background/98 text-foreground border-border fixed z-[120] flex w-[min(100vw-24px,440px)] flex-col gap-2.5 rounded-2xl border p-3.5 shadow-xl backdrop-blur-sm",
      )}
      style={{ left: menu.x, top: menu.y }}
      onMouseDown={(e) => {
        const target = e.target;
        if (
          target instanceof HTMLTextAreaElement ||
          target instanceof HTMLInputElement ||
          (target instanceof Element && target.closest("textarea, input, button"))
        ) {
          return;
        }
        e.preventDefault();
      }}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-sm font-semibold leading-snug">
            {t("agent.selectionBranchDialogTitle")}
          </p>
          <p className="text-muted-foreground mt-0.5 text-[11px] leading-snug">
            {t("agent.selectionBranchInlineHint")}
          </p>
        </div>
        <button
          type="button"
          className="text-muted-foreground hover:bg-muted hover:text-foreground rounded-lg p-1.5 transition-colors"
          disabled={pending}
          aria-label={t("common.close")}
          onClick={closeMenu}
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="bg-muted/70 flex w-fit rounded-xl p-1">
        {UNITS.map((unit) => (
          <button
            key={unit}
            type="button"
            disabled={pending}
            className={cn(
              "min-w-[2.75rem] rounded-lg px-3 py-1.5 text-sm font-medium transition-colors",
              menu.unit === unit
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )}
            title={t("agent.selectionUnitHint").replace("{unit}", unitLabel(unit))}
            onClick={() => expandTo(unit)}
          >
            {unitLabel(unit)}
          </button>
        ))}
      </div>

      <div className="flex flex-col gap-1">
        <span className="text-muted-foreground px-0.5 text-[11px] font-medium">
          {t("agent.selectionBranchFocusLabelReadonly")}
        </span>
        <div
          className={cn(
            "border-border bg-muted/35 text-foreground",
            "max-h-40 min-h-[3rem] w-full overflow-y-auto rounded-xl border px-3 py-2.5",
            "whitespace-pre-wrap break-words text-sm leading-relaxed select-text",
          )}
          // Keep scroll/select inside the panel from dismissing it.
          onMouseDown={() => {
            editingRef.current = true;
          }}
          onMouseUp={() => {
            editingRef.current = false;
          }}
        >
          {menu.focusText || (
            <span className="text-muted-foreground">
              {t("agent.selectionBranchFocusPlaceholder")}
            </span>
          )}
        </div>
      </div>

      <label className="flex flex-col gap-1">
        <span className="text-muted-foreground px-0.5 text-[11px] font-medium">
          {t("agent.selectionBranchQuestionLabel")}
        </span>
        <textarea
          ref={questionRef}
          value={menu.question}
          rows={2}
          disabled={pending}
          className={cn(
            "border-input bg-background text-foreground focus-visible:ring-ring",
            "max-h-28 min-h-[3.25rem] w-full resize-y rounded-xl border px-3 py-2 text-sm leading-relaxed",
            "outline-none focus-visible:ring-2 disabled:opacity-60",
          )}
          placeholder={t("agent.selectionBranchQuestionPlaceholder")}
          onFocus={() => {
            editingRef.current = true;
          }}
          onBlur={() => {
            editingRef.current = false;
          }}
          onChange={(e) => {
            const next = e.target.value;
            questionRefValue.current = next;
            setMenu((prev) => (prev ? { ...prev, question: next } : prev));
          }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Escape" && !pending) {
              e.preventDefault();
              closeMenu();
              return;
            }
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && canSubmit) {
              e.preventDefault();
              submit();
            }
          }}
        />
      </label>

      <button
        type="button"
        disabled={!canSubmit}
        className={cn(
          "inline-flex h-11 w-full items-center justify-center gap-2 rounded-xl px-4 text-sm font-semibold transition-colors",
          "bg-primary text-primary-foreground hover:bg-primary/90",
          "disabled:pointer-events-none disabled:opacity-50",
        )}
        title={t("agent.selectionBranchHint")}
        onClick={submit}
      >
        {pending ? (
          <Loader2 className="h-4 w-4 shrink-0 animate-spin" />
        ) : (
          <GitBranch className="h-4 w-4 shrink-0" />
        )}
        {pending
          ? t("agent.selectionBranchSubmitting")
          : t("agent.selectionBranchSubmit")}
      </button>
    </div>,
    document.body,
  );
}
