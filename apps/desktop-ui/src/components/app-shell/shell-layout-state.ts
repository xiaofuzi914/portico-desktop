export const DEFAULT_INSPECTOR_WIDTH = 480;
export const MIN_INSPECTOR_WIDTH = 320;
export const MAX_INSPECTOR_WIDTH = 720;
export const MIN_MAIN_WIDTH = 360;
export const DEFAULT_SIDEBAR_WIDTH = 264;
export const COLLAPSED_SIDEBAR_WIDTH = 56;

const sidebarCollapsedStorageKey = "portico.shell.sidebar.collapsed";
const inspectorWidthStorageKey = "portico.shell.inspector.width";

export function readSidebarCollapsed(): boolean {
  if (typeof window === "undefined") return false;
  try {
    return window.localStorage.getItem(sidebarCollapsedStorageKey) === "true";
  } catch {
    return false;
  }
}

export function writeSidebarCollapsed(collapsed: boolean): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(sidebarCollapsedStorageKey, String(collapsed));
  } catch {
    // Layout preference persistence must never break navigation.
  }
}

export function readInspectorWidth(): number {
  if (typeof window === "undefined") return DEFAULT_INSPECTOR_WIDTH;
  try {
    const raw = window.localStorage.getItem(inspectorWidthStorageKey);
    if (!raw) return DEFAULT_INSPECTOR_WIDTH;
    const parsed = Number.parseInt(raw, 10);
    if (!Number.isFinite(parsed)) return DEFAULT_INSPECTOR_WIDTH;
    return clampInspectorWidth(parsed, Number.POSITIVE_INFINITY);
  } catch {
    return DEFAULT_INSPECTOR_WIDTH;
  }
}

export function writeInspectorWidth(width: number): void {
  if (typeof window === "undefined") return;
  try {
    const clamped = clampInspectorWidth(width, Number.POSITIVE_INFINITY);
    window.localStorage.setItem(inspectorWidthStorageKey, String(clamped));
  } catch {
    // Layout preference persistence must never break the inspector.
  }
}

/**
 * Clamp inspector width so the main pane keeps a usable minimum when the
 * container width is known.
 */
export function clampInspectorWidth(desired: number, containerWidth: number): number {
  if (!Number.isFinite(desired)) return DEFAULT_INSPECTOR_WIDTH;

  const maxByContainer =
    Number.isFinite(containerWidth) && containerWidth > 0
      ? Math.max(MIN_INSPECTOR_WIDTH, containerWidth - MIN_MAIN_WIDTH)
      : MAX_INSPECTOR_WIDTH;
  const max = Math.min(MAX_INSPECTOR_WIDTH, maxByContainer);
  const min = Math.min(MIN_INSPECTOR_WIDTH, max);
  return Math.min(max, Math.max(min, Math.round(desired)));
}
