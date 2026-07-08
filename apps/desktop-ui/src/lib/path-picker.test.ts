import { describe, expect, it } from "vitest";
import { deriveProjectNameFromPath, normalizeDirectorySelection } from "./path-picker";

describe("normalizeDirectorySelection", () => {
  it("returns null when the dialog is cancelled", () => {
    expect(normalizeDirectorySelection(null)).toBeNull();
  });

  it("returns the selected directory path", () => {
    expect(normalizeDirectorySelection("/Users/owen/project")).toBe("/Users/owen/project");
  });

  it("uses the first path if a platform returns an array", () => {
    expect(normalizeDirectorySelection(["/Users/owen/project"])).toBe("/Users/owen/project");
  });
});

describe("deriveProjectNameFromPath", () => {
  it("uses the final directory name for macOS and Linux paths", () => {
    expect(deriveProjectNameFromPath("/Users/owen/portico-desktop")).toBe("portico-desktop");
  });

  it("uses the final directory name for Windows paths", () => {
    expect(deriveProjectNameFromPath("C:\\Users\\owen\\portico-desktop")).toBe(
      "portico-desktop",
    );
  });
});
