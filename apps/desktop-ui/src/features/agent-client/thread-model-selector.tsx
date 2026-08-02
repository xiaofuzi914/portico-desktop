import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  Bot,
  Brain,
  Check,
  ChevronDown,
  Cloud,
  Cpu,
  Sparkles,
  Zap,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { listModels, listProviders, resolveActiveModel, setActiveModel } from "@/lib/tauri-api";
import { modelKeys, providerKeys } from "@/lib/query-keys";
import type { ModelId, ProviderKind, ThreadId, WorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { cn } from "@/lib/utils";
import {
  modelPickerDetail,
  modelPickerLabel,
  persistThreadModelSelection,
  providerKindForModel,
  selectableThreadModels,
} from "./thread-model-selector-model";

interface ThreadModelSelectorProps {
  workspaceId: WorkspaceId;
  threadId: ThreadId;
  /**
   * `composer` — sits inside the bottom dialog toolbar (default product placement).
   * `inline` — compact text-only for rare secondary surfaces.
   */
  variant?: "composer" | "inline";
  className?: string;
}

type MenuPosition = Readonly<{
  top: number;
  left: number;
  width: number;
  maxHeight: number;
  placement: "above" | "below";
}>;

const MENU_MIN_WIDTH = 176;
const MENU_MAX_WIDTH = 280;
const MENU_MAX_HEIGHT = 256;
const VIEWPORT_PAD = 8;

function providerKindIcon(kind: ProviderKind | null): LucideIcon {
  switch (kind) {
    case "OpenAI":
      return Sparkles;
    case "DeepSeek":
      return Zap;
    case "Anthropic":
      return Brain;
    case "Moonshot":
      return Cloud;
    case "Xai":
      return Bot;
    case "Ollama":
      return Cpu;
    default:
      return Bot;
  }
}

function providerKindAccent(kind: ProviderKind | null): string {
  switch (kind) {
    case "OpenAI":
      return "text-emerald-600 dark:text-emerald-400";
    case "DeepSeek":
      return "text-indigo-600 dark:text-indigo-400";
    case "Anthropic":
      return "text-orange-600 dark:text-orange-400";
    case "Moonshot":
      return "text-sky-600 dark:text-sky-400";
    case "Xai":
      return "text-violet-600 dark:text-violet-400";
    default:
      return "text-muted-foreground";
  }
}

function computeMenuPosition(trigger: DOMRect): MenuPosition {
  const width = Math.min(
    MENU_MAX_WIDTH,
    Math.max(MENU_MIN_WIDTH, trigger.width, 200),
  );
  let left = trigger.left;
  if (left + width > window.innerWidth - VIEWPORT_PAD) {
    left = Math.max(VIEWPORT_PAD, window.innerWidth - VIEWPORT_PAD - width);
  }
  left = Math.max(VIEWPORT_PAD, left);

  const spaceAbove = trigger.top - VIEWPORT_PAD;
  const spaceBelow = window.innerHeight - trigger.bottom - VIEWPORT_PAD;
  // Prefer above (composer sits at bottom); fall back below if cramped.
  const placement: "above" | "below" =
    spaceAbove >= 120 || spaceAbove >= spaceBelow ? "above" : "below";

  if (placement === "above") {
    const maxHeight = Math.min(MENU_MAX_HEIGHT, Math.max(96, spaceAbove - 4));
    return {
      top: trigger.top - 4,
      left,
      width,
      maxHeight,
      placement,
    };
  }

  const maxHeight = Math.min(MENU_MAX_HEIGHT, Math.max(96, spaceBelow - 4));
  return {
    top: trigger.bottom + 4,
    left,
    width,
    maxHeight,
    placement,
  };
}

export function ThreadModelSelector({
  workspaceId,
  threadId,
  variant = "composer",
  className,
}: ThreadModelSelectorProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const resolvedKey = ["active-model", "resolved", workspaceId, threadId] as const;
  const { data: providers = [], isLoading: providersLoading } = useQuery({
    queryKey: providerKeys.list(),
    queryFn: listProviders,
  });
  const { data: models = [], isLoading: modelsLoading } = useQuery({
    queryKey: modelKeys.list(),
    queryFn: () => listModels(),
  });
  const { data: activeModel } = useQuery({
    queryKey: resolvedKey,
    queryFn: () => resolveActiveModel(workspaceId, threadId),
    retry: false,
  });
  const selectableModels = selectableThreadModels(models, providers);
  const selectMutation = useMutation({
    mutationFn: async (modelId: ModelId) => {
      try {
        return await persistThreadModelSelection(
          modelId,
          selectableModels,
          workspaceId,
          threadId,
          setActiveModel,
        );
      } catch (error) {
        if (error instanceof Error && error.message === "MODEL_UNAVAILABLE") {
          throw new Error(t("agent.modelUnavailable"));
        }
        throw error;
      }
    },
    onSuccess: (selection) => {
      queryClient.setQueryData(resolvedKey, selection);
      void queryClient.invalidateQueries({ queryKey: ["active-model"] });
    },
  });

  const [open, setOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<MenuPosition | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLUListElement>(null);

  const updatePosition = useCallback(() => {
    const el = triggerRef.current;
    if (!el) return;
    setMenuPos(computeMenuPosition(el.getBoundingClientRect()));
  }, []);

  useLayoutEffect(() => {
    if (!open) {
      setMenuPos(null);
      return;
    }
    updatePosition();
  }, [open, updatePosition, selectableModels.length]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (triggerRef.current?.contains(target)) return;
      if (menuRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    const onReposition = () => updatePosition();
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onReposition);
    window.addEventListener("scroll", onReposition, true);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onReposition);
      window.removeEventListener("scroll", onReposition, true);
    };
  }, [open, updatePosition]);

  const selected = useMemo(
    () => selectableModels.find((m) => m.id === activeModel?.model_id) ?? null,
    [selectableModels, activeModel?.model_id],
  );
  const selectedKind = selected ? providerKindForModel(selected, providers) : null;
  const SelectedIcon = providerKindIcon(selectedKind);

  if (providersLoading || modelsLoading) {
    return (
      <span
        className={cn(
          "text-muted-foreground text-xs",
          variant === "composer" && "px-2 py-1",
          className,
        )}
      >
        {t("common.loading")}
      </span>
    );
  }

  if (selectableModels.length === 0) {
    return (
      <Link
        to="/models"
        className={cn(
          "text-foreground truncate text-xs hover:underline",
          variant === "composer" &&
            "bg-muted/40 hover:bg-muted/70 inline-flex h-8 max-w-full items-center rounded-lg border px-2.5",
          className,
        )}
      >
        {providers.length > 0 ? t("agent.noRegisteredModels") : t("agent.modelNotConfigured")}
      </Link>
    );
  }

  const triggerLabel = selected ? modelPickerLabel(selected) : t("agent.selectModel");
  const triggerTitle = selected ? modelPickerDetail(selected) : t("agent.selectModelHint");

  const menu =
    open && menuPos && typeof document !== "undefined"
      ? createPortal(
          <ul
            ref={menuRef}
            role="listbox"
            aria-label={t("agent.selectModel")}
            className={cn(
              // Solid surface — do NOT use undefined bg-popover token.
              "border-border bg-background text-foreground fixed z-[200]",
              "overflow-y-auto rounded-lg border py-1 shadow-lg",
            )}
            style={{
              left: menuPos.left,
              width: menuPos.width,
              maxHeight: menuPos.maxHeight,
              ...(menuPos.placement === "above"
                ? { bottom: window.innerHeight - menuPos.top, top: "auto" }
                : { top: menuPos.top, bottom: "auto" }),
            }}
          >
            {selectableModels.map((model) => {
              const kind = providerKindForModel(model, providers);
              const Icon = providerKindIcon(kind);
              const isActive = model.id === activeModel?.model_id;
              const label = modelPickerLabel(model);
              return (
                <li key={model.id} role="option" aria-selected={isActive}>
                  <button
                    type="button"
                    title={modelPickerDetail(model)}
                    className={cn(
                      "hover:bg-muted flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-xs transition-colors",
                      isActive && "bg-muted font-medium",
                    )}
                    onClick={() => {
                      setOpen(false);
                      if (!isActive) selectMutation.mutate(model.id);
                    }}
                  >
                    <Icon
                      className={cn("h-3.5 w-3.5 shrink-0", providerKindAccent(kind))}
                      aria-hidden
                    />
                    <span className="min-w-0 flex-1 truncate">{label}</span>
                    {isActive ? (
                      <Check className="text-foreground h-3.5 w-3.5 shrink-0" aria-hidden />
                    ) : (
                      <span className="w-3.5 shrink-0" aria-hidden />
                    )}
                  </button>
                </li>
              );
            })}
          </ul>,
          document.body,
        )
      : null;

  return (
    <div
      className={cn(
        "relative min-w-0",
        variant === "composer" ? "max-w-[min(100%,14rem)]" : "max-w-48",
        className,
      )}
    >
      <button
        ref={triggerRef}
        type="button"
        aria-label={t("agent.selectModel")}
        aria-haspopup="listbox"
        aria-expanded={open}
        title={triggerTitle}
        disabled={selectMutation.isPending}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "flex w-full min-w-0 items-center gap-1.5 text-left outline-none transition-colors",
          variant === "composer"
            ? cn(
                "border-border bg-muted/40 text-foreground hover:bg-muted/70 focus-visible:ring-ring",
                "h-8 rounded-lg border px-2.5 text-xs font-medium focus-visible:ring-2",
              )
            : "text-foreground h-7 bg-transparent text-xs",
          selectMutation.isPending && "opacity-60",
        )}
      >
        <SelectedIcon
          className={cn("h-3.5 w-3.5 shrink-0", providerKindAccent(selectedKind))}
          aria-hidden
        />
        <span className="min-w-0 flex-1 truncate">{triggerLabel}</span>
        <ChevronDown className="text-muted-foreground h-3.5 w-3.5 shrink-0" aria-hidden />
      </button>

      {menu}

      {selectMutation.error ? (
        <span
          role="alert"
          className="text-destructive absolute -top-1 -right-1 cursor-help text-xs font-bold"
          title={selectMutation.error.message}
        >
          !
        </span>
      ) : null}
    </div>
  );
}
