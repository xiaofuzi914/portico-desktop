export const inspectorTabs = ["timeline", "files", "audit"] as const;

export type InspectorTab = (typeof inspectorTabs)[number];

export function isInspectorTab(value: string): value is InspectorTab {
  return inspectorTabs.includes(value as InspectorTab);
}

const inspectorCollapsedStorageKey = "portico.inspector.collapsed";

export function readInspectorCollapsed(): boolean {
  if (typeof window === "undefined") return false;
  try {
    return window.localStorage.getItem(inspectorCollapsedStorageKey) === "true";
  } catch {
    return false;
  }
}

export function writeInspectorCollapsed(collapsed: boolean): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(inspectorCollapsedStorageKey, String(collapsed));
  } catch {
    // Layout preference persistence must never break the inspector.
  }
}
