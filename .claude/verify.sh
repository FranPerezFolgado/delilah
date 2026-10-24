#!/bin/bash
# Checks for the files an implementer changed, called by ~/.claude/hooks/verify-package.sh
# with those files as arguments (repo-relative). Mirrors "Stack & checks" in CLAUDE.md, plus the
# fmt and clippy gates from docs/development.md. Non-zero exit = the agent keeps working.
set -u
cd "$(git rev-parse --show-toplevel)" || exit 1

crates=() workspace=no tauri=no ts=()
for f in "$@"; do
  case "$f" in
    crates/*)
      dir=${f#crates/}; dir=crates/${dir%%/*}
      name=$(sed -nE 's/^name *= *"([^"]+)".*/\1/p' "$dir/Cargo.toml" 2>/dev/null | head -1)
      [ -n "$name" ] && [[ " ${crates[*]-} " != *" $name "* ]] && crates+=("$name") ;;
    Cargo.toml|Cargo.lock) workspace=yes ;;
    app/src-tauri/*) tauri=yes ;;
    app/src/*.ts|app/src/*.tsx|app/*.ts|app/package.json|app/tsconfig*.json) ts+=("${f#app/}") ;;
  esac
done

failed=0
run() { echo "== $*"; "$@" 2>&1 || { failed=1; echo "!! failed: $*"; }; }

if [ "$workspace" = yes ] || [ ${#crates[@]} -gt 0 ]; then
  pkgs=(--workspace)
  [ "$workspace" = no ] && { pkgs=(); for c in "${crates[@]}"; do pkgs+=(-p "$c"); done; }
  run cargo fmt --all --check
  run cargo clippy --quiet "${pkgs[@]}" --all-targets -- -D warnings
  run cargo test --quiet "${pkgs[@]}"
fi

if [ "$tauri" = yes ]; then
  m=app/src-tauri/Cargo.toml
  run cargo fmt --manifest-path "$m" --check
  run cargo clippy --quiet --manifest-path "$m" --all-targets -- -D warnings
  run cargo test --quiet --manifest-path "$m"
fi

if [ ${#ts[@]} -gt 0 ]; then
  ( cd app || exit 1
    run npx --no-install tsc --noEmit
    src=(); for f in "${ts[@]}"; do [[ "$f" == *.ts || "$f" == *.tsx ]] && src+=("$f"); done
    if [ ${#src[@]} -gt 0 ]; then run npx --no-install vitest related --run "${src[@]}"; else run npx --no-install vitest run; fi
    exit $failed ) || failed=1
fi

exit $failed
