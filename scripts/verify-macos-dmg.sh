#!/usr/bin/env bash
set -euo pipefail

device=""
mount_point=""

cleanup() {
  if [[ -n "$device" ]]; then
    hdiutil detach "$device" >/dev/null 2>&1 || hdiutil detach "$device" -force >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ $# -gt 0 ]]; then
  DMG_PATH="$1"
else
  shopt -s nullglob
  dmg_candidates=(target/release/bundle/dmg/Portico_*.dmg)
  test "${#dmg_candidates[@]}" -eq 1
  DMG_PATH="${dmg_candidates[0]}"
fi

test -f "$DMG_PATH"
hdiutil verify "$DMG_PATH" >/dev/null

attach_output="$(hdiutil attach -readonly -nobrowse "$DMG_PATH")"
device="$(printf '%s\n' "$attach_output" | awk '/^\/dev\// {print $1; exit}')"
mount_point="$(printf '%s\n' "$attach_output" | awk -F '\t' '/\/Volumes\// {print $NF; exit}')"
test -n "$device"
test -n "$mount_point"

app="$mount_point/Portico.app"
binary="$app/Contents/MacOS/portico-tauri"
test -x "$binary"
test -f "$app/Contents/Resources/icon.icns"
codesign --verify --deep --strict --verbose=2 "$app"

while IFS= read -r candidate; do
  if file "$candidate" | grep -q 'Mach-O'; then
    if LC_ALL=C strings "$candidate" | \
      grep -E 'WDIO WebDriver plugin|TAURI_WEBDRIVER_PORT|WebDriver server listening' >/dev/null; then
      echo "test-only WebDriver server found in production DMG: $candidate" >&2
      exit 1
    fi
  fi
done < <(find "$app" -type f -perm -111 -print)

printf 'macOS DMG static verification passed: %s\n' "$DMG_PATH"
