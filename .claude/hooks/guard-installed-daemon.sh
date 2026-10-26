#!/bin/bash
# PreToolUse hook (Bash): keeps agents off the installed Delilah (daemon on 7420, ~/.delilah),
# which runs the agents working on this repo. Changes are tried in a preview (scripts/preview.sh).
#
# Blocks: killing or unloading delilahd, running scripts/dev-daemon.sh (it binds 7420), writes to
# the 7420 API, the CLI without a DELILAH_URL (it defaults to 7420), and deleting ~/.delilah.
# Reading code or docs that mention 7420 is fine. Each statement (split on ; && || and newlines,
# pipes kept together) is checked on its own. Exit 2 sends the reason back to the agent.

cmd=$(jq -r '.tool_input.command // empty')
[ -z "$cmd" ] && exit 0

# Heredoc bodies are data (docs, scripts being written), not commands: drop them.
cmd=$(awk '
  delim != "" { line = $0; if (strip) sub(/^\t+/, "", line); if (line == delim) delim = ""; next }
  { print }
  match($0, /<<-?[ \t]*["'\'']?[A-Za-z_][A-Za-z0-9_]*/) {
    d = substr($0, RSTART, RLENGTH); strip = (d ~ /^<<-/)
    sub(/^<<-?[ \t]*["'\'']?/, "", d); delim = d
  }' <<<"$cmd")

block() {
  {
    echo "Blocked: $1"
    echo "The installed Delilah (7420, ~/.delilah) runs the agents on this repo; never stop it or try changes on it."
    echo "Try changes in a preview: scripts/preview.sh (in the background), then DELILAH_URL=<its API> target/debug/delilah …"
    echo "If this really must touch the installed app, ask the user to run it themselves."
  } >&2
  exit 2
}

installed='(127\.0\.0\.1|localhost|\[::1\]):7420'
# Optional env assignments and launcher words before the program a statement runs.
lead='^[[:space:]]*([A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+)*((bash|sh|zsh|exec|nohup|env)[[:space:]]+)*'

check() {
  local s=$1
  grep -qE '\b(pkill|killall)\b.*delilah' <<<"$s" && block "killing delilahd"
  grep -qE '\bkill\b' <<<"$s" && grep -qE 'delilahd|7420' <<<"$s" && block "killing the installed daemon"
  grep -qE '\blaunchctl\b.*\b(bootout|unload|stop|kill|kickstart|remove|disable)\b.*delilah' <<<"$s" \
    && block "unloading the installed daemon's launchd job"
  grep -qE "${lead}(\./)?scripts/dev-daemon\.sh" <<<"$s" && block "scripts/dev-daemon.sh binds 7420"
  grep -qE "$installed" <<<"$s" \
    && grep -qE '(-X *|--request *)(POST|PUT|PATCH|DELETE)|(^|[[:space:]])(-d|--data[a-z-]*|-F|--form)[[:space:]=]' <<<"$s" \
    && block "writing to the installed daemon's API"
  grep -qE "DELILAH_URL=[^[:space:]]*$installed" <<<"$s" && block "pointing the CLI at the installed daemon"
  grep -qE "${lead}(\./)?target/(debug|release)/delilah([[:space:]]|$)" <<<"$s" && ! grep -qE 'DELILAH_URL=' <<<"$s" \
    && block "the CLI without DELILAH_URL talks to 7420"
  grep -qE '\brm\b.*(~|\$HOME|/Users/[^/]+)/\.delilah([/[:space:]]|$)' <<<"$s" && block "deleting ~/.delilah"
}

# A DELILAH_URL exported earlier in the command covers the CLI calls after it.
exported=$(grep -oE "(export[[:space:]]+)?DELILAH_URL=[^[:space:];&|]+" <<<"$cmd" | tail -1)

while IFS= read -r stmt; do
  [ -n "$exported" ] && ! grep -qE "$installed" <<<"$exported" && stmt="DELILAH_URL= $stmt"
  check "$stmt"
done < <(sed -E 's/(&&|\|\||;)/\n/g' <<<"$cmd")

exit 0
