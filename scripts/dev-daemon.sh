#!/usr/bin/env bash
# Runs delilahd for development against the fake repos in fixtures/ (git-ignored):
# a parent with submodules and a plain repo, state in fixtures/dev-home.
# The environment is cleaned the way launchd would leave it, so agents behave as in
# production (a Claude launched from another Claude session would not save its chats).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIX="$ROOT/fixtures"
[[ -d "$FIX/parent-env" ]] || "$ROOT/scripts/make-fixture-parent.sh" "$FIX/parent-env" >/dev/null
[[ -d "$FIX/signal" ]] || git clone -q "$FIX/parent-env/remotes/svc-orders.git" "$FIX/signal"

cargo build -q -p delilah-daemon --manifest-path "$ROOT/Cargo.toml"
exec env -i \
  HOME="$HOME" USER="$USER" SHELL="${SHELL:-/bin/zsh}" LANG="${LANG:-es_ES.UTF-8}" \
  PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin" \
  DELILAH_HOME="$FIX/dev-home" DELILAH_ALLOW_FILE_PROTOCOL=1 DELILAH_NOTIFY="${DELILAH_NOTIFY:-1}" \
  RUST_LOG="${RUST_LOG:-info}" \
  "$ROOT/target/debug/delilahd"
