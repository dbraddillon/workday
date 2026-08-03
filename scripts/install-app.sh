#!/usr/bin/env bash
# Build, ad-hoc sign, and install Workday.app into /Applications in one step.
# Usage: npm run install-app
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

APP="src-tauri/target/release/bundle/macos/Workday.app"
DEST="/Applications/Workday.app"

echo "▸ Building release bundle…"
npm run app:build

echo "▸ Ad-hoc signing…"
codesign --force --deep --sign - "$APP"

echo "▸ Installing to /Applications…"
# Quit a running copy so we can replace it.
pkill -f "$DEST/Contents/MacOS/workday" 2>/dev/null || true
sleep 1
rm -rf "$DEST"
cp -R "$APP" "$DEST"
# Strip quarantine so the local build opens without a Gatekeeper prompt.
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true

echo "▸ Launching…"
open "$DEST"

echo "✓ Installed and launched. Look for the icon in your menu bar (or press ⌘⇧J)."
