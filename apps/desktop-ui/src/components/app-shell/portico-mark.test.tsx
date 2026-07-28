import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import { PorticoMark } from "./portico-mark";

describe("PorticoMark", () => {
  let container: HTMLDivElement | null = null;
  let root: Root | null = null;

  afterEach(async () => {
    if (root) {
      await act(async () => root?.unmount());
    }
    container?.remove();
    container = null;
    root = null;
  });

  it("renders the shared brand asset as a decorative image", async () => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);

    await act(async () => {
      root?.render(<PorticoMark className="h-7 w-7" />);
    });

    const mark = container.querySelector("img");
    expect(mark?.getAttribute("src")).toBe("/portico-mark.svg");
    expect(mark?.getAttribute("aria-hidden")).toBe("true");
    expect(mark?.getAttribute("draggable")).toBe("false");
    expect(mark?.className).toContain("h-7 w-7");
  });
});
