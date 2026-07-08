import { describe, expect, it } from "vitest";
import { buildNavigationSections } from "./navigation-model";

describe("navigation model", () => {
  it("keeps work surfaces before management surfaces", () => {
    const sections = buildNavigationSections();

    expect(sections.map((section) => section.id)).toEqual(["capabilities", "operations", "native"]);
    expect(sections[0]?.links.map((link) => link.to)).toEqual(["/models", "/plugins", "/skills"]);
  });

  it("keeps every legacy route reachable", () => {
    const routes = buildNavigationSections().flatMap((section) => section.links.map((link) => link.to));

    expect(routes).toEqual(
      expect.arrayContaining([
        "/models",
        "/plugins",
        "/skills",
        "/automations",
        "/audit",
        "/browser",
        "/desktop",
      ]),
    );
  });
});
