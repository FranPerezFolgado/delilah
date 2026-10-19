#!/usr/bin/env bash
# Builds delilahd and puts it where Tauri bundles it into Delilah.app
# (bundle.externalBin needs the target triple in the file name).
# Usage: scripts/build-sidecar.sh [release|debug]   (default: release)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="${1:-release}"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
FLAGS=()
[[ "$PROFILE" == "release" ]] && FLAGS+=(--release)

cargo build -q -p delilah-daemon "${FLAGS[@]+"${FLAGS[@]}"}" --manifest-path "$ROOT/Cargo.toml"
mkdir -p "$ROOT/app/src-tauri/binaries"
cp "$ROOT/target/$PROFILE/delilahd" "$ROOT/app/src-tauri/binaries/delilahd-$TRIPLE"
echo "sidecar: app/src-tauri/binaries/delilahd-$TRIPLE ($PROFILE)"
