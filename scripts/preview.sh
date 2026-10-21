#!/usr/bin/env bash
# A preview of this checkout's Delilah (or of any worktree's), next to the one you work
# with: its own daemon on its own port and its own state, and the UI of this branch in
# the browser. Nothing is shared with the installed app (7420, ~/.delilah) or with
# scripts/dev-daemon.sh, so it can be started, broken and restarted freely.
#
#   scripts/preview.sh             # first free slot; Ctrl-C stops everything
#   scripts/preview.sh --slot 2    # a fixed slot: daemon on 7502, UI on 1502
#   scripts/preview.sh --fresh     # start from an empty state
#   scripts/preview.sh --no-ui     # only the daemon (API, CLI, curl)
#
# State lives in fixtures/preview-home-<slot> (git-ignored), with the fake repos of
# scripts/make-fixture-parent.sh to register as projects. Notifications are off
# unless --notify. Agents launched from the preview are real Claude sessions.
#
#   DELILAH_GH_BIN=scripts/fake-gh.sh FAKE_GH_DIR=… scripts/preview.sh   # PRs from a fake gh (specs/003-pr-loop/quickstart.md)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIX="$ROOT/fixtures"
SLOT="" FRESH=0 UI=1 NOTIFY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --slot) SLOT="$2"; shift 2 ;;
    --fresh) FRESH=1; shift ;;
    --no-ui) UI=0; shift ;;
    --notify) NOTIFY=1; shift ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
done

in_use() { lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; }
if [[ -z "$SLOT" ]]; then
  for n in 1 2 3 4 5 6 7 8 9; do
    if ! in_use $((7500 + n)) && ! in_use $((1500 + n)); then SLOT=$n; break; fi
  done
  [[ -n "$SLOT" ]] || { echo "every preview slot (1-9) is taken" >&2; exit 1; }
fi
API_PORT=$((7500 + SLOT))
UI_PORT=$((1500 + SLOT))
for port in "$API_PORT" "$UI_PORT"; do
  if in_use "$port"; then echo "port $port is taken: is preview $SLOT already running?" >&2; exit 1; fi
done
API="http://127.0.0.1:$API_PORT"
UI_URL="http://localhost:$UI_PORT"
HOME_DIR="$FIX/preview-home-$SLOT"

[[ -d "$FIX/parent-env" ]] || "$ROOT/scripts/make-fixture-parent.sh" "$FIX/parent-env" >/dev/null
[[ -d "$FIX/signal" ]] || git clone -q "$FIX/parent-env/remotes/svc-orders.git" "$FIX/signal"
[[ $FRESH == 1 ]] && rm -rf "$HOME_DIR"
mkdir -p "$HOME_DIR"

echo "building this branch's delilahd and delilah…"
cargo build -q -p delilah-daemon -p delilah-cli --manifest-path "$ROOT/Cargo.toml"
if [[ $UI == 1 && ! -d "$ROOT/app/node_modules" ]]; then
  echo "installing the UI's packages (first preview in this worktree)…"
  npm --prefix "$ROOT/app" ci --silent
fi

# The daemon gets the environment launchd would give it, as in scripts/dev-daemon.sh:
# a Claude launched from another Claude session would not behave as in production.
[[ -n "${DELILAH_GH_BIN:-}" && "$DELILAH_GH_BIN" != /* ]] && DELILAH_GH_BIN="$PWD/$DELILAH_GH_BIN"
[[ -n "${FAKE_GH_DIR:-}" && "$FAKE_GH_DIR" != /* ]] && FAKE_GH_DIR="$PWD/$FAKE_GH_DIR"
KEEP=()
for var in HTTPS_PROXY HTTP_PROXY NO_PROXY ALL_PROXY https_proxy http_proxy no_proxy all_proxy SSL_CERT_FILE NODE_EXTRA_CA_CERTS DELILAH_GH_BIN FAKE_GH_DIR; do
  [[ -n "${!var:-}" ]] && KEEP+=("$var=${!var}")
done
LOG="$HOME_DIR/delilahd.log"
env -i "${KEEP[@]+"${KEEP[@]}"}" \
  HOME="$HOME" USER="$USER" SHELL="${SHELL:-/bin/zsh}" LANG="${LANG:-es_ES.UTF-8}" \
  PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin" \
  DELILAH_HOME="$HOME_DIR" DELILAH_ADDR="127.0.0.1:$API_PORT" DELILAH_UI_ORIGIN="$UI_URL" \
  DELILAH_ALLOW_FILE_PROTOCOL=1 DELILAH_NOTIFY="$NOTIFY" RUST_LOG="${RUST_LOG:-info}" \
  "$ROOT/target/debug/delilahd" >>"$LOG" 2>&1 &
DAEMON=$!
UI_PID=""
stop() {
  [[ -n "$UI_PID" ]] && kill "$UI_PID" 2>/dev/null
  kill "$DAEMON" 2>/dev/null
  wait 2>/dev/null
  echo "preview $SLOT stopped"
}
trap stop EXIT INT TERM

for _ in $(seq 1 60); do
  curl -fs "$API/api/v1/health" >/dev/null 2>&1 && break
  kill -0 "$DAEMON" 2>/dev/null || { echo "delilahd exited; see $LOG" >&2; tail -20 "$LOG" >&2; exit 1; }
  sleep 0.25
done

cat <<EOF

Delilah preview $SLOT ($(git -C "$ROOT" branch --show-current 2>/dev/null || echo "?"))
  API    $API
  UI     $([[ $UI == 1 ]] && echo "$UI_URL" || echo "(off)")
  state  $HOME_DIR
  logs   $LOG
  CLI    DELILAH_URL=$API $ROOT/target/debug/delilah …
  repos  $FIX/parent-env/parent (parent) · $FIX/signal (repo)

EOF

if [[ $UI == 1 ]]; then
  VITE_DELILAH_URL="$API" npm --prefix "$ROOT/app" exec -- vite "$ROOT/app" --port "$UI_PORT" --strictPort --clearScreen false &
  UI_PID=$!
  wait "$UI_PID"
else
  wait "$DAEMON"
fi
