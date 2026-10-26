#!/usr/bin/env bash
# Install the latest Delilah release into /Applications.
# Usage: bash install.sh [--yes] [-h|--help]
# See specs/008-public-repo-onboarding/contracts/install-script.md for the contract.
set -euo pipefail

REPO_URL="https://github.com/FranPerezFolgado/delilah/releases/latest/download"
ASSET="Delilah_aarch64.app.tar.gz"
SUMS="SHA256SUMS"

BASE_URL="${DELILAH_BASE_URL:-$REPO_URL}"
INSTALL_DIR="${DELILAH_INSTALL_DIR:-/Applications}"
ARCH="${DELILAH_ARCH:-$(uname -m)}"

usage() {
  cat <<USAGE
Usage: install.sh [--yes] [-h|--help]

Downloads the latest Delilah release, verifies its SHA-256 checksum and installs
Delilah.app into /Applications (Apple Silicon Macs only).

  --yes    Don't ask before replacing an existing Delilah.app
  -h       Show this help
USAGE
}

die() { # die <code> <message>
  local code="$1"
  shift
  echo "error: $*" >&2
  exit "$code"
}

YES=0
for arg in "$@"; do
  case "$arg" in
    --yes) YES=1 ;;
    -h | --help) usage; exit 0 ;;
    *) usage >&2; exit 1 ;;
  esac
done

# 1. Platform
if [ "$(uname -s)" != "Darwin" ] || [ "$ARCH" != "arm64" ]; then
  die 2 "Delilah needs a Mac with Apple Silicon"
fi

# 2. Tools
for tool in curl shasum tar ditto; do
  command -v "$tool" >/dev/null 2>&1 || die 2 "missing required tool: $tool"
done

TMP="$(mktemp -d)"
[ -n "$TMP" ] && [ -d "$TMP" ] || die 1 "could not create a temporary directory"
OLD=""
NEW=""
FINAL=""
cleanup() {
  # If an install was interrupted after moving the old app aside, put it back.
  if [ -n "$OLD" ] && [ -d "$OLD" ] && [ -n "$FINAL" ] && [ ! -e "$FINAL" ]; then
    mv "$OLD" "$FINAL" 2>/dev/null || true
  fi
  if [ -n "$NEW" ] && [ -d "$NEW" ]; then
    rm -rf "${NEW:?}"
  fi
  rm -rf "${TMP:?}"
}
trap cleanup EXIT

# Plain https unless the base URL is overridden (tests use file://).
fetch() { # fetch <name>
  if [ -z "${DELILAH_BASE_URL:-}" ]; then
    curl -fL --retry 2 --proto '=https' --tlsv1.2 -sS -o "$TMP/$1" "$BASE_URL/$1"
  else
    curl -fL --retry 2 -sS -o "$TMP/$1" "$BASE_URL/$1"
  fi
}

# 3. Download
echo "Downloading $ASSET ..."
fetch "$ASSET" || die 9 "could not download $BASE_URL/$ASSET"

HAVE_SUMS=1
if ! fetch "$SUMS" 2>/dev/null; then
  HAVE_SUMS=0
  if [ "${DELILAH_SKIP_CHECKSUM:-}" != "1" ]; then
    die 3 "this release has no $SUMS, so the download can't be verified.
Only releases before 1.1.0 lack it. To install anyway: DELILAH_SKIP_CHECKSUM=1"
  fi
fi

# 4/5. Verify the checksum before extracting anything
if [ "$HAVE_SUMS" -eq 1 ]; then
  EXPECTED="$(awk -v n="$ASSET" '$2 == n || $2 == "*" n { print $1; exit }' "$TMP/$SUMS")"
  ACTUAL="$(shasum -a 256 "$TMP/$ASSET" | awk '{ print $1 }')"
  if [ -z "$EXPECTED" ] || [ "$EXPECTED" != "$ACTUAL" ]; then
    die 4 "checksum mismatch for $ASSET; nothing was installed"
  fi
  echo "Checksum OK."
else
  echo "WARNING: DELILAH_SKIP_CHECKSUM=1 set and no $SUMS found: this download is UNVERIFIED." >&2
fi

# 6. Extract; the archive must contain exactly Delilah.app
mkdir -p "$TMP/extract"
tar -xzf "$TMP/$ASSET" -C "$TMP/extract" || die 5 "could not extract $ASSET"
ENTRIES="$(ls -A "$TMP/extract")"
if [ "$ENTRIES" != "Delilah.app" ] || [ ! -d "$TMP/extract/Delilah.app" ]; then
  die 5 "the archive must contain exactly Delilah.app"
fi

# 7. Never replace a running app
if pgrep -x delilah-app >/dev/null 2>&1; then
  die 6 "Quit Delilah first, then run this again"
fi

# 9. Writable target (no sudo here)
if [ ! -d "$INSTALL_DIR" ] || [ ! -w "$INSTALL_DIR" ]; then
  die 8 "$INSTALL_DIR is not writable. Re-run with sudo, or use DELILAH_INSTALL_DIR=~/Applications"
fi

FINAL="$INSTALL_DIR/Delilah.app"

# 8. Replace atomically: stage next to the target, move the old one aside, swap
if [ -e "$FINAL" ]; then
  if [ "$YES" -ne 1 ] && [ -t 0 ]; then
    printf '%s exists. Replace it? [y/N] ' "$FINAL"
    read -r answer
    case "$answer" in
      y | Y | yes | YES) ;;
      *) echo "Aborted; nothing changed."; exit 0 ;;
    esac
  fi
fi

NEW="$INSTALL_DIR/Delilah.app.new-$$"
if ! ditto "$TMP/extract/Delilah.app" "$NEW"; then
  die 7 "copying the app failed; any previous install is untouched"
fi
# Clear quarantine defensively (curl doesn't set it, a copied file might carry it)
xattr -dr com.apple.quarantine "$NEW" 2>/dev/null || true

if [ -e "$FINAL" ]; then
  OLD="$INSTALL_DIR/Delilah.app.old-$$"
  mv "$FINAL" "$OLD" || die 7 "could not move the existing app aside"
fi
if ! mv "$NEW" "$FINAL"; then
  if [ -n "$OLD" ]; then mv "$OLD" "$FINAL" || true; fi
  die 7 "install failed; the previous app was restored"
fi
NEW=""
if [ -n "$OLD" ]; then
  rm -rf "${OLD:?}"
  OLD=""
fi

# 11. Report
VERSION="$(defaults read "$FINAL/Contents/Info" CFBundleShortVersionString 2>/dev/null || true)"
if [ -z "$VERSION" ]; then
  VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$FINAL/Contents/Info.plist" 2>/dev/null || echo unknown)"
fi
echo "Installed Delilah $VERSION to $FINAL"
echo "Open it with: open -a Delilah"
