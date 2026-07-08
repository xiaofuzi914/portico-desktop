export const inspectorTabs = [
  "files",
  "terminal",
  "context",
  "artifacts",
  "browser",
  "desktop",
  "audit",
] as const;

export type InspectorTab = (typeof inspectorTabs)[number];

export function isInspectorTab(value: string): value is InspectorTab {
  return inspectorTabs.includes(value as InspectorTab);
}
