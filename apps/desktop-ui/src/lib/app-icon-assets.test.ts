import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const repoRoot = resolve(process.cwd(), "../..");
const iconDir = join(repoRoot, "src-tauri", "icons");
const placeholderHash = "55299864f4c74fb19b9ff0a1ab313552b33b128fec02f0997fb6b3c245308bf4";

function readIcon(name: string): Buffer {
  return readFileSync(join(iconDir, name));
}

function sha256(asset: Buffer): string {
  return createHash("sha256").update(asset).digest("hex");
}

function expectPngSize(asset: Buffer, width: number, height: number) {
  expect(asset.subarray(1, 4).toString()).toBe("PNG");
  expect(asset.readUInt32BE(16)).toBe(width);
  expect(asset.readUInt32BE(20)).toBe(height);
}

describe("app icon assets", () => {
  it("uses the product name for the native executable and macOS bundle checks", () => {
    const manifest = readFileSync(join(repoRoot, "src-tauri", "Cargo.toml"), "utf8");
    const desktopE2e = readFileSync(join(repoRoot, "scripts", "run-desktop-e2e.sh"), "utf8");
    const dmgVerifier = readFileSync(join(repoRoot, "scripts", "verify-macos-dmg.sh"), "utf8");

    expect(manifest).toMatch(/\[\[bin\]\]\s+name = "Portico"\s+path = "src\/main\.rs"/);
    expect(desktopE2e).toContain('release/Portico"');
    expect(dmgVerifier).toContain('Contents/MacOS/Portico"');
  });

  it("invalidates the native build when runtime icon files change", () => {
    const buildScript = readFileSync(join(repoRoot, "src-tauri", "build.rs"), "utf8");
    const emittedDirectives = buildScript
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => /^println!\("cargo:rerun-if-changed=icons\/[^"]+"\);$/.test(line));

    for (const icon of ["icon.png", "icon.icns", "icon.ico"]) {
      expect(emittedDirectives).toContain(`println!("cargo:rerun-if-changed=icons/${icon}");`);
    }
  });

  it("keeps every desktop bundle format generated from the Portico mark", () => {
    expectPngSize(readIcon("32x32.png"), 32, 32);
    expectPngSize(readIcon("128x128.png"), 128, 128);
    expectPngSize(readIcon("128x128@2x.png"), 256, 256);
    expect(readIcon("icon.ico").subarray(0, 4)).toEqual(Buffer.from([0, 0, 1, 0]));
    expect(readIcon("icon.icns").subarray(0, 4).toString()).toBe("icns");
  });

  it("uses the packaged 32px mark as the favicon instead of the Tauri placeholder", () => {
    const packagedIcon = readIcon("32x32.png");
    const favicon = readFileSync(join(repoRoot, "apps", "desktop-ui", "public", "favicon.png"));

    expect(sha256(packagedIcon)).not.toBe(placeholderHash);
    expect(favicon).toEqual(packagedIcon);
  });
});
