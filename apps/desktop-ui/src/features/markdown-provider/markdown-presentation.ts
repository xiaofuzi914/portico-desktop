/**
 * Presentation is a *view-layer* concern: users preview source first, then
 * switch display effect and polish — without rewriting generation prompts.
 */

export type MarkdownPresentationMode = "source" | "preview" | "document";

export type MarkdownFontScale = "sm" | "md" | "lg";

export type MarkdownPresentationPolish = Readonly<{
  /** Body text scale for preview/document modes. */
  fontScale: MarkdownFontScale;
  /** Emphasize diagrams (larger mono / more padding). */
  diagramEmphasis: boolean;
  /** Paper-like document chrome (serif titles, page card). */
  paperTheme: boolean;
}>;

export type MarkdownPresentationState = Readonly<{
  mode: MarkdownPresentationMode;
  polish: MarkdownPresentationPolish;
  /** Whether the polish strip is expanded. */
  polishOpen: boolean;
}>;

const STORAGE_KEY = "portico.markdownPresentation.v1";

export const DEFAULT_MARKDOWN_POLISH: MarkdownPresentationPolish = {
  fontScale: "md",
  diagramEmphasis: true,
  paperTheme: true,
};

export const DEFAULT_MARKDOWN_PRESENTATION: MarkdownPresentationState = {
  mode: "preview",
  polish: DEFAULT_MARKDOWN_POLISH,
  polishOpen: false,
};

export function parsePresentationMode(value: unknown): MarkdownPresentationMode {
  if (value === "source" || value === "preview" || value === "document") return value;
  return "preview";
}

export function parseFontScale(value: unknown): MarkdownFontScale {
  if (value === "sm" || value === "md" || value === "lg") return value;
  return "md";
}

export function loadMarkdownPresentation(): MarkdownPresentationState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_MARKDOWN_PRESENTATION;
    const parsed = JSON.parse(raw) as Partial<MarkdownPresentationState>;
    return {
      mode: parsePresentationMode(parsed.mode),
      polish: {
        fontScale: parseFontScale(parsed.polish?.fontScale),
        diagramEmphasis: parsed.polish?.diagramEmphasis ?? true,
        paperTheme: parsed.polish?.paperTheme ?? true,
      },
      polishOpen: Boolean(parsed.polishOpen),
    };
  } catch {
    return DEFAULT_MARKDOWN_PRESENTATION;
  }
}

export function saveMarkdownPresentation(state: MarkdownPresentationState): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

/** CSS class tokens applied to the preview surface for polish options. */
export function presentationSurfaceClass(
  mode: MarkdownPresentationMode,
  polish: MarkdownPresentationPolish,
): string {
  if (mode === "source") return "markdown-presentation-source";
  const parts = [
    mode === "document" ? "markdown-presentation-document" : "markdown-presentation-preview",
    `markdown-font-${polish.fontScale}`,
  ];
  if (polish.diagramEmphasis) parts.push("markdown-diagram-emphasis");
  if (polish.paperTheme && mode === "document") parts.push("markdown-paper-theme");
  return parts.join(" ");
}

export const PRESENTATION_MODE_OPTIONS: ReadonlyArray<{
  id: MarkdownPresentationMode;
  label: string;
  description: string;
}> = [
  { id: "source", label: "源码", description: "原始 Markdown 文本" },
  { id: "preview", label: "预览", description: "GFM / 公式 / Mermaid" },
  { id: "document", label: "文档", description: "版式修饰后的阅读视图" },
];
