import { describe, expect, it } from "vitest";
import { BUNDLED_PLUGIN_CATALOG, findCatalogEntry } from "./plugin-catalog";

describe("plugin catalog", () => {
  it("lists bundled markdown providers with stable ids", () => {
    expect(BUNDLED_PLUGIN_CATALOG.length).toBeGreaterThanOrEqual(2);
    const viewer = BUNDLED_PLUGIN_CATALOG.find((e) => e.name === "markdown-viewer-provider");
    expect(viewer?.capabilities).toContain("markdown.preview");
    expect(viewer?.packagePath).toContain("markdown-viewer-provider");
  });

  it("matches catalog entries by plugin id", () => {
    const first = BUNDLED_PLUGIN_CATALOG[0]!;
    expect(findCatalogEntry(first.id)?.name).toBe(first.name);
    expect(findCatalogEntry("00000000-0000-0000-0000-000000000000")).toBeUndefined();
  });
});
