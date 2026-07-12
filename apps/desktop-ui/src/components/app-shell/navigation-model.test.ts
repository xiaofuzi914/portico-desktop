import { describe, expect, it } from "vitest";
import { buildNavigationSections } from "./navigation-model";

describe("navigation model", () => {
  it("keeps work surfaces before management surfaces", () => {
    const sections = buildNavigationSections();

    expect(sections.map((section) => section.id)).toEqual(["capabilities", "operations"]);
    expect(sections[0]?.links.map((link) => link.to)).toEqual([
      "/models",
      "/memory",
      "/plugins",
      "/mcp",
    ]);
  });

  it("keeps safe management routes reachable including plugins and MCP", () => {
    const routes = buildNavigationSections().flatMap((section) =>
      section.links.map((link) => link.to),
    );

    expect(routes).toEqual(
      expect.arrayContaining(["/models", "/memory", "/plugins", "/mcp", "/audit"]),
    );
    expect(routes).not.toEqual(
      expect.arrayContaining(["/skills", "/automations", "/browser", "/desktop"]),
    );
  });
});
