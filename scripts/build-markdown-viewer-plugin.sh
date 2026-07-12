#!/usr/bin/env bash
# Upgrade helper: refresh vendor/engine from upstream, keep Portico bridge intact.
#
# Portico bridge (NOT replaced by upstream):
#   provider.js, chrome-shim.js, styles.css, index.html, vendor/fallback/**
# Only vendor/engine + version metadata track upstream package.json.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKG="$ROOT/examples/plugins/markdown-viewer-provider"
SRC="${MARKDOWN_VIEWER_SRC:-/tmp/markdown-viewer-extension}"

BRIDGE_FILES=(
  provider.js
  chrome-shim.js
  styles.css
  index.html
  plugin.json
)
FALLBACK_MARKERS=(
  vendor/fallback/marked.umd.js
  vendor/fallback/mermaid.min.js
  vendor/fallback/katex.min.js
)

assert_bridge() {
  local missing=0
  for f in "${BRIDGE_FILES[@]}"; do
    if [[ ! -f "$PKG/$f" ]]; then
      echo "ERROR: Portico bridge missing: $PKG/$f" >&2
      missing=1
    fi
  done
  for f in "${FALLBACK_MARKERS[@]}"; do
    if [[ ! -f "$PKG/$f" ]]; then
      echo "ERROR: fallback engine missing: $PKG/$f (required for performance path)" >&2
      missing=1
    fi
  done
  if [[ "$missing" -ne 0 ]]; then
    echo "Refusing to finish upgrade without Portico bridge + vendor/fallback." >&2
    exit 1
  fi
}

echo "==> preflight: Portico bridge must already exist (upgrade never invents it)"
assert_bridge

if [[ ! -d "$SRC/.git" ]]; then
  git clone --depth 1 https://github.com/markdown-viewer/markdown-viewer-extension.git "$SRC"
fi

pushd "$SRC" >/dev/null
npm install --ignore-scripts
# Node esbuild build (upstream chrome/build.js targets fibjs)
cat > build-chrome-node.mjs <<'EOF'
import { build } from 'esbuild';
import { createBuildConfig } from './chrome/build-config.js';
import fs from 'fs';
import path from 'path';

const packageJson = JSON.parse(fs.readFileSync('package.json', 'utf8'));
let manifestText = fs.readFileSync('chrome/manifest.json', 'utf8');
manifestText = manifestText.replace(/"version":\s*"[^"]*"/, `"version": "${packageJson.version}"`);
fs.writeFileSync('chrome/manifest.json', manifestText);
console.log('building markdown-viewer-extension', packageJson.version);

try {
  const mod = await import('./scripts/sync-formats.js');
  const syncFormats = mod.default || mod;
  if (typeof syncFormats === 'function') syncFormats();
} catch (_) {}

const outdir = path.join(process.cwd(), 'dist/chrome');
if (fs.existsSync(outdir)) fs.rmSync(outdir, { recursive: true, force: true });
await build(createBuildConfig());
console.log('wrote', outdir);
EOF
node build-chrome-node.mjs
UPSTREAM_VER="$(node -p "require('./package.json').version")"
popd >/dev/null

# Only replace the versioned engine tree — never delete vendor/fallback
mkdir -p "$PKG/vendor"
rsync -a --delete "$SRC/dist/chrome/" "$PKG/vendor/engine/"
cp "$SRC/LICENSE" "$PKG/LICENSE-GPL-3.0"

# Re-assert bridge survived rsync (engine is under vendor/engine only)
assert_bridge

# Version metadata only (bridge logic stays; badge/meta track upstream)
python3 - <<PY
import json
import re
from pathlib import Path

pkg = Path("$PKG")
ver = "$UPSTREAM_VER"

# plugin.json
p = pkg / "plugin.json"
data = json.loads(p.read_text())
data["version"] = ver
data["display_name"] = f"Markdown Viewer (markdown-viewer-extension {ver})"
data["description"] = (
    f"Portico Markdown Provider powered by upstream markdown-viewer-extension v{ver} "
    "(GPL-3.0). Default render: vendor/fallback; optional engine under vendor/engine; "
    "protocol portico.markdown-provider/v1."
)
p.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")

# index.html meta version
idx = pkg / "index.html"
html = idx.read_text()
if 'name="portico-plugin-version"' in html:
    html = re.sub(
        r'(<meta\s+name="portico-plugin-version"\s+content=")[^"]*(")',
        rf"\g<1>{ver}\g<2>",
        html,
        count=1,
    )
else:
    html = html.replace(
        "<head>",
        f'<head>\n    <meta name="portico-plugin-version" content="{ver}" />',
        1,
    )
idx.write_text(html)

print("plugin.json + portico-plugin-version ->", ver)
print("bridge files preserved: provider.js, chrome-shim.js, styles.css, vendor/fallback")
PY

echo "Done. Package: $PKG (upstream engine $UPSTREAM_VER)"
echo "In Portico: Capabilities → Plugins → Reinstall Markdown Viewer"
