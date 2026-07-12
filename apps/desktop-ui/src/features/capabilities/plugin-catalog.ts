/**
 * Built-in catalog of third-party plugins that ship with the Portico repo.
 * Install is still folder-based (user picks the package directory); the catalog
 * documents what each package provides and how to find it.
 */

export type PluginCatalogEntry = Readonly<{
  /** Stable plugin UUID from plugin.json — used to match installed state. */
  id: string;
  name: string;
  displayNameKey: string;
  descriptionKey: string;
  /** Relative path from the monorepo root for documentation. */
  packagePath: string;
  featuresKey: string;
  /** Host capabilities declared by the package. */
  capabilities: readonly string[];
  category: "markdown" | "other";
}>;

export const BUNDLED_PLUGIN_CATALOG: readonly PluginCatalogEntry[] = [
  {
    id: "a8c3e2f1-4b5d-6e7f-8091-a2b3c4d5e6f7",
    name: "markdown-viewer-provider",
    displayNameKey: "capabilities.catalog.markdownViewer.name",
    descriptionKey: "capabilities.catalog.markdownViewer.description",
    packagePath: "examples/plugins/markdown-viewer-provider",
    featuresKey: "capabilities.catalog.markdownViewer.features",
    capabilities: ["markdown.preview", "markdown.export.html"],
    category: "markdown",
  },
  {
    id: "3e76cc57-3911-4e61-8baa-4b354f32d7d1",
    name: "markdown-rich-provider",
    displayNameKey: "capabilities.catalog.markdownRich.name",
    descriptionKey: "capabilities.catalog.markdownRich.description",
    packagePath: "examples/plugins/markdown-rich-provider",
    featuresKey: "capabilities.catalog.markdownRich.features",
    capabilities: ["markdown.preview", "markdown.export.html"],
    category: "markdown",
  },
];

export function findCatalogEntry(pluginId: string): PluginCatalogEntry | undefined {
  return BUNDLED_PLUGIN_CATALOG.find((entry) => entry.id === pluginId);
}
