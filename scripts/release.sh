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
NOTES_FILE="$(mktemp)"
trap 'rm -f "$NOTES_FILE"' EXIT
printf '%s\n' "$NOTES" > "$NOTES_FILE"
node scripts/release/manifest.mjs --write "$BUNDLE/latest.json" "v$VERSION" "$SIG_FILE" "$NOTES_FILE" "$REPO"

# 5. SHA256SUMS over the outputs, as the release workflow will publish it.
SUMS_DIR="$(mktemp -d)"
trap 'rm -f "$NOTES_FILE"; rm -rf "$SUMS_DIR"' EXIT
cp "$BUNDLE/macos/Delilah.app.tar.gz" "$SUMS_DIR/Delilah_aarch64.app.tar.gz"
cp "$SIG_FILE" "$SUMS_DIR/Delilah_aarch64.app.tar.gz.sig"
cp "$BUNDLE/latest.json" "$SUMS_DIR/latest.json"
for dmg in "$BUNDLE"/dmg/*.dmg; do
  [[ -f "$dmg" ]] && cp "$dmg" "$SUMS_DIR/"
done
(cd "$SUMS_DIR" && shasum -a 256 -- * | sort -k2) > "$BUNDLE/SHA256SUMS"

# 6. What was produced.
echo "dry run for v$VERSION done; nothing was uploaded. Artefacts:"
echo "  $BUNDLE/macos/Delilah.app.tar.gz"
echo "  $SIG_FILE"
echo "  $BUNDLE/latest.json"
ls "$BUNDLE"/dmg/*.dmg 2>/dev/null | sed 's/^/  /' || true
echo "  $BUNDLE/SHA256SUMS:"
sed 's/^/    /' "$BUNDLE/SHA256SUMS"
