#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DATA_DIR="$(mktemp -d "${TMPDIR:-/tmp}/portico-desktop-e2e.XXXXXX")"
E2E_TARGET_DIR="$ROOT_DIR/target/desktop-e2e"
SCHEMA_BACKUP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/portico-schema-backup.XXXXXX")"
SCHEMA_DIR="$ROOT_DIR/src-tauri/gen/schemas"
PROVIDER_NAME="Portico E2E Provider $(uuidgen)"

printf 'portico-desktop-e2e\n' >"$APP_DATA_DIR/.portico-desktop-e2e"

for schema in acl-manifests.json desktop-schema.json macOS-schema.json; do
  cp "$SCHEMA_DIR/$schema" "$SCHEMA_BACKUP_DIR/$schema"
done

cleanup() {
  rm -rf "$APP_DATA_DIR"
  for schema in acl-manifests.json desktop-schema.json macOS-schema.json; do
    cp "$SCHEMA_BACKUP_DIR/$schema" "$SCHEMA_DIR/$schema"
  done
  rm -rf "$SCHEMA_BACKUP_DIR"
}
trap cleanup EXIT

cd "$ROOT_DIR"
CARGO_TARGET_DIR="$E2E_TARGET_DIR" pnpm exec tauri build --no-bundle \
  --features desktop-e2e --config src-tauri/tauri.e2e.conf.json

e2e_binary="$E2E_TARGET_DIR/release/portico-tauri"
LC_ALL=C strings "$e2e_binary" | \
  grep -E 'WDIO WebDriver plugin|TAURI_WEBDRIVER_PORT|WebDriver server listening' >/dev/null

PORTICO_E2E_APP_DATA_DIR="$APP_DATA_DIR" \
  PORTICO_E2E_BINARY="$e2e_binary" \
  PORTICO_E2E_PROVIDER_NAME="$PROVIDER_NAME" \
  node e2e/desktop/run.mjs

provider_count="$(sqlite3 "$APP_DATA_DIR/portico.sqlite" \
  "SELECT COUNT(*) FROM provider_configs WHERE display_name = '$PROVIDER_NAME' AND base_url = 'http://127.0.0.1:9/v1' AND api_key_reference = 'openai-default';")"
test "$provider_count" = "1"

expected_migrations="$(find crates/app-runtime/migrations -type f -name '[0-9]*_*.sql' | wc -l | tr -d ' ')"
applied_migrations="$(sqlite3 "$APP_DATA_DIR/portico.sqlite" \
  "SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1;")"
test "$applied_migrations" = "$expected_migrations"
echo "desktop-e2e-ok"
