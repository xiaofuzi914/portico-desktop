export const typographyLevels = [
  "pageTitle",
  "pageDescription",
  "sectionTitle",
  "cardTitle",
  "itemTitle",
  "body",
  "metadata",
] as const;

export type TypographyLevel = (typeof typographyLevels)[number];

export const typography: Record<TypographyLevel, string> = {
  pageTitle: "text-2xl font-semibold tracking-tight",
  pageDescription: "text-sm leading-6 text-muted-foreground",
  sectionTitle: "text-sm font-semibold",
  cardTitle: "text-base font-semibold leading-none",
  itemTitle: "text-sm font-medium",
  body: "text-sm leading-6",
  metadata: "text-xs text-muted-foreground",
};

export const typographyLevelRank: Record<TypographyLevel, number> = {
  pageTitle: 5,
  pageDescription: 4,
  sectionTitle: 3,
  cardTitle: 3,
  itemTitle: 2,
  body: 2,
  metadata: 1,
};
