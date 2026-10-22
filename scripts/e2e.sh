#!/usr/bin/env bash
# End-to-end tests of Delilah against a fake Claude and a fake gh: each test starts its
# own daemon with its own temp home and port, so it is safe to run anywhere, next to the
# installed app (7420, ~/.delilah) and the previews. It never runs the real claude or gh.
#
#   scripts/e2e.sh                  # both layers: API/CLI, then UI (Playwright)
#   scripts/e2e.sh api              # API/CLI layer only
#   scripts/e2e.sh ui               # UI layer only
#   scripts/e2e.sh api -- <filter>  # only the API tests whose name matches <filter>
#   scripts/e2e.sh --real-claude    # only the real-Claude smoke test: USES YOUR SUBSCRIPTION
#
# Exit code is non-zero if any layer failed. Details: specs/005-e2e-tests/quickstart.md.
set -euo pipefail

unset DELILAH_URL DELILAH_HOME DELILAH_ADDR DELILAH_AGENT_BIN DELILAH_GH_BIN FAKE_GH_DIR

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

API=1 UI=1 REAL=0 FILTER=()
if [[ $# -gt 0 ]]; then
  case "$1" in
    -h|--help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    api) UI=0; shift ;;
    ui) API=0; shift ;;
    --real-claude) API=0 UI=0 REAL=1; shift ;;
    --) ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
fi
if [[ $# -gt 0 ]]; then
  [[ "$1" == "--" ]] || { echo "unexpected argument: $1 (see --help)" >&2; exit 2; }
  shift
  FILTER=("$@")
fi

echo "building the daemon, the CLI and the harness..."
cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e

FAILED=0
API_RESULT="" UI_RESULT="" REAL_RESULT=""

# run <name> <command...>: runs it and records "PASS|FAIL (Ns)" in the <name>_RESULT variable.
run() {
  local name="$1" start status=0
  shift
  start=$SECONDS
  "$@" || status=$?
  local label="PASS"
  if [[ $status -ne 0 ]]; then label="FAIL"; FAILED=1; fi
  printf -v "${name}_RESULT" '%s (%ss)' "$label" $((SECONDS - start))
}

api_layer() {
  cargo test -p delilah-e2e --features e2e -- ${FILTER[@]+"${FILTER[@]}"}
}

ui_layer() {
  [[ -d app/node_modules ]] || npm --prefix app ci
  npx --prefix app playwright install chromium
  npm --prefix app run e2e
}

real_layer() {
  echo "WARNING: this runs the real Claude Code with your HOME and uses your subscription." >&2
  DELILAH_E2E_REAL_CLAUDE=1 cargo test -p delilah-e2e --features e2e --test real_claude -- --ignored
}

if [[ $API -eq 1 ]]; then run API api_layer; fi
if [[ $UI -eq 1 ]]; then run UI ui_layer; fi
if [[ $REAL -eq 1 ]]; then run REAL real_layer; fi

echo
[[ -z "$API_RESULT" ]] || echo "api: $API_RESULT"
[[ -z "$UI_RESULT" ]] || echo "ui: $UI_RESULT"
[[ -z "$REAL_RESULT" ]] || echo "real-claude: $REAL_RESULT"
exit "$FAILED"
