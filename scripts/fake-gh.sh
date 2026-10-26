#!/usr/bin/env bash
# A fake `gh` for trying the PR loop without GitHub: answers from JSON files in
# $FAKE_GH_DIR (repo.json, user.json, pr-list.json, pr-<n>.json). Use it with
# DELILAH_GH_BIN=$PWD/scripts/fake-gh.sh.
set -euo pipefail

dir="${FAKE_GH_DIR:?FAKE_GH_DIR is not set}"
args=("$@")

# The value after a flag, or empty.
flag() {
  local name=$1 i
  for ((i = 0; i < ${#args[@]} - 1; i++)); do
    [[ "${args[i]}" == "$name" ]] && { printf '%s' "${args[i + 1]}"; return; }
  done
}

case "${1:-} ${2:-}" in
  "repo view")
    cat "$dir/repo.json"
    ;;
  "api user")
    if [[ ! -f "$dir/user.json" ]]; then
      echo "You are not logged into any GitHub hosts. To log in, run: gh auth login" >&2
      exit 1
    fi
    if [[ " ${args[*]} " == *" --jq .login "* ]]; then
      python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["login"])' "$dir/user.json"
    else
      cat "$dir/user.json"
    fi
    ;;
  "pr list")
    cat "$dir/pr-list.json"
    ;;
  "pr view")
    n="${3:-}"
    n="${n%/}"
    n="${n##*/}"
    cat "$dir/pr-$n.json"
    ;;
  "pr create")
    body=$(cat)
    python3 -I - "$dir" "$(flag --head)" "$(flag --base)" "$(flag --title)" \
      "$([[ " ${args[*]} " == *" --draft "* ]] && echo 1 || echo 0)" "$body" <<'PY'
import json, os, sys
d, head, base, title, draft, body = sys.argv[1:7]
lp = os.path.join(d, "pr-list.json")
prs = json.load(open(lp)) if os.path.exists(lp) else []
n = max([p.get("number", 0) for p in prs], default=0) + 1
url = json.load(open(os.path.join(d, "repo.json")))["url"].rstrip("/") + "/pull/%d" % n
prs.append({"number": n, "url": url, "headRefName": head, "baseRefName": base,
            "title": title, "isDraft": draft == "1", "body": body, "state": "OPEN"})
json.dump(prs, open(lp, "w"), indent=2)
print(url)
PY
    ;;
  "pr merge")
    n="${args[${#args[@]} - 1]}"
    n="${n%/}"
    n="${n##*/}"
    python3 -I - "$dir" "$n" <<'PY'
import json, os, sys
d, n = sys.argv[1:3]
p = os.path.join(d, "pr-%s.json" % n)
doc = json.load(open(p))
pr = doc["data"]["repository"]["pullRequest"]
pr["state"] = "MERGED"
pr["merged"] = True
json.dump(doc, open(p, "w"), indent=2)
lp = os.path.join(d, "pr-list.json")
if os.path.exists(lp):
    prs = json.load(open(lp))
    for x in prs:
        if str(x.get("number")) == n:
            x["state"] = "MERGED"
    json.dump(prs, open(lp, "w"), indent=2)
PY
    ;;
  "api graphql")
    n=""
    for ((i = 0; i < ${#args[@]} - 1; i++)); do
      if [[ "${args[i]}" == "-F" && "${args[i + 1]}" == number=* ]]; then n="${args[i + 1]#number=}"; fi
    done
    cat "$dir/pr-$n.json"
    ;;
  *)
    echo "fake-gh: unsupported: $*" >&2
    exit 1
    ;;
esac
