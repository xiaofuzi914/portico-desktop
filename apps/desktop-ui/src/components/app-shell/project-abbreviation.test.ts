import { describe, expect, it } from "vitest";
import { projectAbbreviation } from "./project-abbreviation";

describe("projectAbbreviation", () => {
  it("uses initials for hyphenated English names", () => {
    expect(projectAbbreviation("Agent-Harness")).toBe("AH");
  });

  it("uses initials for camelCase names", () => {
    expect(projectAbbreviation("PorticoItem")).toBe("PI");
  });

  it("uses the first two letters of a single English token", () => {
    expect(projectAbbreviation("Portico")).toBe("PO");
  });

  it("uses the first two CJK characters", () => {
    expect(projectAbbreviation("智能体编程")).toBe("智能");
    expect(projectAbbreviation("项目")).toBe("项目");
  });

  it("ignores surrounding whitespace and empty names", () => {
    expect(projectAbbreviation("  Alpha Beta  ")).toBe("AB");
    expect(projectAbbreviation("   ")).toBe("?");
    expect(projectAbbreviation("")).toBe("?");
  });
});
