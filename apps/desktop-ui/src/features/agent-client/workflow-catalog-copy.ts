/**
 * Product-facing copy for multi-agent catalog entries.
 * Backend may still ship English technical titles/summaries; the menu
 * overlays localized “when to use / what you get” language for non-technical users.
 */

export type CatalogCopy = {
  /** Short plain-language name. */
  title: string;
  /** One line: when this option fits. */
  when: string;
  /** One line: what happens / how it differs. */
  difference: string;
};

const KNOWN_KEYS = [
  "multi-lens-review",
  "deep-research",
  "iterative-refine",
] as const;

export type KnownCatalogKey = (typeof KNOWN_KEYS)[number];

export function isKnownCatalogKey(id: string | null | undefined): id is KnownCatalogKey {
  return Boolean(id && (KNOWN_KEYS as readonly string[]).includes(id));
}

/** Preferred display order for built-in templates (user-facing). */
export function catalogDisplayOrder(aId: string, bId: string): number {
  const rank = (id: string) => {
    const i = (KNOWN_KEYS as readonly string[]).indexOf(id);
    return i === -1 ? 100 : i;
  };
  return rank(aId) - rank(bId);
}

/**
 * Resolve display copy for a catalog/template entry.
 * @param id catalog_key or stable id (e.g. multi-lens-review)
 * @param t i18n translate
 * @param fallbackTitle server title if unknown custom template
 * @param fallbackSummary server summary if unknown custom template
 */
export function resolveCatalogCopy(
  id: string,
  t: (key: string) => string,
  fallbackTitle?: string,
  fallbackSummary?: string,
): CatalogCopy {
  if (isKnownCatalogKey(id)) {
    return {
      title: t(`orchestration.wf.${id}.title`),
      when: t(`orchestration.wf.${id}.when`),
      difference: t(`orchestration.wf.${id}.diff`),
    };
  }
  return {
    title: fallbackTitle?.trim() || id,
    when: fallbackSummary?.trim() || t("orchestration.wf.custom.when"),
    difference: t("orchestration.wf.custom.diff"),
  };
}
