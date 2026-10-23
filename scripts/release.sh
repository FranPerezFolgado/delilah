#!/usr/bin/env bash
# Local release rehearsal: ./scripts/release.sh X.Y.Z --dry-run
# Runs the pre-release checks and builds + signs the app, writes latest.json,
# and prints the artefacts. It never uploads; the real release is the tag push
# (see `make release`), which the GitHub workflow builds and publishes.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="FranPerezFolgado/delilah"
VERSION="${1:-}"
MODE="${2:-}"

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: scripts/release.sh X.Y.Z --dry-run" >&2
  exit 1
fi

if [[ "$MODE" != "--dry-run" ]]; then
  cat >&2 <<MSG
This script only rehearses a release (--dry-run); it never uploads.
To release, run: make release VERSION=$VERSION
or by hand, on main with the version bumped:
  git tag v$VERSION && git push origin v$VERSION
MSG
  exit 2
fi

cd "$ROOT"

# 1. Clean tree and a public repo.
if [[ -n "$(git status --porcelain)" ]]; then
  echo "working tree is not clean" >&2
  exit 1
fi
visibility="$(gh repo view "$REPO" --json visibility -q .visibility)"
if [[ "$visibility" != "PUBLIC" ]]; then
  echo "$REPO is $visibility, expected PUBLIC" >&2
  exit 1
fi

# 2. Checks, with the signing key from the usual local places (never printed).
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/delilah.key")"
fi
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]]; then
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s delilah-updater -w)"
fi
export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD
node scripts/release/check.mjs "$VERSION" --require-secrets

# 3. Notes and build.
BUNDLE="$ROOT/app/src-tauri/target/release/bundle"
NOTES="$(node scripts/release/notes.mjs "v$VERSION")"
(cd app && npm run dist)

# 4. latest.json from the .sig file.
SIG_FILE="$BUNDLE/macos/Delilah.app.tar.gz.sig"
if [[ ! -f "$SIG_FILE" ]]; then
  echo "missing $SIG_FILE; did the build sign the app?" >&2
  exit 1
fi
NOTES="$NOTES" SIG_FILE="$SIG_FILE" VERSION="$VERSION" REPO="$REPO" \
  node --input-type=module -e '
import { readFileSync, writeFileSync } from "node:fs";
import { manifest } from "./scripts/release/manifest.mjs";
const m = manifest({
  version: process.env.VERSION,
  notes: process.env.NOTES,
  pubDate: new Date().toISOString(),
  signature: readFileSync(process.env.SIG_FILE, "utf8").trim(),
  repo: process.env.REPO,
});
writeFileSync(process.argv[1], JSON.stringify(m, null, 2) + "\n");
' "$BUNDLE/latest.json"

# 5. What was produced.
echo "dry run for v$VERSION done; nothing was uploaded. Artefacts:"
echo "  $BUNDLE/macos/Delilah.app.tar.gz"
echo "  $SIG_FILE"
echo "  $BUNDLE/latest.json"
ls "$BUNDLE"/dmg/*.dmg 2>/dev/null | sed 's/^/  /' || true
