import { describe, expect, it, beforeEach } from "vitest";
import {
  DEFAULT_MARKDOWN_PRESENTATION,
  loadMarkdownPresentation,
  parsePresentationMode,
  presentationSurfaceClass,
  saveMarkdownPresentation,
} from "./markdown-presentation";

describe("markdown presentation", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults to preview mode without injection-side side effects", () => {
    expect(DEFAULT_MARKDOWN_PRESENTATION.mode).toBe("preview");
    expect(parsePresentationMode("document")).toBe("document");
    expect(parsePresentationMode("nope")).toBe("preview");
  });

  it("persists mode and polish to localStorage", () => {
    saveMarkdownPresentation({
      mode: "document",
      polish: { fontScale: "lg", diagramEmphasis: false, paperTheme: true },
      polishOpen: true,
    });
    const loaded = loadMarkdownPresentation();
    expect(loaded.mode).toBe("document");
    expect(loaded.polish.fontScale).toBe("lg");
    expect(loaded.polish.diagramEmphasis).toBe(false);
    expect(loaded.polishOpen).toBe(true);
  });

  it("maps presentation to surface classes", () => {
    expect(presentationSurfaceClass("source", DEFAULT_MARKDOWN_PRESENTATION.polish)).toBe(
      "markdown-presentation-source",
    );
    const doc = presentationSurfaceClass("document", {
      fontScale: "md",
      diagramEmphasis: true,
      paperTheme: true,
    });
    expect(doc).toContain("markdown-presentation-document");
    expect(doc).toContain("markdown-diagram-emphasis");
    expect(doc).toContain("markdown-paper-theme");
    // Document must not imply a narrow left-rail layout class on the article
    expect(doc).not.toContain("max-w-3xl");
  });
});
