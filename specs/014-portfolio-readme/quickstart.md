# Quickstart: verifying this feature

## Recompute the figures

From the repo root, on the revision being quoted:

```bash
git log --reverse --format=%ad --date=short | head -1; git log -1 --format=%ad --date=short
git log --format=%ad --date=short | sort -u | wc -l        # active days
git rev-list --count HEAD; git rev-list --count --merges HEAD; git tag | wc -l
loc() { git ls-files -- "$@" | grep -vE '(Cargo.lock|package-lock.json|\.png$|\.icns$|\.ico$|\.svg$|gen/)' | tr '\n' '\0' | xargs -0 cat | wc -l; }
for a in crates app/src-tauri/src app/src app/e2e scripts docs specs; do printf '%s: ' "$a"; loc "$a"; done
git ls-files specs | cut -d/ -f2 | grep -v README | sort -u | wc -l
grep -cE '^\*\*[0-9]+\.' docs/decisions.md
git ls-files 'crates/*.rs' 'app/src-tauri/*.rs' | tr '\n' '\0' | xargs -0 grep -hE '#\[(tokio::)?test' | wc -l
git ls-files 'app/src/*.test.ts' 'app/src/*.test.tsx' | tr '\n' '\0' | xargs -0 grep -hE '^\s*(it|test)(\.each\([^)]*\))?\(' | wc -l
git ls-files 'app/e2e/*.spec.ts' | tr '\n' '\0' | xargs -0 grep -hE '^\s*test(\.[a-z]+)?\(' | wc -l
ls crates/delilah-daemon/migrations | wc -l
```

## Generate the captures

Never against the installed app: the spec starts its own daemon on a free port.

```bash
cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e
cd app && DELILAH_SCREENS=despues DELILAH_SCREENS_LANG=en DELILAH_SCREENS_ONLY=Playground \
  DELILAH_SCREENS_OUT=../docs/images npx playwright test screens
```

## Record the GIF by hand (fallback)

1. Run `scripts/preview.sh --fresh`, register a fixture project and start a few tasks.
2. Open the Playground in the printed UI URL, in English, with real tasks only from fixture repos.
3. Record the window with ⇧⌘5 for 10-15 seconds.
4. Convert it to a GIF under 5 MB (for example `gifski`, after `brew install gifski`), save it as `docs/images/playground.gif`, and swap it into the README's hero.

## Checks before the PR

```bash
wc -l README.md                                          # about 200 at most
git diff --stat main -- scripts .github/workflows crates app/src app/src-tauri     # empty
git diff --stat main -- .github/dependabot.yml           # empty unless approved
git grep -n -E 'README|newcomer' -- 'docs/*.md' CLAUDE.md ':!docs/sessions' ':!docs/archive'   # nothing still calls it a newcomer page
```

- Every relative link and anchor in `README.md`, `CONTRIBUTING.md` and the new `docs/usage.md` sections resolves.
- On the GitHub page of the branch: the Mermaid diagram renders and the hero and diagram read well in light and dark.
- Screenshots show only demo projects: no real repository, company or ticket names.
