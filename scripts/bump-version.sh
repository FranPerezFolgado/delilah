#!/usr/bin/env bash
# Bumps the version in the four places from docs/development.md#releasing and refreshes the
# lockfiles. Usage: scripts/bump-version.sh <major|minor|patch|X.Y.Z> [--commit]
# The base is the version on main (origin/main if fetched), as CLAUDE.md asks; --commit
# commits the result as "chore: bump version to X.Y.Z".
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

level=${1:-}; commit=${2:-}
[[ -n "$level" ]] || { echo "usage: $0 <major|minor|patch|X.Y.Z> [--commit]" >&2; exit 1; }

base_ref=main
git rev-parse -q --verify origin/main >/dev/null && base_ref=origin/main
base=$(git show "$base_ref:Cargo.toml" | sed -nE 's/^version *= *"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/p' | head -1)
[[ -n "$base" ]] || { echo "could not read the version on $base_ref" >&2; exit 1; }
IFS=. read -r major minor patch <<<"$base"

case "$level" in
  major) new="$((major + 1)).0.0" ;;
  minor) new="$major.$((minor + 1)).0" ;;
  patch) new="$major.$minor.$((patch + 1))" ;;
  [0-9]*.[0-9]*.[0-9]*) new=$level ;;
  *) echo "unknown level: $level" >&2; exit 1 ;;
esac

# First `version = ` line of each manifest is the package/workspace version.
for f in Cargo.toml app/src-tauri/Cargo.toml; do
  perl -0pi -e "s/^version = \"[^\"]+\"/version = \"$new\"/m" "$f"
done
for f in app/package.json app/src-tauri/tauri.conf.json; do
  perl -0pi -e "s/\"version\": \"[^\"]+\"/\"version\": \"$new\"/" "$f"
done

# Lockfiles only, no build (the desktop shell's build script needs the sidecar and the web UI).
cargo update --workspace --offline --quiet
(cd app/src-tauri && cargo update --workspace --offline --quiet)
# Only the app's own entries: `npm install --package-lock-only` would also prune optional deps.
node -e '
  const fs = require("fs"), p = "app/package-lock.json", lock = JSON.parse(fs.readFileSync(p));
  lock.version = lock.packages[""].version = process.argv[1];
  fs.writeFileSync(p, JSON.stringify(lock, null, 2) + "\n");
' "$new"

echo "$base ($base_ref) -> $new"
if [[ "$commit" == --commit ]]; then
  git add Cargo.toml Cargo.lock app/src-tauri/Cargo.toml app/src-tauri/Cargo.lock \
    app/package.json app/package-lock.json app/src-tauri/tauri.conf.json
  git commit -q -m "chore: bump version to $new"
  echo "committed: chore: bump version to $new"
fi
