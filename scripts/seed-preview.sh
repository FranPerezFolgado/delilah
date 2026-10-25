#!/usr/bin/env bash
# Seeds a running preview (scripts/preview.sh) with projects and tasks to try chat mode:
# both fixture repos registered, a few backlog tasks, and one chat-mode task running
# with a prompt that makes Claude ask for permissions (the "¿Permitir?" cards).
#
#   scripts/seed-preview.sh [API]      # default http://127.0.0.1:7501 (slot 1)
#   SEED_NO_RUN=1 scripts/seed-preview.sh   # backlog only, no agent started
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export DELILAH_URL="${1:-${DELILAH_URL:-http://127.0.0.1:7501}}"
if [[ "$DELILAH_URL" == *":7420"* ]]; then
  echo "refusing to seed the installed daemon (7420): use a preview" >&2
  exit 1
fi
cli() { "$ROOT/target/debug/delilah" "$@"; }

cli status >/dev/null

add_project() {
  cli project ls 2>/dev/null | grep -qw "$2" || cli project add --kind "$1" --name "$2" "$3"
}
add_project repo signal "$ROOT/fixtures/signal"
add_project parent parent "$ROOT/fixtures/parent-env/parent"

# Prints the key Delilah gave the new free task.
new() { cli new --free --project "$@" | sed -n 's/^Saved \([^ ]*\) .*/\1/p'; }
has() { cli ls 2>/dev/null | grep -qF "$1"; }
seed() { has "$3" || echo "  + $(new "$@") $3"; }
echo "backlog:"
seed signal --title "Chat: probar «Permitir siempre» con prefijos" --chat \
  --notes "Pide ejecutar ls -la, cat README.md y luego cat ~/.ssh/config: el último debe volver a preguntar."
seed signal --title "Chat: cambiar a terminal y volver" --chat \
  --notes "Prueba el interruptor Chat/Terminal con el agente parado y trabajando."
seed signal --title "Terminal: tarea clásica en PTY" \
  --notes "Sin modo chat, para comparar la vista."
seed parent --title "Parent: tocar svc-orders sin rama" --chat \
  --notes "Tarea libre sin rama: un git -C svc-orders commit debe crear la rama antes."

LIVE="Chat: tarjetas de permiso en vivo"
if [[ -z "${SEED_NO_RUN:-}" ]] && ! has "$LIVE"; then
  echo "running (chat mode):"
  key=$(new signal --title "$LIVE" --chat)
  cli run "$key" --chat \
    --prompt "Hola. Para probar las tarjetas de permiso, ejecuta uno a uno, esperando mi respuesta a cada permiso: 1) ls -la  2) cat README.md  3) git diff --stat  4) cat ~/.ssh/config  5) crea notas.md con una línea. Después resume qué te dejé hacer." \
    >/dev/null && echo "  > $key $LIVE"
fi

cli ls
