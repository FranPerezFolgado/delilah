#!/usr/bin/env bash
# Seeds a preview of Delilah with every state the redesigned boards show, so the UI can be
# reviewed without building each situation by hand. Never touches the installed app: it
# refuses 7420.
#
#   scripts/seed-preview.sh --serve [--slot N] [--fresh]
#       A whole seeded preview: a daemon wired to the fake claude and the fake gh (nothing
#       real runs), the UI of this checkout, and the seed below. Prints PREVIEW_URL=<ui>;
#       Ctrl-C stops it. This is the one that shows every state.
#   scripts/seed-preview.sh [API]
#       Seeds an already running preview (default http://127.0.0.1:7501, slot 1), e.g.
#       scripts/preview.sh --fresh. scripts/preview.sh gives its daemon a real Claude, so
#       only the states that need no agent are created (marked [any] below); the rest are
#       skipped with a note. Give it the fakes of a daemon you started yourself with
#       SEED_FAKES_DIR=<fixtures/seed-fakes> to create them all (see --serve for the env).
#
# Creates (per state, against a fresh daemon):
#   [any]   Spanish UI, Jira connected with «Datos de ejemplo»
#   [any]   projects signal (repo, worktrees) and delilah (parent, the fixture parent env)
#   [any]   3 backlog tasks (chat, terminal, parent) and one more «created yesterday-ish»
#           (its created_at is only as old as the seed: no API sets it)
#   [any]   a task in setup-failed (signal's setup command is `exit 3` only while it starts)
#   [fakes] a task waiting on a question (AskUserQuestion)
#   [fakes] a chat-mode task with a pending permission («Te espera»)
#   [fakes] a running task, a task paused by the usage limit
#   [fakes] a done task whose PR (fake gh) is merged, and an open PR with a failing check
#           and a review (the pr-opened aviso)
#   [fakes] a free-flow task (no branch) with several sessions: the primary, a second Claude
#           (its finished turn is the turn-done aviso) and a terminal
#   [fakes] a base-stale aviso (a task started while origin was unreachable) with origin
#           ahead, so the task shows «main avanzó N commits»
# Cannot create: a plain `failed` task (no fake path reaches it), a task finished by a
# different account, and anything native (notifications, Dock badge, menu).
#
# Needs: jq, curl, git, and `cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e`
# (fake-claude) for the fakes. Safe to run twice: each state is skipped when its title exists.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIX="$ROOT/fixtures"
FAKES="${SEED_FAKES_DIR:-}"
SERVE=0 SLOT="" FRESH=0 API_ARG=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --serve) SERVE=1; shift ;;
    --slot) SLOT="$2"; shift 2 ;;
    --fresh) FRESH=1; shift ;;
    -h|--help) sed -n '2,/^[^#]/{/^#/p;}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    http*) API_ARG="$1"; shift ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
done
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }

# ---------- the fakes: the e2e harness's claude and gh, outside any daemon ----------

# Lays out $FIX/seed-fakes like crates/delilah-e2e/src/env.rs `layout`.
prepare_fakes() {
  local d="$FIX/seed-fakes" bin="$ROOT/target/debug/fake-claude"
  [[ -x "$bin" ]] || { echo "build the fake claude first: cargo build -p delilah-e2e" >&2; exit 1; }
  mkdir -p "$d/home" "$d/bin" "$d/fake/scenarios" "$d/fake/records" "$d/gh"
  printf '[user]\n\tname = Delilah Seed\n\temail = seed@delilah.invalid\n[protocol "file"]\n\tallow = always\n[commit]\n\tgpgsign = false\n' >"$d/home/.gitconfig"
  printf '#!/bin/sh\nexec %q --fake-dir %q "$@"\n' "$bin" "$d/fake" >"$d/bin/claude-fake"
  printf '#!/bin/sh\nFAKE_GH_DIR=%q exec %q "$@"\n' "$d/gh" "$ROOT/scripts/fake-gh.sh" >"$d/bin/gh-fake"
  # The daemon's one-shot Claude calls (branch names): fail, so it falls back to its own.
  printf '#!/bin/sh\nexit 1\n' >"$d/bin/claude-oneshot"
  chmod +x "$d/bin/"*
  echo '{"steps":[{"stop":"Fake Claude stopped."}]}' >"$d/fake/scenarios/default.json"
  FAKES="$d"
}

scenario() { printf '%s' "$2" >"$FAKES/fake/scenarios/$1.json"; }
gh_put() { printf '%s' "$2" >"$FAKES/gh/.$1.tmp" && mv "$FAKES/gh/.$1.tmp" "$FAKES/gh/$1"; }

REPO_URL="https://github.com/o/r"
# One fake PR: number, branch, title, state, build conclusion, optional review text.
fake_pr() {
  local n=$1 branch=$2 title=$3 state=$4 ci=$5 review=${6:-} sha=${7:-1}
  jq -n --argjson n "$n" --arg b "$branch" --arg t "$title" --arg s "$state" --arg ci "$ci" --arg r "$review" --arg sha "$sha" --arg u "$REPO_URL" '
    {data:{repository:{pullRequest:{
      state:$s, isDraft:false, merged:($s=="MERGED"), headRefOid:$sha, baseRefName:"main",
      title:$t, url:"\($u)/pull/\($n)",
      reviewDecision:(if $r!="" then "CHANGES_REQUESTED" elif $s=="MERGED" then "APPROVED" else null end),
      commits:{nodes:[{commit:{statusCheckRollup:{contexts:{nodes:[
        {__typename:"CheckRun",name:"build",status:"COMPLETED",conclusion:$ci,detailsUrl:"https://ci.invalid/1"},
        {__typename:"CheckRun",name:"lint",status:"COMPLETED",conclusion:"SUCCESS",detailsUrl:"https://ci.invalid/2"}]}}}}]},
      reviews:{nodes:(if $r!="" then [{databaseId:(100+$n),state:"CHANGES_REQUESTED",author:{login:"reviewer"},body:$r,submittedAt:"2026-10-20T10:00:00Z"}] else [] end)},
      comments:{nodes:[]}, reviewThreads:{nodes:[]}}}}}' >"$FAKES/gh/.pr-$n.tmp"
  mv "$FAKES/gh/.pr-$n.tmp" "$FAKES/gh/pr-$n.json"
  jq -n --argjson n "$n" --arg b "$branch" --arg t "$title" --arg s "$state" --arg u "$REPO_URL" \
    '{number:$n,url:"\($u)/pull/\($n)",state:$s,updatedAt:"2026-10-20T10:00:00Z",headRefName:$b,baseRefName:"main",title:$t,isDraft:false}'
}

# ---------- the daemon (--serve) ----------

if [[ $SERVE == 1 ]]; then
  in_use() { lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; }
  if [[ -z "$SLOT" ]]; then
    for n in 1 2 3 4 5 6 7 8 9; do
      if ! in_use $((7500 + n)) && ! in_use $((1500 + n)); then SLOT=$n; break; fi
    done
    [[ -n "$SLOT" ]] || { echo "every preview slot (1-9) is taken" >&2; exit 1; }
  fi
  API_PORT=$((7500 + SLOT)); UI_PORT=$((1500 + SLOT))
  for p in "$API_PORT" "$UI_PORT"; do
    if in_use "$p"; then echo "port $p is taken: is preview $SLOT already running?" >&2; exit 1; fi
  done
  API_ARG="http://127.0.0.1:$API_PORT"
  UI_URL="http://localhost:$UI_PORT"
  HOME_DIR="$FIX/preview-home-$SLOT"
  [[ -d "$FIX/parent-env" ]] || "$ROOT/scripts/make-fixture-parent.sh" "$FIX/parent-env" >/dev/null
  [[ -d "$FIX/signal" ]] || git clone -q "$FIX/parent-env/remotes/svc-orders.git" "$FIX/signal"
  if [[ $FRESH == 1 ]]; then
    rm -rf "$HOME_DIR" "$FIX/seed-fakes"
    git -C "$FIX/signal" worktree prune
    # Branches an earlier seed left in the fixture repo would make the same task names collide.
    git -C "$FIX/signal" for-each-ref --format='%(refname:short)' 'refs/heads/feature/libre-*' | xargs -n1 git -C "$FIX/signal" branch -D -q 2>/dev/null || true
  fi
  mkdir -p "$HOME_DIR"
  prepare_fakes
  cargo build -q -p delilah-daemon -p delilah-cli -p delilah-e2e --manifest-path "$ROOT/Cargo.toml"
  [[ -d "$ROOT/app/node_modules" ]] || npm --prefix "$ROOT/app" ci --silent
  # Only fakes run: the env is the e2e harness's (crates/delilah-e2e/src/env.rs daemon_env).
  env -i HOME="$FAKES/home" USER="$USER" SHELL=/bin/zsh LANG="${LANG:-es_ES.UTF-8}" \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin" \
    DELILAH_HOME="$HOME_DIR" DELILAH_ADDR="127.0.0.1:$API_PORT" DELILAH_UI_ORIGIN="$UI_URL" \
    DELILAH_ALLOW_FILE_PROTOCOL=1 DELILAH_NOTIFY=0 DELILAH_SECRETS=memory DELILAH_PR_POLL_SECS=1 \
    DELILAH_AGENT_BIN="$FAKES/bin/claude-fake" DELILAH_CLAUDE_BIN="$FAKES/bin/claude-oneshot" \
    DELILAH_GH_BIN="$FAKES/bin/gh-fake" RUST_LOG="${RUST_LOG:-info}" \
    "$ROOT/target/debug/delilahd" >>"$HOME_DIR/delilahd.log" 2>&1 &
  DAEMON=$!
  UI_PID=""
  stop() {
    [[ -n "$UI_PID" ]] && kill "$UI_PID" 2>/dev/null
    kill "$DAEMON" 2>/dev/null
    wait 2>/dev/null
    echo "seeded preview $SLOT stopped"
  }
  trap stop EXIT INT TERM
  for _ in $(seq 1 60); do
    curl -fs "$API_ARG/api/v1/health" >/dev/null 2>&1 && break
    kill -0 "$DAEMON" 2>/dev/null || { echo "delilahd exited; see $HOME_DIR/delilahd.log" >&2; exit 1; }
    sleep 0.25
  done
fi

API="${API_ARG:-${DELILAH_URL:-http://127.0.0.1:7501}}"
if [[ "$API" == *":7420"* ]]; then
  echo "refusing to seed the installed daemon (7420): use a preview" >&2
  exit 1
fi
[[ -n "$FAKES" && -z "${FAKES%%/*}" && ! -d "$FAKES" ]] && { echo "SEED_FAKES_DIR $FAKES does not exist" >&2; exit 1; }

# ---------- API helpers ----------

# api METHOD PATH [JSON]: prints the body, fails on an HTTP error.
api() {
  local m=$1 p=$2 b=${3:-}
  if [[ -n "$b" ]]; then
    curl -fsS -X "$m" "$API$p" -H 'content-type: application/json' -d "$b"
  else
    curl -fsS -X "$m" "$API$p"
  fi
}
has_title() { api GET /api/v1/tasks | jq -e --arg t "$1" 'any(.[]; .title == $t)' >/dev/null; }
key_of() { api GET /api/v1/tasks | jq -r --arg t "$1" '[.[] | select(.title == $t)][0].key'; }
status_of() { api GET "/api/v1/tasks/$1" | jq -r .status.type; }
wait_status() {
  for _ in $(seq 1 120); do
    [[ "$(status_of "$1")" == "$2" ]] && return 0
    sleep 0.5
  done
  echo "  ! $1 did not reach $2 (is $(status_of "$1"))" >&2
  return 1
}
# Waits until the inbox lists an item matching a jq test.
wait_inbox() {
  for _ in $(seq 1 120); do
    api GET /api/v1/inbox | jq -e "$1" >/dev/null && return 0
    sleep 0.5
  done
  echo "  ! inbox never matched: $1" >&2
  return 1
}
say() { echo "  + $*"; }

api GET /api/v1/health >/dev/null || { echo "no daemon answers on $API" >&2; exit 1; }
[[ -z "$FAKES" ]] && echo "note: no fake agent given (SEED_FAKES_DIR): only the [any] states are created; use --serve for all"

# ---------- projects, preferences, Jira ----------

api PUT /api/v1/preferences/language '"es"' >/dev/null
echo "projects:"
add_project() {
  api GET /api/v1/projects | jq -e --arg n "$2" 'any(.[]; .name == $n)' >/dev/null && return 0
  api POST /api/v1/projects "$(jq -n --arg k "$1" --arg n "$2" --arg p "$3" '{kind:$k,name:$n,path:$p}')" >/dev/null
  say "$2"
}
add_project repo signal "$FIX/signal"
add_project parent delilah "$FIX/parent-env/parent"
signal_settings() { api GET /api/v1/projects | jq -c '[.[] | select(.name == "signal")][0].settings'; }
BASE_SETTINGS="$(signal_settings | jq -c '. + {workspace:"worktree"}')"
api PUT /api/v1/projects/signal/settings "$BASE_SETTINGS" >/dev/null
echo "jira:"
api PUT /api/v1/jira '{"mode":"sample"}' >/dev/null && say "Datos de ejemplo"

# ---------- backlog ----------

echo "backlog:"
backlog() { # title project chat notes
  has_title "$1" && return 0
  local k
  k=$(api POST /api/v1/tasks "$(jq -n --arg t "$1" --arg p "$2" --argjson c "$3" --arg n "$4" \
    '{key:"",free:true,title:$t,project:$p,chat_mode:$c,notes:$n}')" | jq -r .key)
  say "$k $1"
}
backlog "Chat: probar «Permitir siempre» con prefijos" signal true "Pide ejecutar ls -la, cat README.md y luego cat ~/.ssh/config: el último debe volver a preguntar."
backlog "Terminal: tarea clásica en PTY" signal false "Sin modo chat, para comparar la vista."
backlog "Parent: tocar svc-orders sin rama" delilah true "Tarea libre sin rama: un git -C svc-orders commit debe crear la rama antes."
backlog "Revisar la política de reintentos" signal false "Anotada ayer: mirar si los reintentos son idempotentes."

# ---------- setup failed (no agent: the setup command fails before it starts) ----------

echo "setup failed:"
SETUP_TITLE="Actualizar common-lib a Java 21"
run() { # JSON body merged over the defaults
  api POST /api/v1/runs "$(jq -n --argjson b "$1" '{key:"",free:true,submodules:[],project:"signal"} + $b')"
}
if ! has_title "$SETUP_TITLE"; then
  api PUT /api/v1/projects/signal/settings "$(jq -c '. + {setup:"echo \"> npm ci\"; echo \"npm ERR! code ENOENT\" >&2; exit 3"}' <<<"$BASE_SETTINGS")" >/dev/null
  k=$(run "$(jq -n --arg t "$SETUP_TITLE" '{title:$t,branch:"feature/libre-setup-common-lib-java-21"}')" | jq -r .key)
  wait_status "$k" setup-failed && say "$k $SETUP_TITLE"
  api PUT /api/v1/projects/signal/settings "$BASE_SETTINGS" >/dev/null
fi

if [[ -z "$FAKES" ]]; then
  echo "skipped (need the fake agent): question, permission, running, paused, done+PR, free-flow, avisos, base-stale"
  api GET /api/v1/tasks | jq -r '.[] | "\(.key)\t\(.status.type)\t\(.title)"'
  exit 0
fi

# ---------- states that need an agent: the fake claude and fake gh ----------

echo "agents (fake claude, fake gh):"
B_DONE="feature/libre-reintentos-webhook"
B_PR="feature/libre-tarjetas-permiso"
B_RUN="feature/libre-paginacion-cursor"
B_PAUSE="feature/libre-metrica-latencia"
B_ASK="feature/libre-cursor-u-offset"
B_STALE="feature/libre-ruta-obsoleta"
repo_json='{"nameWithOwner":"o/r","url":"'"$REPO_URL"'","defaultBranchRef":{"name":"main"}}'
gh_put repo.json "$repo_json"
gh_put user.json '{"login":"seed-user"}'
scenario seed-running "$(jq -nc '{steps:([{print:"Leyendo el plan de la spec…\r\nImplementando la paginación por cursor en GET /invoices\r\n"}] + [range(180) | {sleep_ms:5000}])}')"
scenario seed-paused '{"steps":[{"print":"Midiendo la latencia del gateway…\r\n"},{"fail":"API Error: 429 rate_limit_error: usage limit reached"}]}'
scenario seed-question '{"steps":[{"ask":{"question":"¿Paginamos por cursor o por offset?","header":"Paginación","options":["Cursor","Offset"]}},{"stop":"Hecho."}]}'
scenario seed-live '{"turns":[[
  {"say":{"text":"Vale. Voy a ejecutar los comandos uno a uno y esperar tu respuesta a cada permiso."}},
  {"tool":{"name":"Bash","input":{"command":"git status --short"},"output":" M README.md","ask":false}},
  {"say":{"text":"Hay un cambio en README.md. Ahora listo el directorio."}},
  {"tool":{"name":"Bash","input":{"command":"ls -la"},"output":"total 8\nREADME.md\nsrc"}},
  {"say":{"text":"Listado hecho."}}]]}'
scenario seed-turn '{"steps":[{"print":"Revisando los cambios de la rama…\r\n"},{"stop":"Revisión hecha: sin hallazgos."}]}'

agent_task() { # title, run JSON extras; waits are the caller's
  has_title "$1" && { key_of "$1"; return 0; }
  run "$(jq -n --arg t "$1" --argjson x "$2" '{title:$t} + $x')" | jq -r .key
}
announce() { say "$1 $2"; }

T="Reintentos con backoff en el webhook de Stripe"
new=0; has_title "$T" || new=1
k_done=$(agent_task "$T" "$(jq -n --arg b "$B_DONE" '{branch:$b}')")
new_pr=""
if [[ $new == 1 ]]; then
  new_pr=1
  wait_status "$k_done" waiting-for-user
fi

T="Chat: tarjetas de permiso en vivo"
new=0; has_title "$T" || new=1
k_live=$(agent_task "$T" "$(jq -n --arg b "$B_PR" '{branch:$b,chat_mode:true,prompt:"Prueba los permisos. [fake:seed-live]"}')")
[[ $new == 1 ]] && announce "$k_live" "$T (permission pending, PR #14 open)"

T="Paginación por cursor en GET /invoices"
new=0; has_title "$T" || new=1
k_run=$(agent_task "$T" "$(jq -n --arg b "$B_RUN" '{branch:$b,prompt:"Implementa [fake:seed-running]"}')")
[[ $new == 1 ]] && announce "$k_run" "$T (running)"

T="Métrica de latencia del gateway de pagos"
new=0; has_title "$T" || new=1
k_pause=$(agent_task "$T" "$(jq -n --arg b "$B_PAUSE" '{branch:$b,prompt:"Mide [fake:seed-paused]"}')")
[[ $new == 1 ]] && announce "$k_pause" "$T (paused by limit)"

T="Elegir paginación de GET /invoices"
new=0; has_title "$T" || new=1
k_ask=$(agent_task "$T" "$(jq -n --arg b "$B_ASK" '{branch:$b,prompt:"Decide la paginación [fake:seed-question]"}')")
[[ $new == 1 ]] && announce "$k_ask" "$T (question pending)"

# Free-flow: no branch, a second Claude and a terminal next to the primary.
T="Investigar timeouts del gateway"
new=0; has_title "$T" || new=1
k_free=$(agent_task "$T" '{}')
if [[ $new == 1 ]]; then
  wait_status "$k_free" waiting-for-user
  api POST "/api/v1/tasks/$k_free/sessions" '{"kind":"claude","label":"Revisión","prompt":"Revisa los cambios de esta rama [fake:seed-turn]"}' >/dev/null
  api POST "/api/v1/tasks/$k_free/sessions" '{"kind":"shell"}' >/dev/null
  announce "$k_free" "$T (free-flow, 3 sessions)"
fi

# Base-stale: the task starts while origin is unreachable (a failed fetch is the aviso), and
# origin then moves on, so the branch is behind. Origin is restored either way.
T="Ruta obsoleta en el cliente de pagos"
if ! has_title "$T"; then
  sleep 6   # a fetch under 5 s old is reused (delilah-git base.rs FRESH_FOR): wait it out
  origin=$(git -C "$FIX/signal" remote get-url origin)
  git -C "$FIX/signal" remote set-url origin "$FIX/does-not-exist.git"
  k_stale=$(run "$(jq -n --arg t "$T" --arg b "$B_STALE" '{title:$t,branch:$b}')" | jq -r .key) || true
  wait_status "$k_stale" waiting-for-user || true   # the base is fetched while it starts
  git -C "$FIX/signal" remote set-url origin "$origin"
  tmp=$(mktemp -d)
  if git -c protocol.file.allow=always clone -q "$origin" "$tmp/c" 2>/dev/null; then
    git -C "$tmp/c" -c user.name=Seed -c user.email=seed@delilah.invalid commit -q --allow-empty -m "seed: main avanza 1"
    git -C "$tmp/c" -c user.name=Seed -c user.email=seed@delilah.invalid commit -q --allow-empty -m "seed: main avanza 2"
    git -C "$tmp/c" push -q origin HEAD 2>/dev/null || true
  fi
  rm -rf "$tmp"
  announce "${k_stale:-?}" "$T (base-stale aviso, origin ahead by 2)"
fi

# PRs: the loop looks for a branch's PR once the task is in review, and a PR whose head is
# the branch tip (as on GitHub) is the one it links. The merged one is linked before the
# task is marked done.
tip() { git -C "$FIX/signal" rev-parse "$1"; }
if [[ -n "${new_pr:-}" ]]; then
  # The daemon may have suffixed the branch (-2) when an older seed left one: the fake PRs
  # must name the branch the task really has, or gh --head matches no (or the wrong) task.
  B_DONE=$(api GET "/api/v1/tasks/$k_done" | jq -r --arg d "$B_DONE" '.branch // $d')
  B_PR=$(api GET "/api/v1/tasks/$k_live" | jq -r --arg d "$B_PR" '.branch // $d')
  pr12=$(fake_pr 12 "$B_DONE" "Reintentos con backoff en el webhook de Stripe" MERGED SUCCESS "" "$(tip "$B_DONE")")
  pr14=$(fake_pr 14 "$B_PR" "Chat: tarjetas de permiso en vivo" OPEN FAILURE "El test de la tarjeta de permiso falla en CI, ¿lo miras?" "$(tip "$B_PR")")
  gh_put pr-list.json "$(jq -n --argjson a "$pr12" --argjson b "$pr14" '[$a,$b]')"
  for k in "$k_live" "$k_done"; do api POST "/api/v1/tasks/$k/phase" '{"phase":"review"}' >/dev/null; done
  for _ in $(seq 1 60); do
    api GET "/api/v1/tasks/$k_done/prs" | jq -e '.prs | length > 0' >/dev/null && break
    sleep 0.5
  done
  api POST "/api/v1/tasks/$k_done/done" >/dev/null && announce "$k_done" "Reintentos con backoff en el webhook de Stripe (done, PR #12 merged)"
fi

echo "waiting for the states..."
wait_status "$k_run" running || true
wait_status "$k_pause" paused-by-limit || true
wait_status "$k_ask" waiting-for-user || true
wait_inbox 'any(.[]; (.kind|tostring) | test("question|ask"))' || true
wait_inbox 'any(.[]; .kind == "permission" or ((.kind|tostring) | test("permission")))' || true
wait_inbox 'any(.[]; .kind == "turn-done")' || true
wait_inbox 'any(.[]; .kind == "pr-opened")' || true
for entry in "$k_run:plan" "$k_pause:implement" "$k_ask:implement"; do
  api POST "/api/v1/tasks/${entry%%:*}/phase" "{\"phase\":\"${entry##*:}\"}" >/dev/null || true
done

echo
echo "tasks:"
api GET /api/v1/tasks | jq -r '.[] | "  \(.key)\t\(.status.type)\t\(.title)"'
echo "inbox:"
api GET /api/v1/inbox | jq -r '.[] | "  \(.kind)\t\(.task_key // "-")"'

if [[ $SERVE == 1 ]]; then
  cat <<EOF

PREVIEW_URL=$UI_URL
Seeded Delilah preview $SLOT: UI $UI_URL  API $API  state $HOME_DIR (fakes: $FAKES)
Ctrl-C stops it.
EOF
  VITE_DELILAH_URL="$API" npm --prefix "$ROOT/app" exec -- vite "$ROOT/app" --port "$UI_PORT" --strictPort --clearScreen false &
  UI_PID=$!
  wait "$UI_PID"
fi
